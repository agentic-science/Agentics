use std::io::{Cursor, Write};

use super::private_assets::extract_private_asset_overlay_blocking;

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
