use std::fs::{self, File};
use std::io::{Cursor, Seek, Write};
use std::path::{Path, PathBuf};

use agentics_error::{Result, ServiceError};
use ignore::{DirEntry, WalkBuilder};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::validation::archive::NormalizedArchivePath;
use crate::zip_project::{
    MAX_ZIP_PROJECT_ARTIFACT_BYTES, MAX_ZIP_PROJECT_FILE_COUNT, MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
    ZIP_PROJECT_MANIFEST_FILE, ZipProjectManifest, validate_zip_project_archive_envelope,
};

#[derive(Debug, Clone)]
/// Packaged `zip_project` workspace bytes plus stable package metadata.
pub struct ZipProjectWorkspacePackage {
    pub workspace_dir: PathBuf,
    pub bytes: Vec<u8>,
    pub file_count: usize,
    pub uncompressed_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
/// Limits used while packaging a `zip_project` workspace.
pub struct ZipProjectWorkspacePackageLimits {
    pub max_zip_bytes: u64,
    pub max_file_count: usize,
    pub max_uncompressed_bytes: u64,
}

impl ZipProjectWorkspacePackageLimits {
    pub const DEFAULT: Self = Self {
        max_zip_bytes: MAX_ZIP_PROJECT_ARTIFACT_BYTES,
        max_file_count: MAX_ZIP_PROJECT_FILE_COUNT,
        max_uncompressed_bytes: MAX_ZIP_PROJECT_UNCOMPRESSED_BYTES,
    };
}

#[derive(Debug, Clone)]
struct PackageFile {
    path: PathBuf,
    archive_name: String,
    unix_permissions: u32,
}

#[derive(Debug)]
struct CollectedPackageFiles {
    files: Vec<PackageFile>,
    uncompressed_bytes: u64,
}

/// Package one local `zip_project` solution workspace using the platform policy.
pub fn package_zip_project_workspace(workspace_dir: &Path) -> Result<ZipProjectWorkspacePackage> {
    package_zip_project_workspace_with_limits(
        workspace_dir,
        ZipProjectWorkspacePackageLimits::DEFAULT,
    )
}

/// Package one local `zip_project` solution workspace with explicit limits.
pub fn package_zip_project_workspace_with_limits(
    workspace_dir: &Path,
    limits: ZipProjectWorkspacePackageLimits,
) -> Result<ZipProjectWorkspacePackage> {
    let workspace_dir = workspace_dir.canonicalize().map_err(|error| {
        ServiceError::Validation(format!(
            "failed to resolve workspace {}: {error}",
            workspace_dir.display()
        ))
    })?;
    if !workspace_dir.is_dir() {
        return Err(ServiceError::Validation(format!(
            "workspace is not a directory: {}",
            workspace_dir.display()
        )));
    }

    let manifest_path = workspace_dir.join(ZIP_PROJECT_MANIFEST_FILE);
    if !fs::exists(&manifest_path).map_err(|error| {
        ServiceError::Validation(format!(
            "failed to inspect {}: {error}",
            manifest_path.display()
        ))
    })? {
        return Err(ServiceError::Validation(format!(
            "{ZIP_PROJECT_MANIFEST_FILE} must exist at the workspace root before packaging a solution submission"
        )));
    }
    if !manifest_path.is_file() {
        return Err(ServiceError::Validation(format!(
            "{ZIP_PROJECT_MANIFEST_FILE} must be a file"
        )));
    }
    let manifest_raw = fs::read_to_string(&manifest_path)?;
    let manifest = ZipProjectManifest::parse_json(&manifest_raw)?;

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
        if !fs::exists(&script_path).map_err(|error| {
            ServiceError::Validation(format!(
                "failed to inspect {}: {error}",
                script_path.display()
            ))
        })? {
            return Err(ServiceError::Validation(format!(
                "{script} must exist before packaging a solution submission"
            )));
        }
        if !script_path.is_file() {
            return Err(ServiceError::Validation(format!("{script} must be a file")));
        }
    }

    let collected = collect_package_files(&workspace_dir, limits)?;
    let files = collected.files;
    if !files
        .iter()
        .any(|file| file.archive_name == ZIP_PROJECT_MANIFEST_FILE)
    {
        return Err(ServiceError::Validation(format!(
            "{ZIP_PROJECT_MANIFEST_FILE} is excluded by .gitignore or package filters"
        )));
    }
    for script in required_scripts {
        if !files
            .iter()
            .any(|file| file.archive_name == script.as_str())
        {
            return Err(ServiceError::Validation(format!(
                "{script} is excluded by .gitignore or package filters"
            )));
        }
    }
    if files.is_empty() {
        return Err(ServiceError::Validation(
            "workspace contains no packageable files".to_string(),
        ));
    }

    let bytes = write_zip_archive(&files, limits)?;
    let zip_bytes = u64::try_from(bytes.len()).map_err(|_| {
        ServiceError::Validation("zip archive length exceeds supported range".to_string())
    })?;
    if zip_bytes > limits.max_zip_bytes {
        return Err(ServiceError::Validation(format!(
            "solution archive must be at most {} bytes after compression",
            limits.max_zip_bytes
        )));
    }
    validate_zip_project_archive_envelope(&bytes)?;

    Ok(ZipProjectWorkspacePackage {
        workspace_dir,
        bytes,
        file_count: files.len(),
        uncompressed_bytes: collected.uncompressed_bytes,
    })
}

fn collect_package_files(
    workspace_dir: &Path,
    limits: ZipProjectWorkspacePackageLimits,
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
        let entry = entry.map_err(|error| {
            ServiceError::Validation(format!(
                "failed to walk workspace while packaging {}: {error}",
                workspace_dir.display()
            ))
        })?;
        if entry.path() == workspace_dir || !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }

        let relative = entry.path().strip_prefix(workspace_dir).map_err(|error| {
            ServiceError::internal(format!(
                "failed to compute relative path for {}: {error}",
                entry.path().display()
            ))
        })?;
        let archive_name = archive_name(relative)?;
        let metadata = entry.metadata().map_err(|error| {
            ServiceError::Validation(format!(
                "failed to stat {}: {error}",
                entry.path().display()
            ))
        })?;
        if files.len() >= limits.max_file_count {
            return Err(ServiceError::Validation(format!(
                "solution workspace must contain at most {} packageable files",
                limits.max_file_count
            )));
        }
        uncompressed_bytes = uncompressed_bytes
            .checked_add(metadata.len())
            .ok_or_else(|| {
                ServiceError::Validation("solution workspace is too large".to_string())
            })?;
        if uncompressed_bytes > limits.max_uncompressed_bytes {
            return Err(ServiceError::Validation(format!(
                "solution workspace must contain at most {} bytes before compression",
                limits.max_uncompressed_bytes
            )));
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

fn write_zip_archive(
    files: &[PackageFile],
    limits: ZipProjectWorkspacePackageLimits,
) -> Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);

    for file in files {
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(file.unix_permissions);
        archive.start_file(&file.archive_name, options)?;
        copy_file_to_archive(file, &mut archive)?;
        if current_archive_len(&archive)? > limits.max_zip_bytes {
            return Err(ServiceError::Validation(format!(
                "solution archive must be at most {} bytes after compression",
                limits.max_zip_bytes
            )));
        }
    }

    Ok(archive.finish()?.into_inner())
}

fn current_archive_len(archive: &zip::ZipWriter<Cursor<Vec<u8>>>) -> Result<u64> {
    let cursor = archive
        .get_ref()
        .ok_or_else(|| ServiceError::internal("zip writer closed before package finalization"))?;
    u64::try_from(cursor.get_ref().len()).map_err(|_| {
        ServiceError::Validation("zip archive length exceeds supported range".to_string())
    })
}

fn copy_file_to_archive<W>(file: &PackageFile, archive: &mut zip::ZipWriter<W>) -> Result<()>
where
    W: Write + Seek,
{
    let mut input = File::open(&file.path)?;
    std::io::copy(&mut input, archive)?;
    Ok(())
}

fn archive_name(path: &Path) -> Result<String> {
    NormalizedArchivePath::from_relative_path(path, "solution package path")
        .map(|path| path.to_string())
}

#[cfg(unix)]
fn unix_permissions(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn unix_permissions(_metadata: &std::fs::Metadata) -> u32 {
    0o644
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;

    use super::{
        ZipProjectWorkspacePackageLimits, package_zip_project_workspace,
        package_zip_project_workspace_with_limits,
    };

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

        let package = package_zip_project_workspace(root).expect("workspace should package");
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

    #[test]
    fn package_rejects_missing_run_script() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("main.py"), "print('ok')\n").expect("main.py");

        let error =
            package_zip_project_workspace(temp.path()).expect_err("run.sh should be required");

        assert!(error.to_string().contains("run.sh must exist"));
    }

    #[test]
    fn package_rejects_ignored_run_script() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");
        fs::write(temp.path().join(".gitignore"), "run.sh\n").expect("gitignore");

        let error =
            package_zip_project_workspace(temp.path()).expect_err("ignored run.sh should fail");

        assert!(error.to_string().contains("run.sh is excluded"));
    }

    #[test]
    fn package_requires_declared_setup_and_build_scripts() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest_with_setup_build(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");

        let error = package_zip_project_workspace(temp.path())
            .expect_err("setup script should be required");

        assert!(error.to_string().contains("scripts/setup.sh must exist"));
    }

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

        let package = package_zip_project_workspace(temp.path()).expect("workspace should package");
        let names = zip_file_names(&package.bytes);

        assert!(names.contains(&"scripts/setup.sh".to_string()));
        assert!(names.contains(&"scripts/build.sh".to_string()));
    }

    #[test]
    fn package_rejects_too_many_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");
        fs::write(temp.path().join("extra.txt"), "x").expect("extra");

        let error = package_zip_project_workspace_with_limits(
            temp.path(),
            ZipProjectWorkspacePackageLimits {
                max_file_count: 2,
                ..ZipProjectWorkspacePackageLimits::DEFAULT
            },
        )
        .expect_err("file count should fail");

        assert!(error.to_string().contains("at most 2 packageable files"));
    }

    #[test]
    fn package_rejects_too_many_uncompressed_bytes() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");
        fs::write(temp.path().join("main.py"), "print('hello')\n").expect("main.py");

        let error = package_zip_project_workspace_with_limits(
            temp.path(),
            ZipProjectWorkspacePackageLimits {
                max_uncompressed_bytes: 16,
                ..ZipProjectWorkspacePackageLimits::DEFAULT
            },
        )
        .expect_err("uncompressed size should fail");

        assert!(error.to_string().contains("bytes before compression"));
    }

    #[test]
    fn package_rejects_zip_bytes_limit() {
        let temp = tempfile::tempdir().expect("tempdir");
        write_manifest(temp.path());
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");

        let error = package_zip_project_workspace_with_limits(
            temp.path(),
            ZipProjectWorkspacePackageLimits {
                max_zip_bytes: 32,
                ..ZipProjectWorkspacePackageLimits::DEFAULT
            },
        )
        .expect_err("zip size should fail");

        assert!(error.to_string().contains("bytes after compression"));
    }

    fn zip_file_names(bytes: &[u8]) -> Vec<String> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(cursor).expect("zip archive");
        let mut names = Vec::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).expect("zip file");
            let mut contents = String::new();
            drop(file.read_to_string(&mut contents));
            names.push(file.name().to_string());
        }
        names
    }
}
