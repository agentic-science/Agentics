use std::io::{Cursor, Write};

use agentics_domain::models::paths::BundleRelativePath;

use super::private_assets::{
    extract_private_asset_overlay_blocking, validate_private_asset_zip_upload,
};

/// Builds a small in-memory ZIP archive for private asset extraction tests.
fn zip_with_file(path: &str, content: &[u8]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        archive
            .start_file(path, options)
            .expect("test ZIP path should start");
        archive
            .write_all(content)
            .expect("test ZIP content should write");
        archive.finish().expect("test ZIP should finish");
    }
    cursor.into_inner()
}

/// Rejects traversal-like private asset entries instead of silently skipping them.
#[test]
fn private_asset_overlay_rejects_unsafe_zip_entry_path() {
    let target = std::env::temp_dir().join(format!(
        "agentics-private-asset-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&target).expect("target tempdir");
    let escape_target = target.join("escape.txt");
    let bytes = zip_with_file("../escape.txt", b"escape");

    let error = extract_private_asset_overlay_blocking(&bytes, &target, "official-cases", 1024)
        .expect_err("unsafe ZIP path should fail extraction");

    assert!(error.to_string().contains("contains unsafe path"));
    assert!(
        !escape_target.exists(),
        "unsafe private asset extraction must not write outside the bundle"
    );
    std::fs::remove_dir_all(&target).expect("target tempdir cleanup");
}

/// Verifies required paths must be contributed by the uploaded private ZIP itself.
#[tokio::test]
async fn private_asset_zip_must_include_declared_required_paths() {
    let bytes = zip_with_file("private-benchmark/other.txt", b"hidden");
    let required =
        vec![BundleRelativePath::try_new("private-benchmark/runs.json").expect("required path")];

    let error = validate_private_asset_zip_upload(&bytes, "official-cases", &required, 1024)
        .await
        .expect_err("missing required path should fail");

    assert!(error.to_string().contains("required runtime path"));
}

/// Verifies required directory paths may be satisfied by files beneath that directory.
#[tokio::test]
async fn private_asset_zip_accepts_required_directory_contribution() {
    let bytes = zip_with_file("private-benchmark/cases/case-1.json", b"{}");
    let required =
        vec![BundleRelativePath::try_new("private-benchmark/cases").expect("required path")];

    validate_private_asset_zip_upload(&bytes, "official-cases", &required, 1024)
        .await
        .expect("child file should satisfy required directory path");
}
