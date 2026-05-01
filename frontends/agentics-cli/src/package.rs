use std::fs::File;
use std::io::{Cursor, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use ignore::{DirEntry, WalkBuilder};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

const REQUIRED_RUN_SCRIPT: &str = "run.sh";

#[derive(Debug, Clone)]
pub struct SubmissionPackage {
    pub workspace_dir: PathBuf,
    pub bytes: Vec<u8>,
    pub file_count: usize,
    pub uncompressed_bytes: u64,
}

#[derive(Debug, Clone)]
struct PackageFile {
    path: PathBuf,
    archive_name: String,
    size: u64,
    unix_permissions: u32,
}

pub fn package_solution_workspace(workspace_dir: &Path) -> Result<SubmissionPackage> {
    let workspace_dir = workspace_dir
        .canonicalize()
        .with_context(|| format!("failed to resolve workspace {}", workspace_dir.display()))?;
    if !workspace_dir.is_dir() {
        bail!("workspace is not a directory: {}", workspace_dir.display());
    }

    let run_script = workspace_dir.join(REQUIRED_RUN_SCRIPT);
    if !run_script.is_file() {
        bail!("{REQUIRED_RUN_SCRIPT} must exist at the workspace root before submission");
    }

    let files = collect_package_files(&workspace_dir)?;
    if !files
        .iter()
        .any(|file| file.archive_name == REQUIRED_RUN_SCRIPT)
    {
        bail!("{REQUIRED_RUN_SCRIPT} is excluded by .gitignore or package filters");
    }
    if files.is_empty() {
        bail!("workspace contains no packageable files");
    }

    let uncompressed_bytes = files.iter().map(|file| file.size).sum();
    let bytes = write_zip_archive(&files)?;

    Ok(SubmissionPackage {
        workspace_dir,
        bytes,
        file_count: files.len(),
        uncompressed_bytes,
    })
}

fn collect_package_files(workspace_dir: &Path) -> Result<Vec<PackageFile>> {
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

        files.push(PackageFile {
            path: entry.path().to_path_buf(),
            archive_name,
            size: metadata.len(),
            unix_permissions: unix_permissions(&metadata),
        });
    }

    files.sort_by(|a, b| a.archive_name.cmp(&b.archive_name));
    Ok(files)
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

fn write_zip_archive(files: &[PackageFile]) -> Result<Vec<u8>> {
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
    }

    Ok(archive.finish()?.into_inner())
}

fn copy_file_to_archive<W>(file: &PackageFile, archive: &mut zip::ZipWriter<W>) -> Result<()>
where
    W: Write + Seek,
{
    let mut input = File::open(&file.path)
        .with_context(|| format!("failed to open {}", file.path.display()))?;
    let mut buffer = Vec::new();
    input
        .read_to_end(&mut buffer)
        .with_context(|| format!("failed to read {}", file.path.display()))?;
    archive
        .write_all(&buffer)
        .with_context(|| format!("failed to write {} to zip", file.archive_name))
}

fn archive_name(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .with_context(|| format!("path is not valid UTF-8: {}", path.display()))?;
                parts.push(value);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("invalid package path: {}", path.display());
            }
        }
    }

    if parts.is_empty() {
        bail!("empty package path");
    }

    Ok(parts.join("/"))
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

    use super::package_solution_workspace;

    #[test]
    fn package_respects_gitignore_and_excludes_git_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        fs::write(root.join("run.sh"), "#!/usr/bin/env bash\npython main.py\n").expect("run.sh");
        fs::write(root.join("main.py"), "print('ok')\n").expect("main.py");
        fs::write(root.join("ignored.txt"), "ignored").expect("ignored");
        fs::write(root.join(".gitignore"), "ignored.txt\n").expect("gitignore");
        fs::create_dir(root.join(".git")).expect("git dir");
        fs::write(root.join(".git/config"), "private").expect("git config");

        let package = package_solution_workspace(root).expect("workspace should package");
        let names = zip_file_names(&package.bytes);

        assert_eq!(package.file_count, 2);
        assert_eq!(names, vec!["main.py".to_string(), "run.sh".to_string()]);
    }

    #[test]
    fn package_rejects_missing_run_script() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("main.py"), "print('ok')\n").expect("main.py");

        let error = package_solution_workspace(temp.path()).expect_err("run.sh should be required");

        assert!(error.to_string().contains("run.sh must exist"));
    }

    #[test]
    fn package_rejects_ignored_run_script() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("run.sh"), "#!/usr/bin/env bash\n").expect("run.sh");
        fs::write(temp.path().join(".gitignore"), "run.sh\n").expect("gitignore");

        let error =
            package_solution_workspace(temp.path()).expect_err("ignored run.sh should fail");

        assert!(error.to_string().contains("run.sh is excluded"));
    }

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
