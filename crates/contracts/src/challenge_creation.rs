//! Validation helpers for public GitHub challenge creation proposals.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::challenge_bundle::{
    read_challenge_bundle_spec, read_challenge_run_manifest, read_piped_stdio_session_manifest,
};
use crate::validation::text;
use agentics_domain::models::challenge::{MAX_CHALLENGE_KEYWORDS, MIN_CHALLENGE_KEYWORDS};
use agentics_domain::models::challenge_creation::{
    AGENTICS_CHALLENGE_MANIFEST_FILE, ChallengeCreationManifest, ChallengeCreationRequestKind,
    ChallengePrivateAssetRequirement, ChallengePrivateAssetResponse,
};
use agentics_domain::models::hashes::Sha256Digest;
use agentics_domain::models::paths::RepoRelativePath;
use agentics_error::{Result, ServiceError};

/// Read `agentics.challenge.json` from a proposal root.
pub async fn read_challenge_creation_manifest(root: &Path) -> Result<ChallengeCreationManifest> {
    let manifest_path = root.join(AGENTICS_CHALLENGE_MANIFEST_FILE);
    let raw = tokio::fs::read_to_string(&manifest_path).await?;
    let manifest: ChallengeCreationManifest = serde_json::from_str(&raw).map_err(|e| {
        ServiceError::Validation(format!("invalid {AGENTICS_CHALLENGE_MANIFEST_FILE}: {e}"))
    })?;
    validate_challenge_creation_manifest(&manifest)?;
    Ok(manifest)
}

/// Validate one challenge proposal directory and return the parsed manifest.
///
/// The directory is the challenge-level root inside a public repository, for
/// example `challenges/sample-sum/`. Private benchmark datasets, private
/// evaluator packages, seeds, and reference outputs must be uploaded through
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
        return Err(ServiceError::Validation(
            "schema_version must be 1".to_string(),
        ));
    }
    require_non_empty(&manifest.title, "title")?;
    require_non_empty(&manifest.summary.en, "summary.en")?;
    require_non_empty(&manifest.summary.zh, "summary.zh")?;
    validate_manifest_keywords(manifest)?;
    validate_private_asset_requirements(&manifest.private_assets)?;

    match manifest.request {
        ChallengeCreationRequestKind::NewChallenge => {
            let _bundle_path = manifest.bundle_path.as_ref().ok_or_else(|| {
                ServiceError::Validation("bundle_path is required for new_challenge".to_string())
            })?;
            if manifest.archive.is_some() {
                return Err(ServiceError::Validation(
                    "archive must be omitted for new_challenge".to_string(),
                ));
            }
        }
        ChallengeCreationRequestKind::ArchiveChallenge => {
            if manifest.bundle_path.is_some() {
                return Err(ServiceError::Validation(
                    "bundle_path must be omitted for archive_challenge".to_string(),
                ));
            }
            let archive = manifest.archive.as_ref().ok_or_else(|| {
                ServiceError::Validation("archive is required for archive_challenge".to_string())
            })?;
            require_non_empty(&archive.reason, "archive.reason")?;
        }
    }

    Ok(())
}

/// Return a stable SHA-256 digest of a normalized manifest JSON representation.
pub fn normalized_manifest_sha256(manifest: &ChallengeCreationManifest) -> Result<Sha256Digest> {
    let bytes = serde_json::to_vec(manifest).map_err(|e| ServiceError::Internal(e.to_string()))?;
    Ok(sha256_digest(&bytes))
}

/// Return a deterministic digest for the review_record content a reviewer validated.
///
/// The digest covers the normalized public manifest, the public bundle tree for
/// publishable requests, and the uploaded private asset nameentities. It is not a
/// replacement for a future server-side Git checkout at `commit_sha`, but it
/// gives validation, approval, and publish an exact content identity to compare
/// within the MVP trust boundary.
pub async fn challenge_review_bundle_sha256(
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
    private_assets: &[ChallengePrivateAssetResponse],
) -> Result<Sha256Digest> {
    let proposal_root = proposal_root.to_path_buf();
    let manifest = manifest.clone();
    let private_assets = private_assets.to_vec();
    tokio::task::spawn_blocking(move || {
        challenge_review_bundle_sha256_blocking(&proposal_root, &manifest, &private_assets)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("review_record digest task failed: {e}")))?
}

/// Return a deterministic digest for an assembled runtime bundle reviewed by an admin.
///
/// The digest covers the normalized public manifest and the fully assembled
/// runtime bundle after private overlays have been applied. This is the digest
/// approval and publish compare for publishable new challenges.
pub async fn challenge_review_runtime_bundle_sha256(
    runtime_bundle_root: &Path,
    manifest: &ChallengeCreationManifest,
) -> Result<Sha256Digest> {
    let runtime_bundle_root = runtime_bundle_root.to_path_buf();
    let manifest = manifest.clone();
    tokio::task::spawn_blocking(move || {
        challenge_review_runtime_bundle_sha256_blocking(&runtime_bundle_root, &manifest)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("runtime bundle digest task failed: {e}")))?
}

/// Return the SHA-256 digest of arbitrary bytes.
pub fn sha256_digest(bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Sha256Digest::from_bytes(hasher.finalize().into())
}

/// Handles review_record review bundle sha256 blocking for this module.
fn challenge_review_bundle_sha256_blocking(
    proposal_root: &Path,
    manifest: &ChallengeCreationManifest,
    private_assets: &[ChallengePrivateAssetResponse],
) -> Result<Sha256Digest> {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, "format", b"agentics-review_record-review-v1");

    let manifest_bytes =
        serde_json::to_vec(manifest).map_err(|e| ServiceError::Internal(e.to_string()))?;
    hash_field(&mut hasher, "manifest", &manifest_bytes);

    if let Some(bundle_path) = &manifest.bundle_path {
        let bundle_root = proposal_root.join(bundle_path.as_path());
        hash_public_tree(&mut hasher, &bundle_root)?;
    }

    let mut assets = private_assets.to_vec();
    assets.sort_by(|left, right| left.asset_name.cmp(&right.asset_name));
    for asset in assets {
        hash_field(&mut hasher, "asset_name", asset.asset_name.as_bytes());
        hash_field(&mut hasher, "asset_kind", asset.kind.as_str().as_bytes());
        hash_field(&mut hasher, "asset_required", &[u8::from(asset.required)]);
        hash_field(&mut hasher, "asset_size", &asset.size_bytes.to_be_bytes());
        hash_field(
            &mut hasher,
            "asset_sha256",
            asset.sha256.to_string().as_bytes(),
        );
    }

    Ok(Sha256Digest::from_bytes(hasher.finalize().into()))
}

/// Handles review_record review runtime bundle sha256 blocking for this module.
fn challenge_review_runtime_bundle_sha256_blocking(
    runtime_bundle_root: &Path,
    manifest: &ChallengeCreationManifest,
) -> Result<Sha256Digest> {
    let mut hasher = Sha256::new();
    hash_field(
        &mut hasher,
        "format",
        b"agentics-review_record-runtime-review-v1",
    );

    let manifest_bytes =
        serde_json::to_vec(manifest).map_err(|e| ServiceError::Internal(e.to_string()))?;
    hash_field(&mut hasher, "manifest", &manifest_bytes);
    hash_public_tree(&mut hasher, runtime_bundle_root)?;

    Ok(Sha256Digest::from_bytes(hasher.finalize().into()))
}

/// Handles hash public tree for this module.
fn hash_public_tree(hasher: &mut Sha256, bundle_root: &Path) -> Result<()> {
    let mut stack = vec![bundle_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = std::fs::read_dir(&dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            let relative_path = path.strip_prefix(bundle_root).map_err(|e| {
                ServiceError::Internal(format!("failed to build review digest: {e}"))
            })?;
            let relative_path = relative_path.to_str().ok_or_else(|| {
                ServiceError::Validation(format!(
                    "public bundle path must be UTF-8 for review digest: {}",
                    path.display()
                ))
            })?;

            if metadata.file_type().is_symlink() {
                return Err(ServiceError::Validation(format!(
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

/// Handles hash file for this module.
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
            ServiceError::Internal("file read exceeded digest buffer bounds".to_string())
        })?;
        hasher.update(chunk);
    }

    Ok(())
}

/// Handles hash field for this module.
fn hash_field(hasher: &mut Sha256, label: &str, bytes: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label.as_bytes());
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// Validates challenge creation repository with manifest invariants for this contract.
async fn validate_challenge_creation_repository_with_manifest(
    root: &Path,
    manifest: &ChallengeCreationManifest,
) -> Result<()> {
    if !tokio::fs::try_exists(root.join(AGENTICS_CHALLENGE_MANIFEST_FILE)).await? {
        return Err(ServiceError::Validation(format!(
            "{AGENTICS_CHALLENGE_MANIFEST_FILE} is required"
        )));
    }
    assert_public_file_exists(root.join(manifest.readme_path.as_path()), "readme_path").await?;
    reject_private_files(root)?;

    if let Some(bundle_path) = &manifest.bundle_path {
        validate_public_bundle(root, manifest, bundle_path).await?;
    }

    Ok(())
}

/// Validates public bundle invariants for this contract.
async fn validate_public_bundle(
    root: &Path,
    manifest: &ChallengeCreationManifest,
    bundle_path: &RepoRelativePath,
) -> Result<()> {
    let bundle_dir = root.join(bundle_path.as_path());
    let spec = read_challenge_bundle_spec(&bundle_dir).await?;
    if spec.challenge_name != manifest.challenge_name {
        return Err(ServiceError::Validation(format!(
            "bundle challenge_name mismatch: expected {}, got {}",
            manifest.challenge_name, spec.challenge_name
        )));
    }
    if spec.challenge_title != manifest.title {
        return Err(ServiceError::Validation(format!(
            "bundle challenge_title mismatch: expected {}, got {}",
            manifest.title, spec.challenge_title
        )));
    }
    if spec.summary != manifest.summary {
        return Err(ServiceError::Validation(format!(
            "bundle summary mismatch: expected {}, got {}",
            manifest.summary, spec.summary
        )));
    }
    if spec.keywords != manifest.keywords {
        return Err(ServiceError::Validation(
            "bundle keywords must match agentics.challenge.json keywords".to_string(),
        ));
    }
    assert_public_file_exists(bundle_dir.join("statement.md"), "statement.md").await?;
    assert_public_dir_exists(
        bundle_dir.join(spec.datasets.public_dir.as_path()),
        "datasets.public_dir",
    )
    .await?;
    if let Some(private_benchmark_dir) = spec.datasets.private_benchmark_dir.as_ref()
        && tokio::fs::try_exists(bundle_dir.join(private_benchmark_dir.as_path())).await?
    {
        return Err(ServiceError::Validation(format!(
            "datasets.private_benchmark_dir `{private_benchmark_dir}` must be provided through private asset uploads, not committed to the public challenge repository"
        )));
    }

    if spec.targets.iter().any(|target| target.validation_enabled) {
        match &spec.execution {
            agentics_domain::models::challenge::ChallengeExecutionSpec::SeparatedEvaluator(
                execution,
            ) => {
                if let Some(validation_runs) = &execution.validation_runs {
                    assert_public_file_exists(
                        bundle_dir.join(validation_runs.as_path()),
                        "execution.validation_runs",
                    )
                    .await?;
                    let manifest =
                        read_challenge_run_manifest(&bundle_dir, validation_runs).await?;
                    crate::challenge_bundle::validate_challenge_run_manifest_sources(
                        &bundle_dir,
                        &manifest,
                    )
                    .await?;
                }
            }
            agentics_domain::models::challenge::ChallengeExecutionSpec::PipedStdio(execution) => {
                if let Some(validation_session) = &execution.validation_session {
                    assert_public_file_exists(
                        bundle_dir.join(validation_session.as_path()),
                        "execution.validation_session",
                    )
                    .await?;
                    let manifest =
                        read_piped_stdio_session_manifest(&bundle_dir, validation_session).await?;
                    crate::challenge_bundle::validate_piped_stdio_session_manifest_sources(
                        &bundle_dir,
                        &manifest,
                    )
                    .await?;
                }
            }
            agentics_domain::models::challenge::ChallengeExecutionSpec::CoexecutedBenchmark(_) => {}
        }
    }

    Ok(())
}

/// Validate public challenge keywords in the proposal manifest.
fn validate_manifest_keywords(manifest: &ChallengeCreationManifest) -> Result<()> {
    if !(MIN_CHALLENGE_KEYWORDS..=MAX_CHALLENGE_KEYWORDS).contains(&manifest.keywords.len()) {
        return Err(ServiceError::Validation(format!(
            "keywords must contain between {MIN_CHALLENGE_KEYWORDS} and {MAX_CHALLENGE_KEYWORDS} entries"
        )));
    }
    let mut seen = HashSet::new();
    for keyword in &manifest.keywords {
        let normalized = keyword.as_str().to_lowercase();
        if !seen.insert(normalized) {
            return Err(ServiceError::Validation(format!(
                "duplicate challenge keyword `{keyword}`"
            )));
        }
    }
    Ok(())
}

/// Handles assert public file exists for this module.
async fn assert_public_file_exists(path: PathBuf, field: &str) -> Result<()> {
    let meta = tokio::fs::metadata(&path).await.map_err(|_| {
        ServiceError::Validation(format!("{field} does not exist: {}", path.display()))
    })?;
    if !meta.is_file() {
        return Err(ServiceError::Validation(format!(
            "{field} is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

/// Handles assert public dir exists for this module.
async fn assert_public_dir_exists(path: PathBuf, field: &str) -> Result<()> {
    let meta = tokio::fs::metadata(&path).await.map_err(|_| {
        ServiceError::Validation(format!("{field} does not exist: {}", path.display()))
    })?;
    if !meta.is_dir() {
        return Err(ServiceError::Validation(format!(
            "{field} is not a directory: {}",
            path.display()
        )));
    }
    Ok(())
}

/// Validates private asset requirements invariants for this contract.
fn validate_private_asset_requirements(
    private_assets: &[ChallengePrivateAssetRequirement],
) -> Result<()> {
    let mut ids = HashSet::with_capacity(private_assets.len());
    for asset in private_assets {
        if !ids.insert(asset.asset_name.as_str()) {
            return Err(ServiceError::Validation(format!(
                "private_assets contains duplicate asset_name `{}`",
                asset.asset_name
            )));
        }
        if let Some(note) = &asset.asset_note {
            require_non_empty(note, "private_assets[].asset_note")?;
        }
        let mut required_paths = HashSet::with_capacity(asset.required_paths.len());
        for path in &asset.required_paths {
            if !required_paths.insert(path.as_str()) {
                return Err(ServiceError::Validation(format!(
                    "private_assets `{}` contains duplicate required_paths entry `{path}`",
                    asset.asset_name
                )));
            }
        }
    }
    Ok(())
}

/// Requires non empty and reports a domain error otherwise.
fn require_non_empty(value: &str, field: &str) -> Result<()> {
    text::require_non_empty(value, field)
}

/// Handles reject private files for this module.
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
                return Err(ServiceError::Validation(format!(
                    "public challenge repo must not contain private benchmark or secret material: {}",
                    path.display()
                )));
            }

            let meta = std::fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                stack.push(path);
            } else if meta.file_type().is_symlink() {
                return Err(ServiceError::Validation(format!(
                    "public challenge repo must not contain symlinks: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

/// Returns whether forbidden public repo name holds.
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
    use serde_json::json;
    use uuid::Uuid;

    use agentics_domain::models::localization::LocalizedText;

    use super::*;

    /// Build the standard localized challenge summary for creation tests.
    fn localized_summary() -> LocalizedText {
        LocalizedText::new("Add numbers", "数字求和")
    }

    /// Verifies that validates new challenge repository.
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

    /// Verifies that new challenge proposals must declare catalog keywords.
    #[tokio::test]
    async fn rejects_new_challenge_without_keywords() {
        let repo = temp_repo("new-challenge-no-keywords");
        write_valid_public_challenge(&repo);
        write_file(
            &repo.join(AGENTICS_CHALLENGE_MANIFEST_FILE),
            &json!({
                "schema_version": 1,
                "request": "new_challenge",
                "challenge_name": "sample-sum",
                "title": "Sample Sum",
                "summary": { "en": "Add numbers", "zh": "数字求和" },
                "keywords": [],
                "readme_path": "README.md",
                "bundle_path": "v1"
            })
            .to_string(),
        );

        let error = validate_challenge_creation_repository(&repo)
            .await
            .expect_err("empty keywords should fail");

        assert!(error.to_string().contains("keywords must contain between"));
        cleanup(&repo);
    }

    /// Verifies that rejects new version repository.
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
                "summary": { "en": "Add numbers", "zh": "数字求和" },
                "keywords": ["arithmetic"],
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

    /// Verifies that validates archive request repository.
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
                "summary": { "en": "Add numbers", "zh": "数字求和" },
                "keywords": ["arithmetic"],
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

    /// Verifies that rejects missing readme.
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

    /// Verifies that rejects invalid lifecycle shape.
    #[test]
    fn rejects_invalid_lifecycle_shape() {
        let manifest = ChallengeCreationManifest {
            schema_version: 1,
            request: ChallengeCreationRequestKind::ArchiveChallenge,
            challenge_name: "sample-sum".parse().expect("valid challenge name"),
            title: "Sample Sum".to_string(),
            summary: localized_summary(),
            keywords: vec!["arithmetic".parse().expect("valid keyword")],
            readme_path: "README.md".parse().expect("valid readme path"),
            bundle_path: Some("v1".parse().expect("valid bundle path")),
            archive: None,
            private_assets: Vec::new(),
            ci: Default::default(),
        };

        let error = validate_challenge_creation_manifest(&manifest)
            .expect_err("archive with bundle_path should fail");
        assert!(error.to_string().contains("bundle_path must be omitted"));
    }

    /// Verifies that private asset required policy is explicit in proposal manifests.
    #[test]
    fn private_asset_required_field_is_required() {
        let manifest = json!({
            "schema_version": 1,
            "request": "new_challenge",
            "challenge_name": "sample-sum",
            "title": "Sample Sum",
            "summary": { "en": "Add numbers", "zh": "数字求和" },
            "readme_path": "README.md",
            "bundle_path": "v1",
            "private_assets": [
                {
                    "asset_name": "official-cases",
                    "kind": "private_benchmark_data"
                }
            ]
        });

        let error = serde_json::from_value::<ChallengeCreationManifest>(manifest)
            .expect_err("missing private asset required flag should fail");

        assert!(error.to_string().contains("required"));
    }

    /// Verifies that required private asset paths are safe and unique.
    #[test]
    fn private_asset_required_paths_are_safe_and_unique() {
        let manifest = ChallengeCreationManifest {
            schema_version: 1,
            request: ChallengeCreationRequestKind::NewChallenge,
            challenge_name: "sample-sum".parse().expect("valid challenge name"),
            title: "Sample Sum".to_string(),
            summary: localized_summary(),
            keywords: vec!["arithmetic".parse().expect("valid keyword")],
            readme_path: "README.md".parse().expect("valid readme path"),
            bundle_path: Some("v1".parse().expect("valid bundle path")),
            archive: None,
            private_assets: vec![ChallengePrivateAssetRequirement {
                asset_name: "official-cases".parse().expect("valid asset name"),
                kind: agentics_domain::models::challenge_creation::ChallengePrivateAssetKind::PrivateBenchmarkData,
                required: true,
                required_paths: vec![
                    "private-benchmark/runs.json"
                        .parse()
                        .expect("valid bundle path"),
                    "private-benchmark/runs.json"
                        .parse()
                        .expect("valid bundle path"),
                ],
                asset_note: None,
            }],
            ci: Default::default(),
        };

        let error = validate_challenge_creation_manifest(&manifest)
            .expect_err("duplicate required paths should fail");
        assert!(error.to_string().contains("duplicate required_paths"));

        let manifest = json!({
            "schema_version": 1,
            "request": "new_challenge",
            "challenge_name": "sample-sum",
            "title": "Sample Sum",
            "summary": { "en": "Add numbers", "zh": "数字求和" },
            "readme_path": "README.md",
            "bundle_path": "v1",
            "private_assets": [
                {
                    "asset_name": "official-cases",
                    "kind": "private_benchmark_data",
                    "required": true,
                    "required_paths": ["../private-benchmark/runs.json"]
                }
            ]
        });

        let error = serde_json::from_value::<ChallengeCreationManifest>(manifest)
            .expect_err("unsafe required path should fail");
        assert!(error.to_string().contains("safe relative paths"));
    }

    /// Verifies that rejects private material in public repo.
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

    /// Verifies that public bundle validation rejects declared private benchmark directories.
    #[tokio::test]
    async fn rejects_declared_private_benchmark_directory_in_public_bundle() {
        let repo = temp_repo("declared-private-dir");
        write_valid_public_challenge(&repo);
        std::fs::create_dir_all(repo.join("v1/official-cases")).expect("official cases dir");
        let spec_path = repo.join("v1/spec.json");
        let mut spec: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("spec should read"))
                .expect("spec should parse");
        spec["datasets"]["private_benchmark_dir"] = json!("official-cases");
        spec["execution"]["official_runs"] = json!("official-cases/runs.json");
        write_file(&spec_path, &spec.to_string());

        let error = validate_challenge_creation_repository(&repo)
            .await
            .expect_err("declared private benchmark directory should fail");

        assert!(error.to_string().contains("private asset uploads"));
        cleanup(&repo);
    }

    /// Verifies that public bundle validation checks piped-stdio validation session inputs.
    #[tokio::test]
    async fn rejects_missing_piped_stdio_validation_session_source() {
        let repo = temp_repo("piped-session-source");
        write_valid_public_challenge(&repo);
        let spec_path = repo.join("v1/spec.json");
        let mut spec: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&spec_path).expect("spec should read"))
                .expect("spec should parse");
        spec["execution"] = json!({
            "mode": "piped_stdio",
            "acknowledge_stdio_protocol_framing": true,
            "interactive_evaluator": {
                "command": ["python", "interactive-evaluator/run.py"],
                "result_file": "result.json"
            },
            "validation_session": "public/session.json",
            "official_session": "private-benchmark/session.json"
        });
        write_file(&spec_path, &spec.to_string());
        write_file(
            &repo.join("v1/interactive-evaluator/run.py"),
            "print('ok')\n",
        );
        write_file(
            &repo.join("v1/public/session.json"),
            &json!({
                "session_name": "case-1",
                "input_files": [
                    {
                        "path": "prompt.txt",
                        "source_path": "public/missing-prompt.txt"
                    }
                ]
            })
            .to_string(),
        );

        let error = validate_challenge_creation_repository(&repo)
            .await
            .expect_err("missing piped session source should fail");

        assert!(
            error
                .to_string()
                .contains("session.input_files[].source_path does not exist")
        );
        cleanup(&repo);
    }

    /// Handles temp repo for this module.
    fn temp_repo(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("agentics-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("temp repo");
        path
    }

    /// Handles cleanup for this module.
    fn cleanup(path: &Path) {
        drop(std::fs::remove_dir_all(path));
    }

    /// Writes valid public challenge to the target path.
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
                "summary": { "en": "Add numbers", "zh": "数字求和" },
                "keywords": ["arithmetic"],
                "solution": {
                    "protocol": "zip_project",
                    "manifest_file": "agentics.solution.json"
                },
                "targets": [
                    {
                        "name": "linux-arm64-cpu",
                        "docker_platform": "linux/arm64",
                        "accelerator": null,
                        "validation_enabled": true,
                        "resource_profile": {
                            "name": "agentics-cpu-small",
                            "solution_image": {
                                "source": "local",
                                "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                            },
                            "evaluator_image": {
                                "source": "local",
                                "reference": "agentics-linux-arm64-cpu:ubuntu26.04-local"
                            },
                            "solution": {
                                "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "enabled"},
                                "build": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"},
                                "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                            },
                            "evaluator": {
                                "setup": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "enabled"},
                                "run": {"timeout_sec": 30, "memory_limit_mb": 512, "cpu_limit_millis": 1000, "disk_limit_mb": 1024, "network_access": "disabled"}
                            }
                        }
                    }
                ],
                "starts_at": "2026-01-01T00:00:00Z",
                "eligibility": { "type": "open" },
                "visibility": {
                    "leaderboard": "public_live",
                    "score_distribution": "public_live",
                    "result_detail": "submitter_live_public_after_close"
                },
                "solution_publication": "public",
                "execution": {
                    "mode": "separated_evaluator",
                    "separated_evaluator": {
                        "command": ["python", "separated-evaluator/run.py"],
                        "result_file": "result.json"
                    },
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
            "summary": { "en": "Add numbers", "zh": "数字求和" },
            "keywords": ["arithmetic"],
            "readme_path": "README.md",
            "bundle_path": bundle,
            "private_assets": [
                {
                    "asset_name": "official-cases",
                    "kind": "private_benchmark_data",
                    "required": true,
                    "required_paths": ["private-benchmark/runs.json"]
                }
            ]
        });
        write_file(
            &repo.join(AGENTICS_CHALLENGE_MANIFEST_FILE),
            &manifest.to_string(),
        );
    }

    /// Writes file to the target path.
    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent dir");
        }
        std::fs::write(path, content).expect("write file");
    }
}
