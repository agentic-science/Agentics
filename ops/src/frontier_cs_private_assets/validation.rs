use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use agentics_contracts::challenge_bundle::{copy_challenge_bundle_dir, validate_challenge_bundle};
use agentics_domain::storage::StorageKey;
use anyhow::{Context, anyhow};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

pub(super) async fn validate_generated_zip(
    agentics_challenges_root: &Path,
    challenge_name: &str,
    zip_bytes: &[u8],
    required_paths: &[String],
) -> anyhow::Result<()> {
    let entries = inspect_zip(zip_bytes)?;
    for required_path in required_paths {
        if !entries.contains(required_path) {
            anyhow::bail!("generated ZIP is missing `{required_path}`");
        }
    }

    let temp = TempDir::new().context("failed to create temp overlay validation dir")?;
    let source_bundle = agentics_challenges_root
        .join("challenges")
        .join(challenge_name)
        .join("v1");
    let temp_bundle = temp.path().join("bundle");
    copy_challenge_bundle_dir(&source_bundle, &temp_bundle, false).await?;
    extract_zip(zip_bytes, &temp_bundle)?;
    validate_challenge_bundle(&temp_bundle).await?;
    Ok(())
}

fn inspect_zip(zip_bytes: &[u8]) -> anyhow::Result<HashSet<String>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    let mut entries = HashSet::new();
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        reject_zip_entry(&file)?;
        let name = file.name().to_string();
        if !entries.insert(name.clone()) {
            anyhow::bail!("generated ZIP contains duplicate entry `{name}`");
        }
    }
    Ok(entries)
}

fn extract_zip(zip_bytes: &[u8], destination: &Path) -> anyhow::Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        reject_zip_entry(&file)?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| anyhow!("generated ZIP entry `{}` is unsafe", file.name()))?;
        let destination_path = destination.join(enclosed);
        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination_path)
            .with_context(|| format!("failed to create {}", destination_path.display()))?;
        std::io::copy(&mut file, &mut output)?;
    }
    Ok(())
}

fn reject_zip_entry<R: Read>(file: &zip::read::ZipFile<'_, R>) -> anyhow::Result<()> {
    let name = file.name();
    StorageKey::try_new(name).map_err(|error| anyhow!("unsafe ZIP entry `{name}`: {error}"))?;
    if file.enclosed_name().is_none() {
        anyhow::bail!("unsafe ZIP entry `{name}`");
    }
    if file
        .unix_mode()
        .is_some_and(|mode| mode & 0o170000 == 0o120000)
    {
        anyhow::bail!("generated ZIP entry `{name}` is a symlink");
    }
    Ok(())
}

pub(super) fn build_zip(entries: &BTreeMap<String, Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(&mut cursor);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o600);
    for (name, bytes) in entries {
        StorageKey::try_new(name).map_err(|error| anyhow!("unsafe ZIP entry `{name}`: {error}"))?;
        archive.start_file(name, options)?;
        archive.write_all(bytes)?;
    }
    let _ = archive.finish()?;
    Ok(cursor.into_inner())
}
