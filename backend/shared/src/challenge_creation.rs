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
    ChallengeCreationVersionSpec, ChallengePrivateAssetRequirement,
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
    validate_challenge_namespace(&manifest.challenge_id)?;
    require_non_empty(&manifest.title, "title")?;
    require_non_empty(&manifest.summary, "summary")?;
    require_safe_relative_path(&manifest.readme_path, "readme_path")?;
    validate_private_asset_requirements(&manifest.private_assets)?;

    match manifest.request {
        ChallengeCreationRequestKind::NewChallenge => {
            let version = manifest.version.as_ref().ok_or_else(|| {
                AppError::Validation("version is required for new_challenge".to_string())
            })?;
            if manifest.archive.is_some() {
                return Err(AppError::Validation(
                    "archive must be omitted for new_challenge".to_string(),
                ));
            }
            if version.supersedes_version.is_some() {
                return Err(AppError::Validation(
                    "version.supersedes_version must be omitted for new_challenge".to_string(),
                ));
            }
            validate_version_spec(version)?;
        }
        ChallengeCreationRequestKind::NewVersion => {
            let version = manifest.version.as_ref().ok_or_else(|| {
                AppError::Validation("version is required for new_version".to_string())
            })?;
            if manifest.archive.is_some() {
                return Err(AppError::Validation(
                    "archive must be omitted for new_version".to_string(),
                ));
            }
            let supersedes = version.supersedes_version.as_deref().ok_or_else(|| {
                AppError::Validation(
                    "version.supersedes_version is required for new_version".to_string(),
                )
            })?;
            validate_version_string(supersedes, "version.supersedes_version")?;
            validate_version_spec(version)?;
        }
        ChallengeCreationRequestKind::ArchiveChallenge => {
            if manifest.version.is_some() {
                return Err(AppError::Validation(
                    "version must be omitted for archive_challenge".to_string(),
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

/// Return the hex SHA-256 digest of arbitrary bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Check whether a challenge id is valid in the public repository namespace.
pub fn is_valid_challenge_namespace(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(3..=63).contains(&bytes.len()) {
        return false;
    }
    if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return false;
    }
    if value.contains("--") {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
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

    if let Some(version) = &manifest.version {
        validate_public_bundle(root, manifest, version).await?;
    }

    Ok(())
}

async fn validate_public_bundle(
    root: &Path,
    manifest: &ChallengeCreationManifest,
    version: &ChallengeCreationVersionSpec,
) -> Result<()> {
    let bundle_dir = root.join(&version.bundle_path);
    let spec = read_challenge_bundle_spec(&bundle_dir).await?;
    if spec.challenge_id != manifest.challenge_id {
        return Err(AppError::Validation(format!(
            "bundle challenge_id mismatch: expected {}, got {}",
            manifest.challenge_id, spec.challenge_id
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
    if spec.challenge_version != version.version {
        return Err(AppError::Validation(format!(
            "bundle challenge_version mismatch: expected {}, got {}",
            version.version, spec.challenge_version
        )));
    }

    assert_public_file_exists(bundle_dir.join("statement.md"), "statement.md").await?;
    assert_public_dir_exists(
        bundle_dir.join(&spec.datasets.public_dir),
        "datasets.public_dir",
    )
    .await?;

    if spec
        .benchmark_targets
        .iter()
        .any(|target| target.validation_enabled)
    {
        let validation_runs = spec.execution.validation_runs.as_deref().ok_or_else(|| {
            AppError::Validation(
                "execution.validation_runs is required when validation is enabled".to_string(),
            )
        })?;
        assert_public_file_exists(
            bundle_dir.join(validation_runs),
            "execution.validation_runs",
        )
        .await?;
        read_challenge_run_manifest(&bundle_dir, validation_runs).await?;
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

fn validate_challenge_namespace(value: &str) -> Result<()> {
    if !is_valid_challenge_namespace(value) {
        return Err(AppError::Validation(
            "challenge_id must be 3-63 lowercase ASCII letters, digits, or single hyphens, and must start and end with a letter or digit"
                .to_string(),
        ));
    }
    Ok(())
}

fn validate_version_spec(version: &ChallengeCreationVersionSpec) -> Result<()> {
    validate_version_string(&version.version, "version.version")?;
    require_safe_relative_path(&version.bundle_path, "version.bundle_path")
}

fn validate_version_string(value: &str, field: &str) -> Result<()> {
    require_non_empty(value, field)?;
    let Some(rest) = value.strip_prefix('v') else {
        return Err(AppError::Validation(format!("{field} must start with `v`")));
    };
    if rest.is_empty()
        || rest
            .split('.')
            .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(AppError::Validation(format!(
            "{field} must use a version like v1, v1.2, or v1.2.3"
        )));
    }
    Ok(())
}

fn validate_private_asset_requirements(
    private_assets: &[ChallengePrivateAssetRequirement],
) -> Result<()> {
    let mut ids = HashSet::with_capacity(private_assets.len());
    for asset in private_assets {
        validate_identifier(&asset.asset_id, "private_assets[].asset_id")?;
        if !ids.insert(asset.asset_id.as_str()) {
            return Err(AppError::Validation(format!(
                "private_assets contains duplicate asset_id `{}`",
                asset.asset_id
            )));
        }
        if let Some(note) = &asset.asset_note {
            require_non_empty(note, "private_assets[].asset_note")?;
        }
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> Result<()> {
    require_non_empty(value, field)?;
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(AppError::Validation(format!(
            "{field} must contain only ASCII letters, digits, underscores, hyphens, or dots"
        )));
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
        write_valid_public_challenge(
            &repo,
            ChallengeCreationRequestKind::NewChallenge,
            "v1",
            None,
        );

        let manifest = validate_challenge_creation_repository(&repo)
            .await
            .expect("new challenge should validate");

        assert_eq!(manifest.challenge_id, "sample-sum");
        cleanup(&repo);
    }

    #[tokio::test]
    async fn validates_new_version_repository() {
        let repo = temp_repo("new-version");
        write_valid_public_challenge(
            &repo,
            ChallengeCreationRequestKind::NewVersion,
            "v2",
            Some("v1"),
        );

        let manifest = validate_challenge_creation_repository(&repo)
            .await
            .expect("new version should validate");

        assert_eq!(
            manifest.version.expect("version").supersedes_version,
            Some("v1".to_string())
        );
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
                "challenge_id": "sample-sum",
                "title": "Sample Sum",
                "summary": "Add numbers",
                "readme_path": "README.md",
                "archive": { "reason": "Superseded by a better benchmark" }
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
        write_valid_public_challenge(
            &repo,
            ChallengeCreationRequestKind::NewChallenge,
            "v1",
            None,
        );
        std::fs::remove_file(repo.join("README.md")).expect("remove readme");

        let error = validate_challenge_creation_repository(&repo)
            .await
            .expect_err("missing readme should fail");

        assert!(error.to_string().contains("readme_path"));
        cleanup(&repo);
    }

    #[test]
    fn rejects_invalid_namespace() {
        assert!(!is_valid_challenge_namespace("Bad_ID"));
        assert!(!is_valid_challenge_namespace("-bad"));
        assert!(!is_valid_challenge_namespace("bad--id"));
        assert!(is_valid_challenge_namespace("sample-sum"));
    }

    #[test]
    fn rejects_invalid_lifecycle_shape() {
        let manifest = ChallengeCreationManifest {
            schema_version: 1,
            request: ChallengeCreationRequestKind::ArchiveChallenge,
            challenge_id: "sample-sum".to_string(),
            title: "Sample Sum".to_string(),
            summary: "Add numbers".to_string(),
            readme_path: "README.md".to_string(),
            version: Some(ChallengeCreationVersionSpec {
                version: "v1".to_string(),
                bundle_path: "versions/v1".to_string(),
                supersedes_version: None,
            }),
            archive: None,
            private_assets: Vec::new(),
            ci: Default::default(),
        };

        let error = validate_challenge_creation_manifest(&manifest)
            .expect_err("archive with version should fail");
        assert!(error.to_string().contains("version must be omitted"));
    }

    #[tokio::test]
    async fn rejects_private_material_in_public_repo() {
        let repo = temp_repo("private-leak");
        write_valid_public_challenge(
            &repo,
            ChallengeCreationRequestKind::NewChallenge,
            "v1",
            None,
        );
        std::fs::create_dir_all(repo.join("versions/v1/private-benchmark")).expect("private dir");
        write_file(
            &repo.join("versions/v1/private-benchmark/cases.json"),
            "[]\n",
        );

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
        let _ = std::fs::remove_dir_all(path);
    }

    fn write_valid_public_challenge(
        repo: &Path,
        request: ChallengeCreationRequestKind,
        version: &str,
        supersedes_version: Option<&str>,
    ) {
        std::fs::create_dir_all(repo.join("versions").join(version).join("public"))
            .expect("public dir");
        write_file(&repo.join("README.md"), "# Sample Sum\n");
        write_file(
            &repo.join("versions").join(version).join("statement.md"),
            "# Sample Sum\n",
        );
        write_file(
            &repo.join("versions").join(version).join("public/runs.json"),
            &json!({
                "runs": [
                    {
                        "run_id": "case-1",
                        "interface": "stdio",
                        "stdin_json": { "a": 1, "b": 2 },
                        "output_files": []
                    }
                ]
            })
            .to_string(),
        );
        write_file(
            &repo.join("versions").join(version).join("spec.json"),
            &json!({
                "schema_version": 1,
                "challenge_id": "sample-sum",
                "challenge_title": "Sample Sum",
                "challenge_summary": "Add numbers",
                "challenge_version": version,
                "solution": {
                    "protocol": "zip_project",
                    "manifest_file": "agentics.solution.json"
                },
                "scorer": {
                    "command": ["python", "scorer/run.py"],
                    "result_file": "result.json"
                },
                "benchmark_targets": [
                    {
                        "id": "cpu-linux-arm64",
                        "docker_platform": "linux/arm64",
                        "accelerator": "cpu",
                        "validation_enabled": true,
                        "resource_profile": {
                            "id": "python-cpu-small",
                            "solution_image": "python:3.12-slim-bookworm",
                            "scorer_image": "python:3.12-slim-bookworm",
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
                            "id": "score",
                            "label": "Score",
                            "direction": "maximize",
                            "visibility": "public"
                        }
                    ],
                    "ranking": {
                        "primary_metric_id": "score"
                    }
                }
            })
            .to_string(),
        );

        let manifest = match request {
            ChallengeCreationRequestKind::NewChallenge => json!({
                "schema_version": 1,
                "request": "new_challenge",
                "challenge_id": "sample-sum",
                "title": "Sample Sum",
                "summary": "Add numbers",
                "readme_path": "README.md",
                "version": {
                    "version": version,
                    "bundle_path": format!("versions/{version}")
                },
                "private_assets": [
                    {
                        "asset_id": "official-cases",
                        "kind": "private_benchmark_data",
                        "required": true
                    }
                ]
            }),
            ChallengeCreationRequestKind::NewVersion => json!({
                "schema_version": 1,
                "request": "new_version",
                "challenge_id": "sample-sum",
                "title": "Sample Sum",
                "summary": "Add numbers",
                "readme_path": "README.md",
                "version": {
                    "version": version,
                    "bundle_path": format!("versions/{version}"),
                    "supersedes_version": supersedes_version.expect("supersedes version")
                }
            }),
            ChallengeCreationRequestKind::ArchiveChallenge => unreachable!(),
        };
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
