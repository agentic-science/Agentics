use std::fs::{self, File};
use std::io::{Cursor, Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ignore::{DirEntry, WalkBuilder};
use shared::validation::archive::NormalizedArchivePath;
use shared::zip_project::validate_zip_project_archive_envelope;
use shared::zip_project::{
    MAX_ZIP_PROJECT_ARTIFACT_BYTES, MAX_ZIP_PROJECT_FILE_COUNT, MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

const REQUIRED_MANIFEST: &str = shared::zip_project::ZIP_PROJECT_MANIFEST_FILE;

#[derive(Debug, Clone)]
/// Carries solution package data across this module boundary.
pub(crate) struct SolutionPackage {
    pub workspace_dir: PathBuf,
    pub bytes: Vec<u8>,
    pub file_count: usize,
    pub uncompressed_bytes: u64,
}

#[derive(Debug, Clone)]
/// Carries package file data across this module boundary.
struct PackageFile {
    path: PathBuf,
    archive_name: String,
    unix_permissions: u32,
}

#[derive(Debug, Clone, Copy)]
/// Carries package limits data across this module boundary.
struct PackageLimits {
    max_zip_bytes: u64,
    max_file_count: usize,
    max_uncompressed_bytes: u64,
}

impl PackageLimits {
    const DEFAULT: Self = Self {
        max_zip_bytes: MAX_ZIP_PROJECT_ARTIFACT_BYTES,
        max_file_count: MAX_ZIP_PROJECT_FILE_COUNT,
        max_uncompressed_bytes: MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
    };
}

#[derive(Debug)]
/// Carries collected package files data across this module boundary.
struct CollectedPackageFiles {
    files: Vec<PackageFile>,
    uncompressed_bytes: u64,
}

/// Handles package solution workspace for this module.
pub(crate) fn package_solution_workspace(workspace_dir: &Path) -> Result<SolutionPackage> {
    package_solution_workspace_with_limits(workspace_dir, PackageLimits::DEFAULT)
}

/// Handles package solution workspace with limits for this module.
fn package_solution_workspace_with_limits(
    workspace_dir: &Path,
    limits: PackageLimits,
) -> Result<SolutionPackage> {
    let workspace_dir = workspace_dir
        .canonicalize()
        .with_context(|| format!("failed to resolve workspace {}", workspace_dir.display()))?;
    if !workspace_dir.is_dir() {
        bail!("workspace is not a directory: {}", workspace_dir.display());
    }

    let manifest_path = workspace_dir.join(REQUIRED_MANIFEST);
    if !fs::exists(&manifest_path)
        .with_context(|| format!("failed to inspect {}", manifest_path.display()))?
    {
        bail!(
            "{REQUIRED_MANIFEST} must exist at the workspace root before packaging a solution submission"
        );
    }
    if !manifest_path.is_file() {
        bail!("{REQUIRED_MANIFEST} must be a file");
    }
    let manifest_raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest = shared::zip_project::ZipProjectManifest::parse_json(&manifest_raw)?;

    let mut required_scripts = Vec::new();
    if let Some(setup) = &manifest.commands.setup {
        required_scripts.push(setup);
    }
    if let Some(build) = &manifest.commands.build {
        required_scripts.push(build);
    }
    required_scripts.push(&manifest.commands.run);
    for script in &required_scripts {
        let script_path = workspace_dir.join(script.as_path());
        if !fs::exists(&script_path)
            .with_context(|| format!("failed to inspect {}", script_path.display()))?
        {
            bail!("{script} must exist before packaging a solution submission");
        }
        if !script_path.is_file() {
            bail!("{script} must be a file");
        }
    }

    let collected = collect_package_files(&workspace_dir, limits)?;
    let files = collected.files;
    if !files
        .iter()
        .any(|file| file.archive_name == REQUIRED_MANIFEST)
    {
        bail!("{REQUIRED_MANIFEST} is excluded by .gitignore or package filters");
    }
    for script in required_scripts {
        if !files
            .iter()
            .any(|file| file.archive_name == script.as_str())
        {
            bail!("{script} is excluded by .gitignore or package filters");
        }
    }
    if files.is_empty() {
        bail!("workspace contains no packageable files");
    }

    let bytes = write_zip_archive(&files, limits)?;
    if bytes.len() as u64 > limits.max_zip_bytes {
        bail!(
            "solution archive must be at most {} bytes after compression",
            limits.max_zip_bytes
        );
    }
    validate_zip_project_archive_envelope(&bytes)?;

    Ok(SolutionPackage {
        workspace_dir,
        bytes,
        file_count: files.len(),
        uncompressed_bytes: collected.uncompressed_bytes,
    })
}

/// Handles collect package files for this module.
fn collect_package_files(
    workspace_dir: &Path,
    limits: PackageLimits,
) -> Result<CollectedPackageFiles> {
    let mut builder = WalkBuilder::new(workspace_dir);
    builder
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .hidden(true)
        .parents(true)
        .require_git(false)
        .filter_entry(should_descend);

    let mut files = Vec::new();
    let mut uncompressed_bytes = 0u64;
    for entry in builder.build() {
        let entry = entry.with_context(|| {
            format!(
                "failed to walk workspace while packaging {}",
                workspace_dir.display()
            )
        })?;
        if entry.path() == workspace_dir || !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }

        let relative = entry.path().strip_prefix(workspace_dir).with_context(|| {
            format!(
                "failed to compute relative path for {}",
                entry.path().display()
            )
        })?;
        let archive_name = archive_name(relative)?;
        let metadata = entry
            .metadata()
            .with_context(|| format!("failed to stat {}", entry.path().display()))?;
        if files.len() >= limits.max_file_count {
            bail!(
                "solution workspace must contain at most {} packageable files",
                limits.max_file_count
            );
        }
        uncompressed_bytes = uncompressed_bytes
            .checked_add(metadata.len())
            .context("solution workspace is too large")?;
        if uncompressed_bytes > limits.max_uncompressed_bytes {
            bail!(
                "solution workspace must contain at most {} bytes before compression",
                limits.max_uncompressed_bytes
            );
        }

        files.push(PackageFile {
            path: entry.path().to_path_buf(),
            archive_name,
            unix_permissions: unix_permissions(&metadata),
        });
    }

    files.sort_by(|a, b| a.archive_name.cmp(&b.archive_name));
    Ok(CollectedPackageFiles {
        files,
        uncompressed_bytes,
    })
}

/// Handles should descend for this module.
fn should_descend(entry: &DirEntry) -> bool {
    let Some(name) = entry.file_name().to_str() else {
        return false;
    };

    !matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | "__pycache__"
            | ".pytest_cache"
            | ".ruff_cache"
            | ".mypy_cache"
            | ".venv"
            | "dist"
            | "build"
            | ".next"
    )
}

/// Writes zip archive to the target path.
fn write_zip_archive(files: &[PackageFile], limits: PackageLimits) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);

    for file in files {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(file.unix_permissions);
        archive
            .start_file(&file.archive_name, options)
            .with_context(|| format!("failed to add {} to zip", file.archive_name))?;
        copy_file_to_archive(file, &mut archive)?;
        if current_archive_len(&archive)? > limits.max_zip_bytes {
            bail!(
                "solution archive must be at most {} bytes after compression",
                limits.max_zip_bytes
            );
        }
    }

    Ok(archive.finish()?.into_inner())
}

/// Handles current archive len for this module.
fn current_archive_len(archive: &zip::ZipWriter<Cursor<Vec<u8>>>) -> Result<u64> {
    let cursor = archive
        .get_ref()
        .context("zip writer closed before package finalization")?;
    Ok(cursor.get_ref().len() as u64)
}

/// Copies file to archive while preserving the module invariants.
fn copy_file_to_archive<W>(file: &PackageFile, archive: &mut zip::ZipWriter<W>) -> Result<()>
where
    W: Write + Seek,
{
    let mut input = File::open(&file.path)
        .with_context(|| format!("failed to open {}", file.path.display()))?;
    std::io::copy(&mut input, archive)
        .with_context(|| format!("failed to write {} to zip", file.archive_name))
        .map(|_| ())
}

/// Handles archive name for this module.
fn archive_name(path: &Path) -> Result<String> {
    Ok(NormalizedArchivePath::from_relative_path(path, "solution package path")?.to_string())
}

/// Handles unix permissions for this module.
fn unix_permissions(metadata: &std::fs::Metadata) -> u32 {
    cfg_select! {
        unix => {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o777
        }
        _ => {
            let _ = metadata;
            0o644
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;

    use super::{
        PackageLimits, package_solution_workspace, package_solution_workspace_with_limits,
    };

    /// Writes manifest to the target path.
    fn write_manifest(root: &std::path::Path) {
        fs::write(
            root.join("agentics.solution.json"),
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "note": "",
                "commands": { "run": "run.sh" }
            })
            .to_string(),
        )
        .expect("manifest");
    }

    /// Writes manifest with setup and build commands to the target path.
    fn write_manifest_with_setup_build(root: &std::path::Path) {
        fs::write(
            root.join("agentics.solution.json"),
            serde_json::json!({
                "protocol": "zip_project",
                "protocol_version": 1,
                "commands": {
                    "setup": "scripts/setup.sh",
                    "build": "scripts/build.sh",
                    "run": "run.sh"
                }
            })
            .to_string(),
        )
        .expect("manifest");
    }

    /// Verifies that package respects gitignore and excludes git directory.
    #[test]
    fn package_respects_gitignore_and_excludes_git_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        write_manifest(root);
        fs::write(root.join("run.sh"), "#!/usr/bin/env bash\npython main.py\n").expect("run.sh");
        fs::write(root.join("main.py"), "print('ok')\n").expect("main.py");
        fs::write(root.join("ignored.txt"), "ignored").expect("ignored");
        fs::write(root.join(".gitignore"), "ignored.txt\n").expect("gitignore");
        fs::create_dir(root.join(".git")).expect("git dir");
        fs::write(root.join(".git/config"), "private").expect("git config");

        let package = package_solution_workspace(root).expect("workspace should package");
        let names = zip_file_names(&package.bytes);

        assert_eq!(package.file_count, 3);
        assert_eq!(
            names,
            vec![
                "agentics.solution.json".to_string(),
                "main.py".to_string(),
                "run.sh".to_string()
            ]
        );
    }

    /// Verifies that package rejects missing run script.
    #[test]
    fn package_rejects_missing_run_script() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("main.py"), "print('ok')\n").expect("main.py");

        let error = package_solution_workspace(temp.path()).expect_err("run.sh should be required");

        assert!(error.to_string().contains("run.sh must exist"));
    }

    /// Verifies that package rejects ignored run script.
    #[test]
    fn package_rejects_ignored_run_script() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");
        fs::write(temp.path().join(".gitignore"), "run.sh\n").expect("gitignore");

        let error =
            package_solution_workspace(temp.path()).expect_err("ignored run.sh should fail");

        assert!(error.to_string().contains("run.sh is excluded"));
    }

    /// Verifies that package requires optional setup and build scripts when declared.
    #[test]
    fn package_requires_declared_setup_and_build_scripts() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest_with_setup_build(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");

        let error =
            package_solution_workspace(temp.path()).expect_err("setup script should be required");

        assert!(error.to_string().contains("scripts/setup.sh must exist"));
    }

    /// Verifies that package includes optional setup and build scripts.
    #[test]
    fn package_includes_declared_setup_and_build_scripts() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest_with_setup_build(temp.path());
        fs::create_dir(temp.path().join("scripts")).expect("scripts dir");
        fs::write(
            temp.path().join("scripts/setup.sh"),
            "#!/usr/bin/env bash\n",
        )
        .expect("setup");
        fs::write(
            temp.path().join("scripts/build.sh"),
            "#!/usr/bin/env bash\n",
        )
        .expect("build");
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");

        let package = package_solution_workspace(temp.path()).expect("workspace should package");
        let names = zip_file_names(&package.bytes);

        assert!(names.contains(&"scripts/setup.sh".to_string()));
        assert!(names.contains(&"scripts/build.sh".to_string()));
    }

    /// Verifies that package rejects too many files before zip creation.
    #[test]
    fn package_rejects_too_many_files_before_zip_creation() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");
        fs::write(temp.path().join("main.py"), "print('ok')\n").expect("main.py");

        let error = package_solution_workspace_with_limits(
            temp.path(),
            PackageLimits {
                max_file_count: 2,
                ..PackageLimits::DEFAULT
            },
        )
        .expect_err("file count limit should reject the workspace");

        assert!(error.to_string().contains("at most 2 packageable files"));
    }

    /// Verifies that package rejects too many uncompressed bytes before zip creation.
    #[test]
    fn package_rejects_too_many_uncompressed_bytes_before_zip_creation() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");
        fs::write(temp.path().join("main.py"), "print('ok')\n").expect("main.py");

        let error = package_solution_workspace_with_limits(
            temp.path(),
            PackageLimits {
                max_uncompressed_bytes: 16,
                ..PackageLimits::DEFAULT
            },
        )
        .expect_err("uncompressed size limit should reject the workspace");

        assert!(
            error
                .to_string()
                .contains("at most 16 bytes before compression")
        );
    }

    /// Verifies that package rejects too many zip bytes.
    #[test]
    fn package_rejects_too_many_zip_bytes() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");

        let error = package_solution_workspace_with_limits(
            temp.path(),
            PackageLimits {
                max_zip_bytes: 1,
                ..PackageLimits::DEFAULT
            },
        )
        .expect_err("zip size limit should reject the workspace");

        assert!(
            error
                .to_string()
                .contains("at most 1 bytes after compression")
        );
    }

    /// Handles zip file names for this module.
    fn zip_file_names(bytes: &[u8]) -> Vec<String> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("zip should open");
        let mut names = Vec::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).expect("zip entry should open");
            let mut contents = String::new();
            file.read_to_string(&mut contents).expect("entry is text");
            names.push(file.name().to_string());
        }
        names.sort();
        names
    }
}
