//! Shared ZIP archive envelope validation and safe extraction helpers.

use std::collections::HashSet;
use std::io::{Read, Seek};
use std::path::{Component, Path};

use crate::error::{Result, ServiceError};

/// Local challenge/archive validation failures before service-boundary mapping.
#[derive(Debug, thiserror::Error)]
pub enum ChallengeValidationError {
    #[error("archive traversal rejected: {0}")]
    ArchiveTraversal(String),
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("unsafe path rejected: {0}")]
    UnsafePath(String),
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
}

impl From<ChallengeValidationError> for ServiceError {
    fn from(error: ChallengeValidationError) -> Self {
        ServiceError::Validation(error.to_string())
    }
}

/// ZIP archive envelope policy for one external contract.
#[derive(Debug, Clone)]
pub struct ArchiveEnvelopePolicy {
    label: String,
    max_archive_bytes: u64,
    max_entries: usize,
    max_expanded_bytes: u64,
    reject_symlinks: bool,
}

impl ArchiveEnvelopePolicy {
    /// Build a policy with the default hostile-archive safety checks enabled.
    pub fn new(
        label: impl Into<String>,
        max_archive_bytes: u64,
        max_entries: usize,
        max_expanded_bytes: u64,
    ) -> Self {
        Self {
            label: label.into(),
            max_archive_bytes,
            max_entries,
            max_expanded_bytes,
            reject_symlinks: true,
        }
    }

    /// Borrow the user-facing archive label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Maximum compressed archive size in bytes.
    pub fn max_archive_bytes(&self) -> u64 {
        self.max_archive_bytes
    }

    /// Maximum entry count.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Maximum total expanded entry bytes.
    pub fn max_expanded_bytes(&self) -> u64 {
        self.max_expanded_bytes
    }
}

/// A normalized path inside a ZIP archive.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalizedArchivePath(String);

impl NormalizedArchivePath {
    /// Normalize and validate an archive entry path.
    pub fn try_new(raw: &str, label: &str) -> Result<Self> {
        if raw.is_empty() || raw.contains('\0') || raw.starts_with('/') || raw.starts_with('\\') {
            return Err(ChallengeValidationError::ArchiveTraversal(format!(
                "{label} contains an unsafe ZIP entry path",
            ))
            .into());
        }

        let trimmed = raw.trim_matches(['/', '\\']);
        if trimmed.is_empty() {
            return Err(ChallengeValidationError::ArchiveTraversal(format!(
                "{label} contains an unsafe ZIP entry path",
            ))
            .into());
        }

        let mut parts = Vec::new();
        for part in trimmed.split(['/', '\\']) {
            if part.is_empty() || part == "." || part == ".." {
                return Err(ChallengeValidationError::UnsafePath(format!(
                    "{label} contains unsafe path `{raw}`",
                ))
                .into());
            }
            parts.push(part);
        }

        Ok(Self(parts.join("/")))
    }

    /// Normalize a trusted local relative path into archive wire form.
    pub fn from_relative_path(path: &Path, label: &str) -> Result<Self> {
        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(value) => {
                    let value = value.to_str().ok_or_else(|| {
                        ServiceError::Validation(format!(
                            "{label} contains a path that is not valid UTF-8: {}",
                            path.display()
                        ))
                    })?;
                    parts.push(value);
                }
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(ChallengeValidationError::UnsafePath(format!(
                        "{label} contains unsafe path `{}`",
                        path.display(),
                    ))
                    .into());
                }
            }
        }

        Self::try_new(&parts.join("/"), label)
    }

    /// Borrow the canonical ZIP path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Borrow as a relative filesystem path for safe joins under a controlled root.
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl std::fmt::Display for NormalizedArchivePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Validated metadata for one archive entry.
#[derive(Debug, Clone)]
pub struct ArchiveEnvelopeEntry {
    index: usize,
    path: NormalizedArchivePath,
    is_dir: bool,
    size: u64,
    compressed_size: u64,
}

impl ArchiveEnvelopeEntry {
    /// Entry index in the ZIP central directory.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Normalized relative archive path.
    pub fn path(&self) -> &NormalizedArchivePath {
        &self.path
    }

    /// Whether the entry is a directory.
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Expanded entry size in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Compressed entry size in bytes.
    pub fn compressed_size(&self) -> u64 {
        self.compressed_size
    }
}

/// Validated archive envelope summary.
#[derive(Debug, Clone)]
pub struct ArchiveEnvelope {
    label: String,
    archive_size: u64,
    expanded_size: u64,
    entries: Vec<ArchiveEnvelopeEntry>,
}

impl ArchiveEnvelope {
    /// User-facing archive label from the policy that produced this envelope.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Compressed archive size in bytes.
    pub fn archive_size(&self) -> u64 {
        self.archive_size
    }

    /// Total expanded entry bytes.
    pub fn expanded_size(&self) -> u64 {
        self.expanded_size
    }

    /// Validated archive entries in central-directory order.
    pub fn entries(&self) -> &[ArchiveEnvelopeEntry] {
        &self.entries
    }
}

/// Validate a ZIP archive already loaded in memory.
pub fn inspect_zip_bytes(bytes: &[u8], policy: &ArchiveEnvelopePolicy) -> Result<ArchiveEnvelope> {
    let archive_size = u64::try_from(bytes.len())
        .map_err(|_| ServiceError::Validation(format!("{} is too large", policy.label())))?;
    ensure_archive_size(archive_size, policy)?;
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)?;
    inspect_zip_archive(archive_size, &mut archive, policy)
}

/// Validate a ZIP archive on disk.
pub fn inspect_zip_file(path: &Path, policy: &ArchiveEnvelopePolicy) -> Result<ArchiveEnvelope> {
    let archive_size = std::fs::metadata(path)?.len();
    ensure_archive_size(archive_size, policy)?;
    let reader = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(reader)?;
    inspect_zip_archive(archive_size, &mut archive, policy)
}

/// Validate and safely extract a ZIP archive under `target_dir`.
pub fn extract_zip_file_to_dir(
    archive_path: &Path,
    target_dir: &Path,
    policy: &ArchiveEnvelopePolicy,
) -> Result<()> {
    let archive_size = std::fs::metadata(archive_path)?.len();
    ensure_archive_size(archive_size, policy)?;
    let reader = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(reader)?;
    let envelope = inspect_zip_archive(archive_size, &mut archive, policy)?;
    extract_validated_zip_archive(&mut archive, &envelope, target_dir)
}

/// Validate and safely extract an in-memory ZIP archive under `target_dir`.
pub fn extract_zip_bytes_to_dir(
    bytes: &[u8],
    target_dir: &Path,
    policy: &ArchiveEnvelopePolicy,
) -> Result<()> {
    let archive_size = u64::try_from(bytes.len())
        .map_err(|_| ServiceError::Validation(format!("{} is too large", policy.label())))?;
    ensure_archive_size(archive_size, policy)?;
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)?;
    let envelope = inspect_zip_archive(archive_size, &mut archive, policy)?;
    extract_validated_zip_archive(&mut archive, &envelope, target_dir)
}

/// Extract entries that were already validated from the same archive object.
fn extract_validated_zip_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    envelope: &ArchiveEnvelope,
    target_dir: &Path,
) -> Result<()> {
    for entry in envelope.entries() {
        let mut file = archive.by_index(entry.index())?;
        let outpath = target_dir.join(entry.path().as_path());

        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if outpath.exists() {
                return Err(ServiceError::Validation(format!(
                    "{} cannot overwrite existing path `{}`",
                    envelope.label(),
                    entry.path()
                )));
            }
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(())
}

/// Validate archive size against policy.
fn ensure_archive_size(archive_size: u64, policy: &ArchiveEnvelopePolicy) -> Result<()> {
    if archive_size > policy.max_archive_bytes() {
        return Err(ServiceError::Validation(format!(
            "{} must be at most {} bytes",
            policy.label(),
            policy.max_archive_bytes()
        )));
    }
    Ok(())
}

/// Inspect a ZIP archive without extracting it.
fn inspect_zip_archive<R: Read + Seek>(
    archive_size: u64,
    archive: &mut zip::ZipArchive<R>,
    policy: &ArchiveEnvelopePolicy,
) -> Result<ArchiveEnvelope> {
    if archive.len() > policy.max_entries() {
        return Err(ServiceError::Validation(format!(
            "{} must contain at most {} entries",
            policy.label(),
            policy.max_entries()
        )));
    }

    let mut expanded_size = 0u64;
    let mut seen_paths = HashSet::with_capacity(archive.len());
    let mut entries = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        let file = archive.by_index(index)?;
        if policy.reject_symlinks
            && file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(ServiceError::Validation(format!(
                "{} must not contain symlinks",
                policy.label()
            )));
        }

        let path = NormalizedArchivePath::try_new(file.name(), policy.label())?;
        if !seen_paths.insert(path.clone()) {
            return Err(ServiceError::Validation(format!(
                "{} contains duplicate path `{path}`",
                policy.label()
            )));
        }

        expanded_size = expanded_size
            .checked_add(file.size())
            .ok_or_else(|| ServiceError::Validation(format!("{} is too large", policy.label())))?;
        if expanded_size > policy.max_expanded_bytes() {
            return Err(ServiceError::Validation(format!(
                "{} must expand to at most {} bytes",
                policy.label(),
                policy.max_expanded_bytes()
            )));
        }

        entries.push(ArchiveEnvelopeEntry {
            index,
            path,
            is_dir: file.is_dir(),
            size: file.size(),
            compressed_size: file.compressed_size(),
        });
    }

    Ok(ArchiveEnvelope {
        label: policy.label().to_string(),
        archive_size,
        expanded_size,
        entries,
    })
}

/// Test helpers for hand-built ZIP payloads.
#[cfg(test)]
pub(crate) mod test_support {
    use std::io::Write;

    /// Build a stored ZIP archive with explicit Unix mode bits.
    pub(crate) fn raw_stored_zip(entries: Vec<(&str, &[u8], u32)>) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut central_directory = Vec::new();
        let entry_count = u16::try_from(entries.len()).expect("test ZIP entries fit u16");

        for (name, content, unix_mode) in entries {
            let local_header_offset =
                u32::try_from(bytes.len()).expect("test ZIP should fit u32 offsets");
            let name_bytes = name.as_bytes();
            let name_len = u16::try_from(name_bytes.len()).expect("test ZIP names are short");
            let content_len =
                u32::try_from(content.len()).expect("test ZIP content should fit u32");

            bytes.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            bytes.extend_from_slice(&20u16.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(&0u32.to_le_bytes());
            bytes.extend_from_slice(&content_len.to_le_bytes());
            bytes.extend_from_slice(&content_len.to_le_bytes());
            bytes.extend_from_slice(&name_len.to_le_bytes());
            bytes.extend_from_slice(&0u16.to_le_bytes());
            bytes.extend_from_slice(name_bytes);
            bytes.extend_from_slice(content);

            central_directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central_directory.extend_from_slice(&20u16.to_le_bytes());
            central_directory.extend_from_slice(&20u16.to_le_bytes());
            central_directory.extend_from_slice(&0u16.to_le_bytes());
            central_directory.extend_from_slice(&0u16.to_le_bytes());
            central_directory.extend_from_slice(&0u16.to_le_bytes());
            central_directory.extend_from_slice(&0u16.to_le_bytes());
            central_directory.extend_from_slice(&0u32.to_le_bytes());
            central_directory.extend_from_slice(&content_len.to_le_bytes());
            central_directory.extend_from_slice(&content_len.to_le_bytes());
            central_directory.extend_from_slice(&name_len.to_le_bytes());
            central_directory.extend_from_slice(&0u16.to_le_bytes());
            central_directory.extend_from_slice(&0u16.to_le_bytes());
            central_directory.extend_from_slice(&0u16.to_le_bytes());
            central_directory.extend_from_slice(&0u16.to_le_bytes());
            central_directory.extend_from_slice(&(unix_mode << 16).to_le_bytes());
            central_directory.extend_from_slice(&local_header_offset.to_le_bytes());
            central_directory.extend_from_slice(name_bytes);
        }

        let central_directory_offset =
            u32::try_from(bytes.len()).expect("test ZIP should fit u32 offsets");
        let central_directory_size =
            u32::try_from(central_directory.len()).expect("test ZIP should fit u32 sizes");
        bytes.write_all(&central_directory).expect("central dir");
        bytes.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&entry_count.to_le_bytes());
        bytes.extend_from_slice(&entry_count.to_le_bytes());
        bytes.extend_from_slice(&central_directory_size.to_le_bytes());
        bytes.extend_from_slice(&central_directory_offset.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{ArchiveEnvelopePolicy, NormalizedArchivePath, inspect_zip_bytes};

    fn policy() -> ArchiveEnvelopePolicy {
        ArchiveEnvelopePolicy::new("test archive", 1024, 4, 64)
    }

    fn zip_with_entries(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for (path, content) in entries {
                archive.start_file(path, options).expect("entry");
                archive.write_all(content).expect("content");
            }
            archive.finish().expect("zip");
        }
        cursor.into_inner()
    }

    #[test]
    fn validates_archive_envelope() {
        let bytes = zip_with_entries(&[("dir/file.txt", b"hello")]);
        let envelope = inspect_zip_bytes(&bytes, &policy()).expect("archive should validate");

        assert_eq!(envelope.entries().len(), 1);
        assert_eq!(envelope.entries()[0].path().as_str(), "dir/file.txt");
        assert_eq!(envelope.expanded_size(), 5);
    }

    #[test]
    fn rejects_hostile_archive_entries() {
        for name in ["../evil", "/evil", "a//b", "a/./b"] {
            let bytes = zip_with_entries(&[(name, b"x")]);
            assert!(inspect_zip_bytes(&bytes, &policy()).is_err(), "{name}");
        }

        let duplicate = zip_with_entries(&[("a/b.txt", b"1"), ("a\\b.txt", b"2")]);
        assert!(inspect_zip_bytes(&duplicate, &policy()).is_err());

        let symlink = super::test_support::raw_stored_zip(vec![("link", b"target", 0o120777)]);
        assert!(inspect_zip_bytes(&symlink, &policy()).is_err());
    }

    #[test]
    fn enforces_archive_limits() {
        let oversized = zip_with_entries(&[("file.txt", &[b'x'; 65])]);
        assert!(inspect_zip_bytes(&oversized, &policy()).is_err());

        let too_many = zip_with_entries(&[
            ("a", b"1"),
            ("b", b"1"),
            ("c", b"1"),
            ("d", b"1"),
            ("e", b"1"),
        ]);
        assert!(inspect_zip_bytes(&too_many, &policy()).is_err());

        let tiny_policy = ArchiveEnvelopePolicy::new("test archive", 8, 4, 64);
        let bytes = zip_with_entries(&[("file.txt", b"hello")]);
        assert!(inspect_zip_bytes(&bytes, &tiny_policy).is_err());
    }

    #[test]
    fn normalizes_local_relative_paths() {
        let path = NormalizedArchivePath::from_relative_path(
            std::path::Path::new("./src/main.rs"),
            "package path",
        )
        .expect("relative path should normalize");
        assert_eq!(path.as_str(), "src/main.rs");
    }
}
