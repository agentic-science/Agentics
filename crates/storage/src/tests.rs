use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

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
async fn local_storage_concurrent_put_preserves_one_writer() {
    let root = temp_storage_root("concurrent-put");
    let storage = LocalStorage::new(&root);
    let key = storage_key("objects/value.txt");

    let first = {
        let storage = storage.clone();
        let key = key.clone();
        tokio::spawn(async move { storage.put(&key, b"first", TEST_INTENT).await })
    };
    let second = {
        let storage = storage.clone();
        let key = key.clone();
        tokio::spawn(async move { storage.put(&key, b"second", TEST_INTENT).await })
    };

    let (first, second) = tokio::join!(first, second);
    let outcomes = [first.expect("first task"), second.expect("second task")];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| matches!(result, Err(StorageError::ObjectConflict(_))))
            .count(),
        1
    );
}

#[tokio::test]
async fn local_storage_concurrent_promote_preserves_one_writer() {
    let root = temp_storage_root("concurrent-promote");
    let storage = LocalStorage::new(&root);
    let first_temp = storage_key("_tmp/first.txt");
    let second_temp = storage_key("_tmp/second.txt");
    let durable = storage_key("objects/value.txt");
    storage
        .put(&first_temp, b"first", TEST_INTENT)
        .await
        .expect("put first temp");
    storage
        .put(&second_temp, b"second", TEST_INTENT)
        .await
        .expect("put second temp");

    let first = {
        let storage = storage.clone();
        let first_temp = first_temp.clone();
        let durable = durable.clone();
        tokio::spawn(async move { storage.promote(&first_temp, &durable).await })
    };
    let second = {
        let storage = storage.clone();
        let second_temp = second_temp.clone();
        let durable = durable.clone();
        tokio::spawn(async move { storage.promote(&second_temp, &durable).await })
    };

    let (first, second) = tokio::join!(first, second);
    let outcomes = [first.expect("first task"), second.expect("second task")];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| matches!(result, Err(StorageError::ObjectConflict(_))))
            .count(),
        1
    );
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

#[tokio::test]
async fn local_storage_delete_prefix_does_not_touch_sibling_prefixes() {
    let root = temp_storage_root("prefix-boundary");
    let storage = LocalStorage::new(&root);
    storage
        .put(&storage_key("_tmp/a.txt"), b"a", TEST_INTENT)
        .await
        .expect("put tmp");
    storage
        .put(&storage_key("_tmpfoo/b.txt"), b"b", TEST_INTENT)
        .await
        .expect("put sibling");

    assert_eq!(
        storage
            .delete_prefix(&storage_key("_tmp"))
            .await
            .expect("delete tmp prefix"),
        1
    );

    assert!(
        !storage
            .exists(&storage_key("_tmp/a.txt"))
            .await
            .expect("tmp exists")
    );
    assert!(
        storage
            .exists(&storage_key("_tmpfoo/b.txt"))
            .await
            .expect("sibling exists")
    );
}

#[tokio::test]
async fn local_storage_deletes_only_stale_prefix_objects() {
    let root = temp_storage_root("stale-prefix");
    let storage = LocalStorage::new(&root);
    storage
        .put(&storage_key("_tmp/stale.txt"), b"a", TEST_INTENT)
        .await
        .expect("put stale candidate");
    storage
        .put(&storage_key("_tmpfoo/sibling.txt"), b"b", TEST_INTENT)
        .await
        .expect("put sibling");

    let future_cutoff = SystemTime::now() + Duration::from_secs(60);
    assert_eq!(
        storage
            .delete_prefix_older_than(&storage_key("_tmp"), future_cutoff)
            .await
            .expect("delete stale tmp prefix"),
        1
    );

    assert!(
        !storage
            .exists(&storage_key("_tmp/stale.txt"))
            .await
            .expect("stale exists")
    );
    assert!(
        storage
            .exists(&storage_key("_tmpfoo/sibling.txt"))
            .await
            .expect("sibling exists")
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

    super::pack_directory_to_tar(
        &source,
        &archive,
        StorageWriteIntent::new("test bundle", 64 * 1024),
    )
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
async fn bundle_tar_unpack_rejects_traversal_entries() {
    let root = temp_storage_root("tar-traversal");
    let archive = root.join("bad.tar");
    write_raw_tar_file_entry(&archive, "../escape.txt", b"bad");

    let error = super::unpack_tar_to_directory(&archive, &root.join("destination"))
        .await
        .expect_err("traversal tar entry should fail");

    assert!(matches!(error, StorageError::InvalidKey(_)));
    assert!(!root.join("escape.txt").exists());
}

#[tokio::test]
async fn bundle_tar_unpack_rejects_absolute_entries() {
    let root = temp_storage_root("tar-absolute");
    let archive = root.join("bad.tar");
    write_raw_tar_file_entry(&archive, "/tmp/escape.txt", b"bad");

    let error = super::unpack_tar_to_directory(&archive, &root.join("destination"))
        .await
        .expect_err("absolute tar entry should fail");

    assert!(matches!(error, StorageError::InvalidKey(_)));
}

#[tokio::test]
async fn bundle_tar_unpack_rejects_symlink_entries() {
    let root = temp_storage_root("tar-symlink");
    let archive = root.join("bad.tar");
    let file = fs::File::create(&archive).expect("archive");
    let mut builder = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_cksum();
    builder
        .append_link(&mut header, "link.txt", "target.txt")
        .expect("append symlink");
    builder.finish().expect("finish tar");

    let error = super::unpack_tar_to_directory(&archive, &root.join("destination"))
        .await
        .expect_err("symlink tar entry should fail");

    assert!(matches!(error, StorageError::InvalidKey(_)));
}

#[tokio::test]
async fn bundle_tar_unpack_rejects_hardlink_entries() {
    let root = temp_storage_root("tar-hardlink");
    let archive = root.join("bad.tar");
    let file = fs::File::create(&archive).expect("archive");
    let mut builder = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Link);
    header.set_size(0);
    header.set_cksum();
    builder
        .append_link(&mut header, "link.txt", "target.txt")
        .expect("append hardlink");
    builder.finish().expect("finish tar");

    let error = super::unpack_tar_to_directory(&archive, &root.join("destination"))
        .await
        .expect_err("hardlink tar entry should fail");

    assert!(matches!(error, StorageError::InvalidKey(_)));
}

#[tokio::test]
async fn bundle_tar_unpack_rejects_unsupported_entries() {
    let root = temp_storage_root("tar-unsupported");
    let archive = root.join("bad.tar");
    let file = fs::File::create(&archive).expect("archive");
    let mut builder = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Fifo);
    header.set_size(0);
    header.set_cksum();
    builder
        .append_data(&mut header, "pipe", &[][..])
        .expect("append fifo");
    builder.finish().expect("finish tar");

    let error = super::unpack_tar_to_directory(&archive, &root.join("destination"))
        .await
        .expect_err("unsupported tar entry should fail");

    assert!(matches!(error, StorageError::InvalidKey(_)));
}

#[tokio::test]
async fn bundle_tar_unpack_rejects_existing_destination_files() {
    let root = temp_storage_root("tar-existing");
    let archive = root.join("bundle.tar");
    let destination = root.join("destination");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(destination.join("statement.md"), "old").expect("existing file");
    write_test_tar(&archive, &[("statement.md", b"new".as_slice(), false)]);

    super::unpack_tar_to_directory(&archive, &destination)
        .await
        .expect_err("existing destination file should fail");

    assert_eq!(
        fs::read_to_string(destination.join("statement.md")).expect("statement"),
        "old"
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
    let sibling_key = storage_key("objects2/value.txt");
    let temp_key = storage_key("_tmp/promote.txt");
    let durable_key = storage_key("objects/promoted.txt");

    storage
        .put(&key, b"hello", TEST_INTENT)
        .await
        .expect("S3 put should work");
    storage
        .put(&sibling_key, b"sibling", TEST_INTENT)
        .await
        .expect("S3 sibling put should work");
    assert_eq!(
        storage.get(&key, TEST_INTENT).await.expect("S3 get"),
        b"hello"
    );
    assert!(matches!(
        storage.put(&key, b"again", TEST_INTENT).await,
        Err(StorageError::ObjectConflict(_))
    ));
    let concurrent_key = storage_key("objects/concurrent.txt");
    let first_put = {
        let storage = storage.clone();
        let concurrent_key = concurrent_key.clone();
        tokio::spawn(async move { storage.put(&concurrent_key, b"first", TEST_INTENT).await })
    };
    let second_put = {
        let storage = storage.clone();
        let concurrent_key = concurrent_key.clone();
        tokio::spawn(async move { storage.put(&concurrent_key, b"second", TEST_INTENT).await })
    };
    let (first_put, second_put) = tokio::join!(first_put, second_put);
    let put_outcomes = [
        first_put.expect("first put task"),
        second_put.expect("second put task"),
    ];
    assert_eq!(
        put_outcomes.iter().filter(|result| result.is_ok()).count(),
        1
    );
    assert_eq!(
        put_outcomes
            .iter()
            .filter(|result| matches!(result, Err(StorageError::ObjectConflict(_))))
            .count(),
        1
    );

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
    let first_promote_temp = storage_key("_tmp/promote-first.txt");
    let second_promote_temp = storage_key("_tmp/promote-second.txt");
    let concurrent_durable = storage_key("objects/promoted-concurrent.txt");
    storage
        .put(&first_promote_temp, b"first", TEST_INTENT)
        .await
        .expect("S3 first temp put");
    storage
        .put(&second_promote_temp, b"second", TEST_INTENT)
        .await
        .expect("S3 second temp put");
    let first_promote = {
        let storage = storage.clone();
        let first_promote_temp = first_promote_temp.clone();
        let concurrent_durable = concurrent_durable.clone();
        tokio::spawn(async move {
            storage
                .promote(&first_promote_temp, &concurrent_durable)
                .await
        })
    };
    let second_promote = {
        let storage = storage.clone();
        let second_promote_temp = second_promote_temp.clone();
        let concurrent_durable = concurrent_durable.clone();
        tokio::spawn(async move {
            storage
                .promote(&second_promote_temp, &concurrent_durable)
                .await
        })
    };
    let (first_promote, second_promote) = tokio::join!(first_promote, second_promote);
    let promote_outcomes = [
        first_promote.expect("first promote task"),
        second_promote.expect("second promote task"),
    ];
    assert_eq!(
        promote_outcomes
            .iter()
            .filter(|result| result.is_ok())
            .count(),
        1
    );
    assert_eq!(
        promote_outcomes
            .iter()
            .filter(|result| matches!(result, Err(StorageError::ObjectConflict(_))))
            .count(),
        1
    );

    assert_eq!(
        storage
            .delete_prefix(&storage_key("objects"))
            .await
            .expect("S3 delete prefix"),
        4
    );
    assert_eq!(
        storage
            .get(&sibling_key, TEST_INTENT)
            .await
            .expect("S3 sibling key survives prefix delete"),
        b"sibling"
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

fn write_test_tar(path: &std::path::Path, entries: &[(&str, &[u8], bool)]) {
    let file = fs::File::create(path).expect("archive");
    let mut builder = tar::Builder::new(file);
    for (name, bytes, is_dir) in entries {
        let mut header = tar::Header::new_gnu();
        if *is_dir {
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_cksum();
            builder
                .append_data(&mut header, name, &[][..])
                .expect("append dir");
        } else {
            header.set_size(u64::try_from(bytes.len()).expect("test entry fits u64"));
            header.set_cksum();
            builder
                .append_data(&mut header, name, *bytes)
                .expect("append file");
        }
    }
    builder.finish().expect("finish tar");
}

fn write_raw_tar_file_entry(path: &std::path::Path, entry_name: &str, bytes: &[u8]) {
    let mut file = fs::File::create(path).expect("archive");
    let mut header = [0u8; 512];
    let name = entry_name.as_bytes();
    assert!(name.len() <= 100, "test tar path must fit ustar header");
    header[..name.len()].copy_from_slice(name);
    write_octal(&mut header[100..108], 0o644);
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    write_octal(
        &mut header[124..136],
        u64::try_from(bytes.len()).expect("test bytes fit u64"),
    );
    write_octal(&mut header[136..148], 0);
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    write_checksum(&mut header[148..156], checksum);
    file.write_all(&header).expect("write header");
    file.write_all(bytes).expect("write bytes");
    let padding = (512 - (bytes.len() % 512)) % 512;
    if padding > 0 {
        file.write_all(&vec![0u8; padding]).expect("write padding");
    }
    file.write_all(&[0u8; 1024]).expect("write tar trailer");
}

fn write_octal(field: &mut [u8], value: u64) {
    let width = field.len();
    let text = format!("{value:0width$o}", width = width - 1);
    field[..width - 1].copy_from_slice(text.as_bytes());
    field[width - 1] = 0;
}

fn write_checksum(field: &mut [u8], value: u32) {
    let text = format!("{value:06o}");
    field[..6].copy_from_slice(text.as_bytes());
    field[6] = 0;
    field[7] = b' ';
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
