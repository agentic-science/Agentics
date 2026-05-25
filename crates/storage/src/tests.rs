use std::fs;
use std::path::PathBuf;

use super::{
    LocalStorage, S3Storage, S3StorageOptions, Storage, StorageError, StorageKey,
    StorageWriteIntent,
};

const TEST_INTENT: StorageWriteIntent = StorageWriteIntent::new("test object", 1024);

#[tokio::test]
async fn local_storage_round_trips_relative_keys() {
    let root = temp_storage_root("relative-keys");
    let storage = LocalStorage::new(&root);
    let key = storage_key("foo/bar.txt");

    let stored = storage
        .put(&key, b"hello", TEST_INTENT)
        .await
        .expect("put should succeed");

    assert_eq!(stored, key);
    assert_eq!(
        storage
            .get(&key, TEST_INTENT)
            .await
            .expect("get should succeed"),
        b"hello"
    );
}

#[tokio::test]
async fn local_storage_rejects_oversized_objects() {
    let root = temp_storage_root("oversized");
    let storage = LocalStorage::new(&root);
    let key = storage_key("too-big.txt");
    let error = storage
        .put(&key, b"hello", StorageWriteIntent::new("tiny", 1))
        .await
        .expect_err("oversized object should fail");

    assert!(matches!(error, StorageError::ObjectTooLarge { .. }));
    assert!(!storage.exists(&key).await.expect("exists should work"));
}

#[tokio::test]
async fn local_storage_rejects_conflicts_and_promotes_temp_objects() {
    let root = temp_storage_root("promote");
    let storage = LocalStorage::new(&root);
    let temp = storage_key("_tmp/object.txt");
    let durable = storage_key("objects/object.txt");
    storage
        .put(&temp, b"hello", TEST_INTENT)
        .await
        .expect("put temp");

    storage
        .promote(&temp, &durable)
        .await
        .expect("promote should work");

    assert!(!storage.exists(&temp).await.expect("temp exists check"));
    assert_eq!(
        storage
            .get(&durable, TEST_INTENT)
            .await
            .expect("durable get"),
        b"hello"
    );
    assert!(storage.put(&durable, b"again", TEST_INTENT).await.is_err());
}

#[tokio::test]
async fn local_storage_lists_prefixes() {
    let root = temp_storage_root("list");
    let storage = LocalStorage::new(&root);
    storage
        .put(&storage_key("_tmp/a.txt"), b"a", TEST_INTENT)
        .await
        .expect("put a");
    storage
        .put(&storage_key("_tmp/nested/b.txt"), b"b", TEST_INTENT)
        .await
        .expect("put b");

    let keys = storage
        .list_prefix(&storage_key("_tmp"))
        .await
        .expect("list should work");

    assert_eq!(
        keys,
        vec![storage_key("_tmp/a.txt"), storage_key("_tmp/nested/b.txt")]
    );
    assert_eq!(
        storage
            .delete_prefix(&storage_key("_tmp"))
            .await
            .expect("delete prefix should work"),
        2
    );
    assert!(
        storage
            .list_prefix(&storage_key("_tmp"))
            .await
            .expect("list after delete should work")
            .is_empty()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn local_storage_rejects_symlink_prefixes() {
    use std::os::unix::fs::symlink;

    let root = temp_storage_root("symlink-root");
    let outside = temp_storage_root("symlink-outside");
    fs::create_dir_all(&root).expect("root dir");
    symlink(&outside, root.join("link")).expect("symlink");
    let storage = LocalStorage::new(&root);

    let result = storage
        .put(&storage_key("link/escape.txt"), b"bad", TEST_INTENT)
        .await;

    assert!(matches!(result, Err(StorageError::SymlinkRejected(_))));
}

#[tokio::test]
async fn bundle_tar_round_trips_without_overwrite() {
    let root = temp_storage_root("bundle-tar");
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir_all(source.join("nested")).expect("source dirs");
    fs::write(source.join("statement.md"), "statement").expect("statement");
    fs::write(source.join("nested/data.txt"), "data").expect("data");
    let archive = root.join("bundle.tar");

    super::pack_directory_to_tar(&source, &archive)
        .await
        .expect("pack");
    super::unpack_tar_to_directory(&archive, &destination)
        .await
        .expect("unpack");

    assert_eq!(
        fs::read_to_string(destination.join("statement.md")).expect("statement"),
        "statement"
    );
    assert_eq!(
        fs::read_to_string(destination.join("nested/data.txt")).expect("data"),
        "data"
    );
}

#[tokio::test]
async fn rustfs_s3_storage_round_trips_when_configured() {
    let Some(storage) = rustfs_storage_from_env().await else {
        eprintln!(
            "skipping RustFS S3 storage test: set AGENTICS_S3_TEST_ENDPOINT and AGENTICS_S3_TEST_BUCKET"
        );
        return;
    };
    let key = storage_key("objects/value.txt");
    let temp_key = storage_key("_tmp/promote.txt");
    let durable_key = storage_key("objects/promoted.txt");

    storage
        .put(&key, b"hello", TEST_INTENT)
        .await
        .expect("S3 put should work");
    assert_eq!(
        storage.get(&key, TEST_INTENT).await.expect("S3 get"),
        b"hello"
    );
    assert!(matches!(
        storage.put(&key, b"again", TEST_INTENT).await,
        Err(StorageError::ObjectConflict(_))
    ));

    let oversized_key = storage_key("objects/oversized.txt");
    let error = storage
        .put(&oversized_key, b"hello", StorageWriteIntent::new("tiny", 1))
        .await
        .expect_err("oversized S3 put should fail before upload");
    assert!(matches!(error, StorageError::ObjectTooLarge { .. }));
    assert!(
        !storage
            .exists(&oversized_key)
            .await
            .expect("oversized key exists check")
    );

    storage
        .put(&temp_key, b"promoted", TEST_INTENT)
        .await
        .expect("S3 temp put");
    storage
        .promote(&temp_key, &durable_key)
        .await
        .expect("S3 promote");
    assert!(
        !storage
            .exists(&temp_key)
            .await
            .expect("temp key exists check")
    );
    assert_eq!(
        storage
            .get(&durable_key, TEST_INTENT)
            .await
            .expect("promoted get"),
        b"promoted"
    );

    assert_eq!(
        storage
            .delete_prefix(&storage_key("objects"))
            .await
            .expect("S3 delete prefix"),
        2
    );
}

fn storage_key(value: &str) -> StorageKey {
    StorageKey::try_new(value).expect("test storage key is valid")
}

fn temp_storage_root(label: &str) -> PathBuf {
    tempfile::Builder::new()
        .prefix(&format!("agentics-storage-{label}-"))
        .tempdir()
        .expect("tempdir")
        .keep()
}

async fn rustfs_storage_from_env() -> Option<S3Storage> {
    let endpoint = std::env::var("AGENTICS_S3_TEST_ENDPOINT").ok()?;
    let bucket = std::env::var("AGENTICS_S3_TEST_BUCKET").ok()?;
    let region =
        std::env::var("AGENTICS_S3_TEST_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let force_path_style = std::env::var("AGENTICS_S3_FORCE_PATH_STYLE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(true);
    let prefix = format!("agentics-test-{}", uuid::Uuid::new_v4());
    let storage = S3Storage::from_options(S3StorageOptions {
        bucket,
        prefix: Some(prefix),
        region,
        endpoint_url: Some(endpoint.parse().expect("valid RustFS endpoint URL")),
        force_path_style,
    })
    .await
    .expect("RustFS S3 storage should initialize");
    storage
        .create_bucket_if_missing_for_tests()
        .await
        .expect("RustFS test bucket should be available");
    Some(storage)
}
