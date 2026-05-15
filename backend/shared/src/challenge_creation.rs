//! Validation helpers for public GitHub challenge creation proposals.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::challenge_bundle::{
    is_safe_relative_path, read_challenge_bundle_spec, read_challenge_run_manifest,
};
use crate::error::{AppError, Result};
use crate::models::challenge_creation::{
    AGENTICS_CHALLENGE_MANIFEST_FILE, ChallengeCreationManifest, ChallengeCreationRequestKind,
    ChallengePrivateAssetRequirement, ChallengePrivateAssetResponse,
};

/// Read `agentics.challenge.json` from a proposal root.
pub async fn read_challenge_creation_manifest(root: &Path) -> Result<ChallengeCreationManifest> {
    let manifest_path = root.join(AGENTICS_CHALLENGE_MANIFEST_FILE);
    let raw = tokio::fs::read_to_string(&manifest_path).await?;
    let manifest: ChallengeCreationManifest = serde_json::from_str(&raw).map_err(|e| {
        AppError::Validation(format!("invalid {AGENTICS_CHALLENGE_MANIFEST_FILE}: {e}"))
    })?;
    validate_challenge_creation_manifest(&manifest)?;
    Ok(manifest)
}

/// Validate one challenge proposal directory and return the parsed manifest.
///
/// The directory is the challenge-level root inside a public repository, for
/// example `challenges/sample-sum/`. Private benchmark datasets, private
/// scorer packages, seeds, and reference outputs must be uploaded through
/// Agentics storage instead of being committed here.
pub async fn validate_challenge_creation_repository(
    root: &Path,
) -> Result<ChallengeCreationManifest> {
    let manifest = read_challenge_creation_manifest(root).await?;
    validate_challenge_creation_repository_with_manifest(root, &manifest).await?;
    Ok(manifest)
}

/// Validate semantic fields that do not depend on local filesystem state.
pub fn validate_challenge_creation_manifest(manifest: &ChallengeCreationManifest) -> Result<()> {
    if manifest.schema_version != 1 {
        return Err(AppError::Validation("schema_version must be 1".to_string()));
    }
    require_non_empty(&manifest.title, "title")?;
    require_non_empty(&manifest.summary, "summary")?;
    require_safe_relative_path(&manifest.readme_path, "readme_path")?;
    validate_private_asset_requirements(&manifest.private_assets)?;

    match manifest.request {
        ChallengeCreationRequestKind::NewChallenge => {
            let bundle_path = manifest.bundle_path.as_deref().ok_or_else(|| {
                AppError::Validation("bundle_path is required for new_challenge".to_string())
            })?;
            if manifest.archive.is_some() {
                return Err(AppError::Validation(
                    "archive must be omitted for new_challenge".to_string(),
                ));
            }
            require_safe_relative_path(bundle_path, "bundle_path")?;
        }
        ChallengeCreationRequestKind::ArchiveChallenge => {
            if manifest.bundle_path.is_some() {
                return Err(AppError::Validation(
                    "bundle_path must be omitted for archive_challenge".to_string(),
                ));
            }
            let archive = manifest.archive.as_ref().ok_or_else(|| {
                AppError::Validation("archive is required for archive_challenge".to_string())
            })?;
            require_non_empty(&archive.reason, "archive.reason")?;
        }
    }

    Ok(())
}

/// Return a stable SHA-256 digest of a normalized manifest JSON representation.
pub fn normalized_manifest_sha256(manifest: &ChallengeCreationManifest) -> Result<String> {
    let bytes = serde_json::to_vec(manifest).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(sha256_hex(&bytes))
}

/// Return a deterministic digest for the draft content a reviewer validated.
///
/// The digest covers the normalized public manifest, the public bundle tree for
/// publishable requests, and the uploaded private asset nameentities. It is not a
/// replacement for a future server-side Git checkout at `commit_sha`, but it
/// gives validation, approval, and publish an exact content identity to compare
/// within the MVP trust boundary.
pub async fn draft_review_bundle_sha256(
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
    private_assets: &[ChallengePrivateAssetResponse],
) -> Result<String> {
    let proposal_root = proposal_root.to_path_buf();
    let manifest = manifest.clone();
    let private_assets = private_assets.to_vec();
    tokio::task::spawn_blocking(move || {
        draft_review_bundle_sha256_blocking(&proposal_root, &manifest, &private_assets)
    })
    .await
    .map_err(|e| AppError::Internal(format!("draft digest task failed: {e}")))?
}

/// Return the hex SHA-256 digest of arbitrary bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn draft_review_bundle_sha256_blocking(
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
    private_assets: &[ChallengePrivateAssetResponse],
) -> Result<String> {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "format", b"agentics-draft-review-v1");

    let manifest_bytes =
        serde_json::to_vec(manifest).map_err(|e| AppError::Internal(e.to_string()))?;
    hash_field(&mut hasher, "manifest", &manifest_bytes);

    if let Some(bundle_path) = &manifest.bundle_path {
        let bundle_root = proposal_root.join(bundle_path);
        hash_public_tree(&mut hasher, &bundle_root)?;
    }

    let mut assets = private_assets.to_vec();
    assets.sort_by(|left, right| left.asset_name.cmp(&right.asset_name));
    for asset in assets {
        hash_field(&mut hasher, "asset_name", asset.asset_name.as_bytes());
        hash_field(&mut hasher, "asset_kind", asset.kind.as_str().as_bytes());
        hash_field(&mut hasher, "asset_required", &[u8::from(asset.required)]);
        hash_field(&mut hasher, "asset_size", &asset.size_bytes.to_be_bytes());
        hash_field(&mut hasher, "asset_sha256", asset.sha256.as_bytes());
    }

    Ok(hex::encode(hasher.finalize()))
}

fn hash_public_tree(hasher: &mut Sha256, bundle_root: &Path) -> Result<()> {
    let mut stack = vec![bundle_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = std::fs::read_dir(&dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            let relative_path = path
                .strip_prefix(bundle_root)
                .map_err(|e| AppError::Internal(format!("failed to build review digest: {e}")))?;
            let relative_path = relative_path.to_str().ok_or_else(|| {
                AppError::Validation(format!(
                    "public bundle path must be UTF-8 for review digest: {}",
                    path.display()
                ))
            })?;

            if metadata.file_type().is_symlink() {
                return Err(AppError::Validation(format!(
                    "public bundle must not contain symlinks: {}",
                    path.display()
                )));
            }
            if metadata.is_dir() {
                hash_field(hasher, "dir", relative_path.as_bytes());
                stack.push(path);
            } else if metadata.is_file() {
                hash_field(hasher, "file", relative_path.as_bytes());
                hash_file(hasher, &path)?;
            }
        }
    }

    Ok(())
}

fn hash_file(hasher: &mut Sha256, path: &Path) -> Result<()> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    hash_field(hasher, "file_size", &size.to_be_bytes());

    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        let chunk = buffer.get(..bytes_read).ok_or_else(|| {
            AppError::Internal("file read exceeded digest buffer bounds".to_string())
        })?;
        hasher.update(chunk);
    }

    Ok(())
}

fn hash_field(hasher: &mut Sha256, label: &str, bytes: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

async fn validate_challenge_creation_repository_with_manifest(
    root: &Path,
    manifest: &ChallengeCreationManifest,
) -> Result<()> {
    if !tokio::fs::try_exists(root.join(AGENTICS_CHALLENGE_MANIFEST_FILE)).await? {
        return Err(AppError::Validation(format!(
            "{AGENTICS_CHALLENGE_MANIFEST_FILE} is required"
        )));
    }
    assert_public_file_exists(root.join(&manifest.readme_path), "readme_path").await?;
    reject_private_files(root)?;

    if let Some(bundle_path) = &manifest.bundle_path {
        validate_public_bundle(root, manifest, bundle_path).await?;
    }

    Ok(())
}

async fn validate_public_bundle(
    root: &Path,
    manifest: &ChallengeCreationManifest,
    bundle_path: &str,
) -> Result<()> {
    let bundle_dir = root.join(bundle_path);
    let spec = read_challenge_bundle_spec(&bundle_dir).await?;
    if spec.challenge_name != manifest.challenge_name {
        return Err(AppError::Validation(format!(
            "bundle challenge_name mismatch: expected {}, got {}",
            manifest.challenge_name, spec.challenge_name
        )));
    }
    if spec.challenge_title != manifest.title {
        return Err(AppError::Validation(format!(
            "bundle challenge_title mismatch: expected {}, got {}",
            manifest.title, spec.challenge_title
        )));
    }
    if spec.challenge_summary != manifest.summary {
        return Err(AppError::Validation(format!(
            "bundle challenge_summary mismatch: expected {}, got {}",
            manifest.summary, spec.challenge_summary
        )));
    }
    assert_public_file_exists(bundle_dir.join("statement.md"), "statement.md").await?;
    assert_public_dir_exists(
        bundle_dir.join(&spec.datasets.public_dir),
        "datasets.public_dir",
    )
    .await?;

    if spec.targets.iter().any(|target| target.validation_enabled)
        && let Some(validation_runs) = spec.execution.validation_runs.as_deref()
    {
        assert_public_file_exists(
            bundle_dir.join(validation_runs),
            "execution.validation_runs",
        )
        .await?;
        let manifest = read_challenge_run_manifest(&bundle_dir, validation_runs).await?;
        crate::challenge_bundle::validate_challenge_run_manifest_sources(&bundle_dir, &manifest)
            .await?;
    }

    Ok(())
}

async fn assert_public_file_exists(path: PathBuf, field: &str) -> Result<()> {
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AppError::Validation(format!("{field} does not exist: {}", path.display())))?;
    if !meta.is_file() {
        return Err(AppError::Validation(format!(
            "{field} is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

async fn assert_public_dir_exists(path: PathBuf, field: &str) -> Result<()> {
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|_| AppError::Validation(format!("{field} does not exist: {}", path.display())))?;
    if !meta.is_dir() {
        return Err(AppError::Validation(format!(
            "{field} is not a directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_private_asset_requirements(
    private_assets: &[ChallengePrivateAssetRequirement],
) -> Result<()> {
    let mut ids = HashSet::with_capacity(private_assets.len());
    for asset in private_assets {
        if !ids.insert(asset.asset_name.as_str()) {
            return Err(AppError::Validation(format!(
                "private_assets contains duplicate asset_name `{}`",
                asset.asset_name
            )));
        }
        if let Some(note) = &asset.asset_note {
            require_non_empty(note, "private_assets[].asset_note")?;
        }
    }
    Ok(())
}

fn require_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }
    Ok(())
}

fn require_safe_relative_path(value: &str, field: &str) -> Result<()> {
    if !is_safe_relative_path(value) {
        return Err(AppError::Validation(format!(
            "{field} must be a safe relative path"
        )));
    }
    Ok(())
}

fn reject_private_files(root: &Path) -> Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();

            if file_name == ".git" {
                continue;
            }
            if is_forbidden_public_repo_name(&file_name) {
                return Err(AppError::Validation(format!(
                    "public challenge repo must not contain private benchmark or secret material: {}",
                    path.display()
                )));
            }

            let meta = std::fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                stack.push(path);
            } else if meta.file_type().is_symlink() {
                return Err(AppError::Validation(format!(
                    "public challenge repo must not contain symlinks: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn is_forbidden_public_repo_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        ".env"
            | ".env.local"
            | ".env.production"
            | "id_rsa"
            | "id_ed25519"
            | "secret"
            | "secrets"
            | "private"
            | "private-benchmark"
            | "private_benchmark"
            | "heldout"
            | "heldout-data"
            | "reference-output"
            | "reference-outputs"
    ) || normalized.ends_with(".pem")
        || normalized.ends_with(".key")
        || normalized.ends_with(".p12")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use uuid::Uuid;

    use super::*;

    #[tokio::test]
    async fn validates_new_challenge_repository() {
        let repo = temp_repo("new-challenge");
        write_valid_public_challenge(&repo);

        let manifest = validate_challenge_creation_repository(&repo)
            .await
            .expect("new challenge should validate");

        assert_eq!(manifest.challenge_name.as_str(), "sample-sum");
        cleanup(&repo);
    }

    #[tokio::test]
    async fn rejects_new_version_repository() {
        let repo = temp_repo("new-version");
        write_valid_public_challenge(&repo);
        write_file(
            &repo.join(AGENTICS_CHALLENGE_MANIFEST_FILE),
            &json!({
                "schema_version": 1,
                "request": "new_version",
                "challenge_name": "sample-sum",
                "title": "Sample Sum",
                "summary": "Add numbers",
                "readme_path": "README.md",
                "bundle_path": "v1"
            })
            .to_string(),
        );

        let error = validate_challenge_creation_repository(&repo)
            .await
            .expect_err("new_version should be rejected");

        assert!(error.to_string().contains("new_version"));
        cleanup(&repo);
    }

    #[tokio::test]
    async fn validates_archive_request_repository() {
        let repo = temp_repo("archive");
        std::fs::create_dir_all(&repo).expect("repo");
        write_file(&repo.join("README.md"), "# Sample Sum\n");
        write_file(
            &repo.join(AGENTICS_CHALLENGE_MANIFEST_FILE),
            &json!({
                "schema_version": 1,
                "request": "archive_challenge",
                "challenge_name": "sample-sum",
                "title": "Sample Sum",
                "summary": "Add numbers",
                "readme_path": "README.md",
                "archive": { "reason": "Retired by challenge owner" }
            })
            .to_string(),
        );

        validate_challenge_creation_repository(&repo)
            .await
            .expect("archive should validate");
        cleanup(&repo);
    }

    #[tokio::test]
    async fn rejects_missing_readme() {
        let repo = temp_repo("missing-readme");
        write_valid_public_challenge(&repo);
        std::fs::remove_file(repo.join("README.md")).expect("remove readme");

        let error = validate_challenge_creation_repository(&repo)
            .await
            .expect_err("missing readme should fail");

        assert!(error.to_string().contains("readme_path"));
        cleanup(&repo);
    }

    #[test]
    fn rejects_invalid_lifecycle_shape() {
        let manifest = ChallengeCreationManifest {
            schema_version: 1,
            request: ChallengeCreationRequestKind::ArchiveChallenge,
            challenge_name: "sample-sum".parse().expect("valid challenge name"),
            title: "Sample Sum".to_string(),
            summary: "Add numbers".to_string(),
            readme_path: "README.md".to_string(),
            bundle_path: Some("v1".to_string()),
            archive: None,
            private_assets: Vec::new(),
            ci: Default::default(),
        };

        let error = validate_challenge_creation_manifest(&manifest)
            .expect_err("archive with bundle_path should fail");
        assert!(error.to_string().contains("bundle_path must be omitted"));
    }

    #[tokio::test]
    async fn rejects_private_material_in_public_repo() {
        let repo = temp_repo("private-leak");
        write_valid_public_challenge(&repo);
        std::fs::create_dir_all(repo.join("v1/private-benchmark")).expect("private dir");
        write_file(&repo.join("v1/private-benchmark/cases.json"), "[]\n");

        let error = validate_challenge_creation_repository(&repo)
            .await
            .expect_err("private material should fail");

        assert!(error.to_string().contains("private benchmark"));
        cleanup(&repo);
    }

    fn temp_repo(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("agentics-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("temp repo");
        path
    }

    fn cleanup(path: &Path) {
        drop(std::fs::remove_dir_all(path));
    }

    fn write_valid_public_challenge(repo: &Path) {
        let bundle = "v1";
        std::fs::create_dir_all(repo.join(bundle).join("public")).expect("public dir");
        write_file(&repo.join("README.md"), "# Sample Sum\n");
        write_file(&repo.join(bundle).join("statement.md"), "# Sample Sum\n");
        write_file(
            &repo.join(bundle).join("public/runs.json"),
            &json!({
                "runs": [
                    {
                        "run_name": "case-1",
                        "interface": "stdio",
                        "stdin_json": { "a": 1, "b": 2 },
                        "output_files": []
                    }
                ]
            })
            .to_string(),
        );
        write_file(
            &repo.join(bundle).join("spec.json"),
            &json!({
                "schema_version": 1,
                "challenge_name": "sample-sum",
                "challenge_title": "Sample Sum",
                "challenge_summary": "Add numbers",
                "solution": {
                    "protocol": "zip_project",
                    "manifest_file": "agentics.solution.json"
                },
                "scorer": {
                    "command": ["python", "scorer/run.py"],
                    "result_file": "result.json"
                },
                "targets": [
                    {
                        "name": "linux-arm64-cpu",
                        "docker_platform": "linux/arm64",
                        "accelerator": "cpu",
                        "validation_enabled": true,
                        "resource_profile": {
                            "name": "agentics-cpu-small",
                            "solution_image": "agentics-linux-arm64-cpu:ubuntu26.04-local",
                            "scorer_image": "agentics-linux-arm64-cpu:ubuntu26.04-local",
                            "timeout_sec": 30,
                            "memory_limit_mb": 512,
                            "cpu_limit_millis": 1000,
                            "disk_limit_mb": 1024,
                            "setup_network_access": "enabled",
                            "build_network_access": "disabled",
                            "run_network_access": "disabled",
                            "scorer_network_access": "disabled"
                        }
                    }
                ],
                "eligibility": { "type": "open" },
                "visibility": {
                    "leaderboard": "public_live",
                    "score_distribution": "public_live",
                    "result_detail": "submitter_live_public_after_close"
                },
                "solution_publication": "public",
                "execution": {
                    "validation_runs": "public/runs.json",
                    "official_runs": "private-benchmark/runs.json"
                },
                "datasets": {
                    "public_dir": "public",
                    "private_benchmark_dir": "private-benchmark",
                    "public_policy": "full",
                    "private_benchmark_policy": "score_only",
                    "private_benchmark_enabled": true
                },
                "metric_schema": {
                    "metrics": [
                        {
                            "name": "score",
                            "label": "Score",
                            "direction": "maximize",
                            "visibility": "public"
                        }
                    ],
                    "ranking": {
                        "primary_metric_name": "score"
                    }
                }
            })
            .to_string(),
        );

        let manifest = json!({
            "schema_version": 1,
            "request": "new_challenge",
            "challenge_name": "sample-sum",
            "title": "Sample Sum",
            "summary": "Add numbers",
            "readme_path": "README.md",
            "bundle_path": bundle,
            "private_assets": [
                {
                    "asset_name": "official-cases",
                    "kind": "private_benchmark_data",
                    "required": true
                }
            ]
        });
        write_file(
            &repo.join(AGENTICS_CHALLENGE_MANIFEST_FILE),
            &manifest.to_string(),
        );
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent dir");
        }
        std::fs::write(path, content).expect("write file");
    }
}
