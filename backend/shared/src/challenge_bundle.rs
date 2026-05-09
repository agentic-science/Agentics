//! Helpers for loading and validating filesystem challenge bundles.
//!
//! Challenge bundles are the public contract between seeded/admin-authored
//! challenges and the runner. Validation accepts the relaxed JSON shape used by
//! the platform: optional nullable fields may be omitted, but contract names are
//! kept explicit and canonical.

use std::collections::HashSet;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{AppError, Result};
use crate::models::challenge::{
    BenchmarkAccelerator, BenchmarkTargetSpec, ChallengeBundleSpec, ChallengePrepareSpec,
    ChallengeRunInputFile, ChallengeRunManifest, ChallengeRunSpec, DockerPlatform,
    PrivateBenchmarkPolicy, ResourceProfileSpec,
};
use crate::zip_project::{ZIP_PROJECT_MANIFEST_FILE, ZIP_PROJECT_PROTOCOL};

/// Read `spec.json` from a bundle directory and validate its contract fields.
pub async fn read_challenge_bundle_spec(bundle_dir: &Path) -> Result<ChallengeBundleSpec> {
    let spec_path = bundle_dir.join("spec.json");
    let raw = tokio::fs::read_to_string(&spec_path).await?;
    let spec: ChallengeBundleSpec = serde_json::from_str(&raw)
        .map_err(|e| AppError::Validation(format!("invalid spec.json: {e}")))?;
    validate_challenge_bundle_spec(&spec)?;
    Ok(spec)
}

/// Read and validate one challenge-owned run manifest from a bundle directory.
pub async fn read_challenge_run_manifest(
    bundle_dir: &Path,
    manifest_path: &str,
) -> Result<ChallengeRunManifest> {
    require_safe_relative_path(manifest_path, "execution run manifest")?;
    read_challenge_run_manifest_file(
        &bundle_dir.join(manifest_path),
        &format!("run manifest {manifest_path}"),
    )
    .await
}

/// Read and validate a challenge-owned run manifest from an already resolved path.
pub async fn read_challenge_run_manifest_file(
    manifest_file: &Path,
    label: &str,
) -> Result<ChallengeRunManifest> {
    let raw = tokio::fs::read_to_string(manifest_file).await?;
    let manifest: ChallengeRunManifest = serde_json::from_str(&raw)
        .map_err(|e| AppError::Validation(format!("invalid {label}: {e}")))?;
    validate_challenge_run_manifest(&manifest)?;
    Ok(manifest)
}

/// Validate that a challenge bundle has the required files and declared data directories.
pub async fn validate_challenge_bundle(bundle_dir: &Path) -> Result<()> {
    let spec = read_challenge_bundle_spec(bundle_dir).await?;
    let spec_path = bundle_dir.join("spec.json");
    let statement_path = bundle_dir.join("statement.md");
    let public_dir = bundle_dir.join(&spec.datasets.public_dir);

    assert_path_type(&spec_path, "file", "spec.json").await?;
    assert_path_type(&statement_path, "file", "statement.md").await?;
    if let Some(script_path) = declared_scorer_script(&spec.scorer.command) {
        assert_path_type(&bundle_dir.join(script_path), "file", "scorer script").await?;
    }
    for (label, prepare) in [
        (
            "validation prepare script",
            spec.execution.validation_prepare.as_ref(),
        ),
        (
            "official prepare script",
            spec.execution.official_prepare.as_ref(),
        ),
    ] {
        if let Some(prepare) = prepare
            && let Some(script_path) = declared_scorer_script(&prepare.command)
        {
            assert_path_type(&bundle_dir.join(script_path), "file", label).await?;
        }
    }
    assert_path_type(&public_dir, "directory", "public data dir").await?;

    if spec
        .benchmark_targets
        .iter()
        .any(|target| target.validation_enabled)
        && let Some(validation_runs) = spec.execution.validation_runs.as_deref()
    {
        assert_path_type(
            &bundle_dir.join(validation_runs),
            "file",
            "validation run manifest",
        )
        .await?;
        let manifest = read_challenge_run_manifest(bundle_dir, validation_runs).await?;
        validate_challenge_run_manifest_sources(bundle_dir, &manifest).await?;
    }

    if spec.datasets.private_benchmark_enabled {
        if let Some(ref private_benchmark_dir) = spec.datasets.private_benchmark_dir {
            assert_path_type(
                &bundle_dir.join(private_benchmark_dir),
                "directory",
                "private benchmark data dir",
            )
            .await?;
        }
        if let Some(official_runs) = spec.execution.official_runs.as_deref() {
            assert_path_type(
                &bundle_dir.join(official_runs),
                "file",
                "official run manifest",
            )
            .await?;
            let manifest = read_challenge_run_manifest(bundle_dir, official_runs).await?;
            validate_challenge_run_manifest_sources(bundle_dir, &manifest).await?;
        }
    }

    Ok(())
}

async fn assert_path_type(path: &Path, kind: &str, label: &str) -> Result<()> {
    let meta = tokio::fs::metadata(path).await.map_err(|_| {
        AppError::Validation(format!("{} does not exist: {}", label, path.display()))
    })?;

    if kind == "file" && !meta.is_file() {
        return Err(AppError::Validation(format!(
            "{} is not a file: {}",
            label,
            path.display()
        )));
    }
    if kind == "directory" && !meta.is_dir() {
        return Err(AppError::Validation(format!(
            "{} is not a directory: {}",
            label,
            path.display()
        )));
    }

    Ok(())
}

/// Return whether `value` can be safely joined under a bundle root.
pub fn is_safe_relative_path(value: &str) -> bool {
    if value.starts_with('/') {
        return false;
    }
    value.split(['/', '\\']).all(|s| !s.is_empty() && s != "..")
}

/// Return a deterministic SHA-256 digest of all files in a bundle tree.
pub async fn challenge_bundle_tree_sha256(bundle_root: &Path) -> Result<String> {
    let bundle_root = bundle_root.to_path_buf();
    tokio::task::spawn_blocking(move || challenge_bundle_tree_sha256_blocking(&bundle_root))
        .await
        .map_err(|e| AppError::Internal(format!("bundle digest task failed: {e}")))?
}

/// Copy a challenge bundle directory while rejecting symlinks.
pub async fn copy_challenge_bundle_dir(
    source: &Path,
    target: &Path,
    replace_existing: bool,
) -> Result<()> {
    let source = source.to_path_buf();
    let target = target.to_path_buf();
    tokio::task::spawn_blocking(move || {
        copy_challenge_bundle_dir_blocking(&source, &target, replace_existing)
    })
    .await
    .map_err(|e| AppError::Internal(format!("bundle copy task failed: {e}")))?
}

fn challenge_bundle_tree_sha256_blocking(bundle_root: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    hash_bundle_tree(&mut hasher, bundle_root)?;
    Ok(hex::encode(hasher.finalize()))
}

fn hash_bundle_tree(hasher: &mut Sha256, bundle_root: &Path) -> Result<()> {
    let mut stack = vec![bundle_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries = std::fs::read_dir(&dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.path());

        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            let relative_path = path
                .strip_prefix(bundle_root)
                .map_err(|e| AppError::Internal(format!("failed to build bundle digest: {e}")))?;
            let relative_path = relative_path.to_str().ok_or_else(|| {
                AppError::Validation(format!(
                    "bundle path must be UTF-8 for digesting: {}",
                    path.display()
                ))
            })?;

            if metadata.file_type().is_symlink() {
                return Err(AppError::Validation(format!(
                    "challenge bundle must not contain symlinks: {}",
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

fn copy_challenge_bundle_dir_blocking(
    source: &Path,
    target: &Path,
    replace_existing: bool,
) -> Result<()> {
    if target.exists() {
        if !replace_existing {
            if target.is_dir() {
                return Ok(());
            }
            return Err(AppError::Validation(format!(
                "managed bundle target exists and is not a directory: {}",
                target.display()
            )));
        }
        std::fs::remove_dir_all(target)?;
    }
    std::fs::create_dir_all(target)?;

    let mut stack = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((current_source, current_target)) = stack.pop() {
        for entry in std::fs::read_dir(&current_source)? {
            let entry = entry?;
            let source_path = entry.path();
            let target_path = current_target.join(entry.file_name());
            let meta = std::fs::symlink_metadata(&source_path)?;
            if meta.file_type().is_symlink() {
                return Err(AppError::Validation(format!(
                    "challenge bundle must not contain symlinks: {}",
                    source_path.display()
                )));
            }
            if meta.is_dir() {
                std::fs::create_dir_all(&target_path)?;
                stack.push((source_path, target_path));
            } else if meta.is_file() {
                if let Some(parent) = target_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&source_path, &target_path)?;
            }
        }
    }

    Ok(())
}

fn validate_challenge_bundle_spec(spec: &ChallengeBundleSpec) -> Result<()> {
    require_non_empty(&spec.challenge_id, "challenge_id")?;
    require_non_empty(&spec.challenge_title, "challenge_title")?;
    require_non_empty(&spec.challenge_summary, "challenge_summary")?;
    require_non_empty(&spec.challenge_version, "challenge_version")?;

    if spec.schema_version != 1 {
        return Err(AppError::Validation("schema_version must be 1".to_string()));
    }
    if spec.solution.protocol != ZIP_PROJECT_PROTOCOL {
        return Err(AppError::Validation(format!(
            "solution.protocol must be {ZIP_PROJECT_PROTOCOL}"
        )));
    }
    require_safe_relative_path(&spec.solution.manifest_file, "solution.manifest_file")?;
    if spec.solution.manifest_file != ZIP_PROJECT_MANIFEST_FILE {
        return Err(AppError::Validation(format!(
            "solution.manifest_file must be {ZIP_PROJECT_MANIFEST_FILE}"
        )));
    }
    validate_scorer_command(&spec.scorer.command)?;
    require_safe_relative_path(&spec.scorer.result_file, "scorer.result_file")?;
    validate_benchmark_targets(spec)?;
    validate_execution(spec)?;

    require_safe_relative_path(&spec.datasets.public_dir, "datasets.public_dir")?;
    if spec.datasets.private_benchmark_policy != PrivateBenchmarkPolicy::ScoreOnly {
        return Err(AppError::Validation(
            "datasets.private_benchmark_policy must be score_only".to_string(),
        ));
    }

    // Challenge authors may stage private benchmark data before enabling
    // official runs. Static official run manifests need a private directory,
    // while prepare-generated official runs may only need private seeds.
    match (
        spec.datasets.private_benchmark_enabled,
        spec.datasets.private_benchmark_dir.as_deref(),
        spec.execution.official_runs.is_some(),
    ) {
        (true, Some(path), _) => {
            require_safe_relative_path(path, "datasets.private_benchmark_dir")?
        }
        (true, None, true) => {
            return Err(AppError::Validation(
                "datasets.private_benchmark_dir is required when private_benchmark_enabled uses static official_runs"
                    .to_string(),
            ));
        }
        (true, None, false) => {}
        (false, Some(path), _) => {
            require_safe_relative_path(path, "datasets.private_benchmark_dir")?
        }
        (false, None, _) => {}
    }

    validate_metric_schema(spec)?;
    validate_community(spec)?;

    Ok(())
}

/// Require immutable Docker image references for hosted or audited execution.
pub fn validate_digest_pinned_images(spec: &ChallengeBundleSpec) -> Result<()> {
    for (index, target) in spec.benchmark_targets.iter().enumerate() {
        let field = format!("benchmark_targets[{index}].resource_profile");
        require_image_digest_reference(
            &target.resource_profile.solution_image,
            &format!("{field}.solution_image"),
        )?;
        require_image_digest_reference(
            &target.resource_profile.scorer_image,
            &format!("{field}.scorer_image"),
        )?;
    }

    Ok(())
}

fn validate_scorer_command(command: &[String]) -> Result<()> {
    if command.is_empty() {
        return Err(AppError::Validation(
            "scorer.command must not be empty".to_string(),
        ));
    }
    for (index, part) in command.iter().enumerate() {
        require_non_empty(part, &format!("scorer.command[{index}]"))?;
        if part.contains('\0') {
            return Err(AppError::Validation(format!(
                "scorer.command[{index}] must not contain NUL bytes"
            )));
        }
    }

    Ok(())
}

fn validate_prepare_command(command: &[String], field: &str) -> Result<()> {
    if command.is_empty() {
        return Err(AppError::Validation(format!("{field} must not be empty")));
    }
    for (index, part) in command.iter().enumerate() {
        require_non_empty(part, &format!("{field}[{index}]"))?;
        if part.contains('\0') {
            return Err(AppError::Validation(format!(
                "{field}[{index}] must not contain NUL bytes"
            )));
        }
    }

    Ok(())
}

fn declared_scorer_script(command: &[String]) -> Option<&str> {
    command
        .iter()
        .find(|part| is_safe_relative_path(part) && part.ends_with(".py"))
        .map(String::as_str)
}

fn validate_benchmark_targets(spec: &ChallengeBundleSpec) -> Result<()> {
    if spec.benchmark_targets.is_empty() {
        return Err(AppError::Validation(
            "benchmark_targets must not be empty".to_string(),
        ));
    }

    let mut target_ids = HashSet::with_capacity(spec.benchmark_targets.len());
    for (index, target) in spec.benchmark_targets.iter().enumerate() {
        let field = format!("benchmark_targets[{index}]");
        validate_benchmark_target(target, &field)?;
        if !target_ids.insert(target.id.as_str()) {
            return Err(AppError::Validation(format!(
                "benchmark_targets contains duplicate id `{}`",
                target.id
            )));
        }
    }

    Ok(())
}

fn validate_benchmark_target(target: &BenchmarkTargetSpec, field: &str) -> Result<()> {
    require_non_empty(&target.id, &format!("{field}.id"))?;
    if target.accelerator != BenchmarkAccelerator::Cpu {
        return Err(AppError::Validation(format!(
            "{field}.accelerator must be cpu until GPU scheduling is implemented"
        )));
    }

    let expected_id = match target.docker_platform {
        DockerPlatform::LinuxArm64 => "cpu-linux-arm64",
        DockerPlatform::LinuxAmd64 => "cpu-linux-amd64",
    };
    if target.id != expected_id {
        return Err(AppError::Validation(format!(
            "{field}.id must be `{expected_id}` for docker_platform `{}`",
            target.docker_platform.as_str()
        )));
    }

    validate_resource_profile(
        &target.resource_profile,
        &format!("{field}.resource_profile"),
    )
}

fn validate_resource_profile(profile: &ResourceProfileSpec, field: &str) -> Result<()> {
    require_non_empty(&profile.id, &format!("{field}.id"))?;
    require_non_empty(&profile.solution_image, &format!("{field}.solution_image"))?;
    require_non_empty(&profile.scorer_image, &format!("{field}.scorer_image"))?;
    let solution_reference_digest = validate_image_reference_digest(
        &profile.solution_image,
        &format!("{field}.solution_image"),
    )?;
    let scorer_reference_digest =
        validate_image_reference_digest(&profile.scorer_image, &format!("{field}.scorer_image"))?;
    validate_image_digest(
        solution_reference_digest,
        profile.solution_image_digest.as_deref(),
        &format!("{field}.solution_image_digest"),
    )?;
    validate_image_digest(
        scorer_reference_digest,
        profile.scorer_image_digest.as_deref(),
        &format!("{field}.scorer_image_digest"),
    )?;
    validate_positive_u64(profile.timeout_sec, &format!("{field}.timeout_sec"))?;
    validate_positive_u64(profile.memory_limit_mb, &format!("{field}.memory_limit_mb"))?;
    validate_positive_u32(
        profile.cpu_limit_millis,
        &format!("{field}.cpu_limit_millis"),
    )?;
    validate_positive_u64(profile.disk_limit_mb, &format!("{field}.disk_limit_mb"))?;
    if let Some(resource_description) = &profile.resource_description {
        require_non_empty(
            resource_description,
            &format!("{field}.resource_description"),
        )?;
    }
    if let Some(hardware) = &profile.hardware {
        require_non_empty(&hardware.kind, &format!("{field}.hardware.kind"))?;
    }

    Ok(())
}

fn require_image_digest_reference(image: &str, field: &str) -> Result<()> {
    if validate_image_reference_digest(image, field)?.is_none() {
        return Err(AppError::Validation(format!(
            "{field} must include an immutable @sha256:<digest> reference"
        )));
    }

    Ok(())
}

fn validate_image_reference_digest<'a>(image: &'a str, field: &str) -> Result<Option<&'a str>> {
    let Some((_, digest)) = image.rsplit_once('@') else {
        return Ok(None);
    };
    validate_sha256_digest(digest, &format!("{field} digest"))?;
    Ok(Some(digest))
}

fn validate_image_digest(
    image_reference_digest: Option<&str>,
    digest: Option<&str>,
    field: &str,
) -> Result<()> {
    let Some(digest) = digest else {
        return Ok(());
    };
    require_non_empty(digest, field)?;
    validate_sha256_digest(digest, field)?;
    if image_reference_digest != Some(digest) {
        return Err(AppError::Validation(format!(
            "{field} must match the digest pinned in the image reference"
        )));
    }

    Ok(())
}

fn validate_sha256_digest(digest: &str, field: &str) -> Result<()> {
    const PREFIX: &str = "sha256:";
    if !digest.starts_with(PREFIX) {
        return Err(AppError::Validation(format!(
            "{field} must start with sha256:"
        )));
    }
    let hex = &digest[PREFIX.len()..];
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::Validation(format!(
            "{field} must be sha256: followed by 64 hexadecimal characters"
        )));
    }

    Ok(())
}

fn validate_execution(spec: &ChallengeBundleSpec) -> Result<()> {
    if let Some(path) = &spec.execution.validation_runs {
        require_safe_relative_path(path, "execution.validation_runs")?;
    }
    if let Some(prepare) = &spec.execution.validation_prepare {
        validate_prepare_spec(prepare, "execution.validation_prepare")?;
    }
    if let Some(path) = &spec.execution.official_runs {
        require_safe_relative_path(path, "execution.official_runs")?;
    }
    if let Some(prepare) = &spec.execution.official_prepare {
        validate_prepare_spec(prepare, "execution.official_prepare")?;
    }
    if spec.execution.validation_runs.is_some() && spec.execution.validation_prepare.is_some() {
        return Err(AppError::Validation(
            "execution must not declare both validation_runs and validation_prepare".to_string(),
        ));
    }
    if spec.execution.official_runs.is_some() && spec.execution.official_prepare.is_some() {
        return Err(AppError::Validation(
            "execution must not declare both official_runs and official_prepare".to_string(),
        ));
    }
    if spec
        .benchmark_targets
        .iter()
        .any(|target| target.validation_enabled)
        && spec.execution.validation_runs.is_none()
        && spec.execution.validation_prepare.is_none()
    {
        return Err(AppError::Validation(
            "execution.validation_runs or execution.validation_prepare is required when any benchmark target has validation_enabled true"
                .to_string(),
        ));
    }
    if spec.datasets.private_benchmark_enabled
        && spec.execution.official_runs.is_none()
        && spec.execution.official_prepare.is_none()
    {
        return Err(AppError::Validation(
            "execution.official_runs or execution.official_prepare is required when private_benchmark_enabled is true"
                .to_string(),
        ));
    }

    Ok(())
}

fn validate_prepare_spec(prepare: &ChallengePrepareSpec, field: &str) -> Result<()> {
    validate_prepare_command(&prepare.command, &format!("{field}.command"))?;
    require_safe_relative_path(
        &prepare.result_runs_file,
        &format!("{field}.result_runs_file"),
    )?;
    if let Some(notes) = &prepare.reproducibility_notes {
        require_non_empty(notes, &format!("{field}.reproducibility_notes"))?;
    }
    for (index, data) in prepare.external_data.iter().enumerate() {
        let data_field = format!("{field}.external_data[{index}]");
        require_non_empty(&data.url, &format!("{data_field}.url"))?;
        if data
            .url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(AppError::Validation(format!(
                "{data_field}.url must not contain whitespace or control characters"
            )));
        }
        if let Some(digest) = &data.digest {
            require_non_empty(digest, &format!("{data_field}.digest"))?;
        }
        if let Some(version) = &data.version {
            require_non_empty(version, &format!("{data_field}.version"))?;
        }
    }
    if let Some(cache_key_hint) = &prepare.cache_key_hint {
        require_non_empty(cache_key_hint, &format!("{field}.cache_key_hint"))?;
    }

    Ok(())
}

fn validate_challenge_run_manifest(manifest: &ChallengeRunManifest) -> Result<()> {
    if manifest.runs.is_empty() {
        return Err(AppError::Validation(
            "run manifest must declare at least one run".to_string(),
        ));
    }

    let mut run_ids = HashSet::with_capacity(manifest.runs.len());
    for run in &manifest.runs {
        validate_challenge_run(run)?;
        if !run_ids.insert(run.run_id.as_str()) {
            return Err(AppError::Validation(format!(
                "run manifest contains duplicate run_id `{}`",
                run.run_id
            )));
        }
    }

    Ok(())
}

fn validate_challenge_run(run: &ChallengeRunSpec) -> Result<()> {
    require_metric_id(&run.run_id, "runs[].run_id")?;
    if run.stdin_json.is_some() && run.stdin_text.is_some() {
        return Err(AppError::Validation(
            "runs[].stdin_json and runs[].stdin_text cannot both be present".to_string(),
        ));
    }
    for input in &run.input_files {
        validate_run_input_file(input)?;
    }
    let mut output_paths = HashSet::with_capacity(run.output_files.len());
    for path in &run.output_files {
        require_safe_relative_path(path, "runs[].output_files[]")?;
        if !output_paths.insert(path.as_str()) {
            return Err(AppError::Validation(format!(
                "runs[].output_files contains duplicate path `{path}`"
            )));
        }
    }

    Ok(())
}

fn validate_run_input_file(input: &ChallengeRunInputFile) -> Result<()> {
    require_safe_relative_path(&input.path, "runs[].input_files[].path")?;
    if let Some(source_path) = &input.source_path {
        require_safe_relative_path(source_path, "runs[].input_files[].source_path")?;
    }
    let source_count = [
        input.source_path.is_some(),
        input.content.is_some(),
        input.content_json.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if source_count > 1 {
        return Err(AppError::Validation(
            "runs[].input_files[] must declare only one of source_path, content, or content_json"
                .to_string(),
        ));
    }
    if source_count == 0 {
        return Err(AppError::Validation(
            "runs[].input_files[] must declare source_path, content, or content_json".to_string(),
        ));
    }

    Ok(())
}

/// Validate that source-backed run inputs exist under the bundle root.
pub async fn validate_challenge_run_manifest_sources(
    bundle_dir: &Path,
    manifest: &ChallengeRunManifest,
) -> Result<()> {
    for run in &manifest.runs {
        for input in &run.input_files {
            if let Some(source_path) = &input.source_path {
                let full_path = bundle_dir.join(source_path);
                let meta = tokio::fs::symlink_metadata(&full_path).await.map_err(|_| {
                    AppError::Validation(format!(
                        "runs[].input_files[].source_path does not exist: {}",
                        full_path.display()
                    ))
                })?;
                if meta.file_type().is_symlink() {
                    return Err(AppError::Validation(format!(
                        "runs[].input_files[].source_path must not be a symlink: {}",
                        full_path.display()
                    )));
                }
                if !meta.is_file() {
                    return Err(AppError::Validation(format!(
                        "runs[].input_files[].source_path is not a file: {}",
                        full_path.display()
                    )));
                }
            }
        }
    }

    Ok(())
}

fn validate_community(spec: &ChallengeBundleSpec) -> Result<()> {
    let Some(community) = &spec.community else {
        return Ok(());
    };

    let has_name = community
        .moltbook_submolt_name
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let has_url = community
        .moltbook_submolt_url
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    if !has_name && !has_url {
        return Err(AppError::Validation(
            "community must declare moltbook_submolt_name or moltbook_submolt_url".to_string(),
        ));
    }

    if let Some(name) = &community.moltbook_submolt_name {
        validate_moltbook_submolt_name(name)?;
    }
    if let Some(url) = &community.moltbook_submolt_url {
        validate_moltbook_submolt_url(url)?;
    }

    Ok(())
}

fn validate_moltbook_submolt_name(value: &str) -> Result<()> {
    require_non_empty(value, "community.moltbook_submolt_name")?;
    if value.chars().count() > 80 {
        return Err(AppError::Validation(
            "community.moltbook_submolt_name must be at most 80 characters".to_string(),
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(AppError::Validation(
            "community.moltbook_submolt_name must contain only ASCII letters, digits, underscores, hyphens, or dots"
                .to_string(),
        ));
    }

    Ok(())
}

fn validate_moltbook_submolt_url(value: &str) -> Result<()> {
    require_non_empty(value, "community.moltbook_submolt_url")?;
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::Validation(
            "community.moltbook_submolt_url must not contain whitespace or control characters"
                .to_string(),
        ));
    }
    if !value.starts_with("https://www.moltbook.com/") {
        return Err(AppError::Validation(
            "community.moltbook_submolt_url must start with https://www.moltbook.com/".to_string(),
        ));
    }

    Ok(())
}

fn validate_metric_schema(spec: &ChallengeBundleSpec) -> Result<()> {
    let schema = &spec.metric_schema;
    if schema.metrics.is_empty() {
        return Err(AppError::Validation(
            "metric_schema.metrics must not be empty".to_string(),
        ));
    }

    let mut ids = HashSet::with_capacity(schema.metrics.len());
    for metric in &schema.metrics {
        require_metric_id(&metric.id, "metric_schema.metrics[].id")?;
        require_non_empty(&metric.label, "metric_schema.metrics[].label")?;
        if let Some(unit) = &metric.unit {
            require_non_empty(unit, "metric_schema.metrics[].unit")?;
        }
        if let Some(metric_description) = &metric.metric_description {
            require_non_empty(
                metric_description,
                "metric_schema.metrics[].metric_description",
            )?;
        }
        if !ids.insert(metric.id.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.metrics contains duplicate id `{}`",
                metric.id
            )));
        }
    }

    require_metric_id(
        &schema.ranking.primary_metric_id,
        "metric_schema.ranking.primary_metric_id",
    )?;
    if !ids.contains(schema.ranking.primary_metric_id.as_str()) {
        return Err(AppError::Validation(format!(
            "metric_schema.ranking.primary_metric_id references unknown metric `{}`",
            schema.ranking.primary_metric_id
        )));
    }

    let mut tie_breakers = HashSet::with_capacity(schema.ranking.tie_breaker_metric_ids.len());
    for metric_id in &schema.ranking.tie_breaker_metric_ids {
        require_metric_id(metric_id, "metric_schema.ranking.tie_breaker_metric_ids[]")?;
        if metric_id == &schema.ranking.primary_metric_id {
            return Err(AppError::Validation(
                "metric_schema.ranking.tie_breaker_metric_ids must not repeat the primary metric"
                    .to_string(),
            ));
        }
        if !ids.contains(metric_id.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.ranking.tie_breaker_metric_ids references unknown metric `{metric_id}`"
            )));
        }
        if !tie_breakers.insert(metric_id.as_str()) {
            return Err(AppError::Validation(format!(
                "metric_schema.ranking.tie_breaker_metric_ids contains duplicate metric `{metric_id}`"
            )));
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

fn validate_positive_u64(value: u64, field: &str) -> Result<()> {
    if value == 0 {
        return Err(AppError::Validation(format!(
            "{field} must be greater than 0"
        )));
    }

    Ok(())
}

fn validate_positive_u32(value: u32, field: &str) -> Result<()> {
    if value == 0 {
        return Err(AppError::Validation(format!(
            "{field} must be greater than 0"
        )));
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

fn require_metric_id(value: &str, field: &str) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::models::challenge::{
        BenchmarkAccelerator, BenchmarkTargetSpec, ChallengeBundleSpec, ChallengeExecutionSpec,
        ChallengePrepareSpec, CommunitySpec, DatasetsSpec, DockerPlatform, MetricDirection,
        MetricSchemaSpec, MetricVisibility, PrivateBenchmarkPolicy, ResourceProfileSpec,
        ScorerSpec, SolutionSpec,
    };
    use crate::models::evaluation::ScoreVisibility;
    use crate::zip_project::ZipProjectNetworkAccess;

    use super::{
        validate_challenge_bundle, validate_challenge_bundle_spec, validate_digest_pinned_images,
    };

    fn test_digest() -> String {
        format!("sha256:{}", "a".repeat(64))
    }

    fn base_spec() -> ChallengeBundleSpec {
        ChallengeBundleSpec {
            schema_version: 1,
            challenge_id: "sample-sum".to_string(),
            challenge_title: "Sample Sum".to_string(),
            challenge_summary: "Add numbers from worker-managed runs.".to_string(),
            challenge_version: "v1".to_string(),
            solution: SolutionSpec {
                protocol: "zip_project".to_string(),
                manifest_file: "agentics.solution.json".to_string(),
            },
            scorer: ScorerSpec {
                command: vec!["python".to_string(), "scorer/run.py".to_string()],
                result_file: "result.json".to_string(),
            },
            benchmark_targets: vec![BenchmarkTargetSpec {
                id: "cpu-linux-arm64".to_string(),
                docker_platform: DockerPlatform::LinuxArm64,
                accelerator: BenchmarkAccelerator::Cpu,
                validation_enabled: true,
                resource_profile: ResourceProfileSpec {
                    id: "python-cpu-small".to_string(),
                    resource_description: None,
                    solution_image: "python:3.12-slim-bookworm".to_string(),
                    solution_image_digest: None,
                    scorer_image: "python:3.12-slim-bookworm".to_string(),
                    scorer_image_digest: None,
                    timeout_sec: 30,
                    memory_limit_mb: 512,
                    cpu_limit_millis: 1000,
                    disk_limit_mb: 1024,
                    setup_network_access: ZipProjectNetworkAccess::Enabled,
                    build_network_access: ZipProjectNetworkAccess::Disabled,
                    run_network_access: ZipProjectNetworkAccess::Disabled,
                    scorer_network_access: ZipProjectNetworkAccess::Disabled,
                    hardware: None,
                },
            }],
            execution: ChallengeExecutionSpec {
                validation_runs: Some("public/runs.json".to_string()),
                validation_prepare: None,
                official_runs: Some("private-benchmark/runs.json".to_string()),
                official_prepare: None,
            },
            datasets: DatasetsSpec {
                public_dir: "public".to_string(),
                private_benchmark_dir: Some("private-benchmark".to_string()),
                public_policy: ScoreVisibility::Full,
                private_benchmark_policy: PrivateBenchmarkPolicy::ScoreOnly,
                private_benchmark_enabled: true,
            },
            community: None,
            metric_schema: MetricSchemaSpec::default(),
        }
    }

    fn pin_images(spec: &mut ChallengeBundleSpec) {
        let digest = test_digest();
        for target in &mut spec.benchmark_targets {
            target.resource_profile.solution_image = format!("python:3.12-slim-bookworm@{digest}");
            target.resource_profile.solution_image_digest = Some(digest.clone());
            target.resource_profile.scorer_image = format!("python:3.12-slim-bookworm@{digest}");
            target.resource_profile.scorer_image_digest = Some(digest.clone());
        }
    }

    #[test]
    fn benchmark_targets_are_required() {
        let mut spec = base_spec();
        spec.benchmark_targets.clear();

        let error = validate_challenge_bundle_spec(&spec).expect_err("empty targets should fail");
        assert!(error.to_string().contains("benchmark_targets"));
    }

    #[test]
    fn target_id_must_match_docker_platform() {
        let mut spec = base_spec();
        spec.benchmark_targets[0].id = "cpu-linux-amd64".to_string();

        let error =
            validate_challenge_bundle_spec(&spec).expect_err("mismatched target should fail");
        assert!(error.to_string().contains("docker_platform"));
    }

    #[test]
    fn digest_pinned_image_policy_rejects_tag_only_images() {
        let spec = base_spec();

        let error =
            validate_digest_pinned_images(&spec).expect_err("tag-only images should fail policy");

        assert!(error.to_string().contains("@sha256:<digest>"));
    }

    #[test]
    fn digest_pinned_image_policy_accepts_immutable_references() {
        let mut spec = base_spec();
        pin_images(&mut spec);

        validate_challenge_bundle_spec(&spec).expect("pinned spec should validate");
        validate_digest_pinned_images(&spec).expect("pinned images should satisfy policy");
    }

    #[test]
    fn image_digest_field_must_match_image_reference() {
        let mut spec = base_spec();
        pin_images(&mut spec);
        spec.benchmark_targets[0]
            .resource_profile
            .solution_image_digest = Some(format!("sha256:{}", "b".repeat(64)));

        let error =
            validate_challenge_bundle_spec(&spec).expect_err("mismatched digest should fail");

        assert!(error.to_string().contains("must match"));
    }

    #[test]
    fn challenge_summary_is_required() {
        let mut spec = base_spec();
        spec.challenge_summary.clear();

        let error = validate_challenge_bundle_spec(&spec).expect_err("empty summary should fail");
        assert!(error.to_string().contains("challenge_summary"));
    }

    #[test]
    fn disabled_private_benchmark_may_still_declare_directory() {
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = false;
        spec.datasets.private_benchmark_dir = Some("private-benchmark".to_string());

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn enabled_private_benchmark_requires_directory() {
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = true;
        spec.datasets.private_benchmark_dir = None;

        assert!(validate_challenge_bundle_spec(&spec).is_err());
    }

    #[test]
    fn validation_run_manifest_required_only_when_target_enables_validation() {
        let mut spec = base_spec();
        spec.execution.validation_runs = None;
        spec.benchmark_targets[0].validation_enabled = false;

        assert!(validate_challenge_bundle_spec(&spec).is_ok());

        spec.benchmark_targets[0].validation_enabled = true;
        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("target validation should require run manifest");
        assert!(error.to_string().contains("execution.validation_runs"));
    }

    #[test]
    fn validation_prepare_satisfies_validation_enabled_target() {
        let mut spec = base_spec();
        spec.execution.validation_runs = None;
        spec.execution.validation_prepare = Some(prepare_spec());

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn official_prepare_satisfies_private_benchmark_execution() {
        let mut spec = base_spec();
        spec.execution.official_runs = None;
        spec.execution.official_prepare = Some(prepare_spec());

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn official_prepare_may_omit_private_benchmark_directory() {
        let mut spec = base_spec();
        spec.execution.official_runs = None;
        spec.execution.official_prepare = Some(prepare_spec());
        spec.datasets.private_benchmark_dir = None;

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn prepare_and_static_runs_are_mutually_exclusive_per_mode() {
        let mut spec = base_spec();
        spec.execution.official_prepare = Some(prepare_spec());

        let error = validate_challenge_bundle_spec(&spec)
            .expect_err("official prepare and official runs should conflict");
        assert!(error.to_string().contains("official_runs"));
    }

    #[test]
    fn metric_schema_rejects_unknown_primary_metric() {
        let mut spec = base_spec();
        spec.metric_schema.ranking.primary_metric_id = "missing".to_string();

        assert!(validate_challenge_bundle_spec(&spec).is_err());
    }

    #[test]
    fn metric_schema_rejects_duplicate_metric_ids() {
        let mut spec = base_spec();
        let mut duplicate = spec.metric_schema.metrics[0].clone();
        duplicate.label = "Duplicate Score".to_string();
        spec.metric_schema.metrics.push(duplicate);

        assert!(validate_challenge_bundle_spec(&spec).is_err());
    }

    #[test]
    fn metric_schema_accepts_tie_breaker_metadata() {
        let mut spec = base_spec();
        spec.metric_schema
            .metrics
            .push(crate::models::challenge::MetricDefinitionSpec {
                id: "runtime_ms".to_string(),
                label: "Runtime".to_string(),
                unit: Some("ms".to_string()),
                direction: MetricDirection::Minimize,
                visibility: MetricVisibility::Public,
                metric_description: Some("Wall-clock runtime in milliseconds.".to_string()),
            });
        spec.metric_schema
            .ranking
            .tie_breaker_metric_ids
            .push("runtime_ms".to_string());

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn community_accepts_moltbook_submolt_metadata() {
        let mut spec = base_spec();
        spec.community = Some(CommunitySpec {
            moltbook_submolt_name: Some("agentics-sample-sum".to_string()),
            moltbook_submolt_url: Some(
                "https://www.moltbook.com/submolts/agentics-sample-sum".to_string(),
            ),
        });

        assert!(validate_challenge_bundle_spec(&spec).is_ok());
    }

    #[test]
    fn community_rejects_non_moltbook_url() {
        let mut spec = base_spec();
        spec.community = Some(CommunitySpec {
            moltbook_submolt_name: Some("agentics-sample-sum".to_string()),
            moltbook_submolt_url: Some("https://example.com/agentics-sample-sum".to_string()),
        });

        let error = validate_challenge_bundle_spec(&spec).expect_err("invalid URL should fail");
        assert!(error.to_string().contains("moltbook_submolt_url"));
    }

    #[test]
    fn community_rejects_invalid_submolt_name() {
        let mut spec = base_spec();
        spec.community = Some(CommunitySpec {
            moltbook_submolt_name: Some("agentics sample sum".to_string()),
            moltbook_submolt_url: None,
        });

        let error = validate_challenge_bundle_spec(&spec).expect_err("invalid name should fail");
        assert!(error.to_string().contains("moltbook_submolt_name"));
    }

    fn create_bundle(root: &Path, spec: &ChallengeBundleSpec) {
        std::fs::create_dir_all(root.join("scorer")).expect("failed to create scorer dir");
        std::fs::create_dir_all(root.join("public")).expect("failed to create public dir");
        std::fs::write(
            root.join("public/runs.json"),
            r#"{"runs":[{"run_id":"public-1","interface":"stdio","stdin_text":"1"}]}"#,
        )
        .expect("failed to write public runs");
        std::fs::write(
            root.join("spec.json"),
            serde_json::to_string(spec).expect("failed to serialize spec"),
        )
        .expect("failed to write spec");
        std::fs::write(root.join("statement.md"), "# Sample\n\nBody\n")
            .expect("failed to write statement");
        std::fs::write(root.join("scorer/run.py"), "print('ok')\n")
            .expect("failed to write scorer");
    }

    fn prepare_spec() -> ChallengePrepareSpec {
        ChallengePrepareSpec {
            command: vec!["python".to_string(), "scorer/prepare.py".to_string()],
            result_runs_file: "generated/runs.json".to_string(),
            network_access: ZipProjectNetworkAccess::Disabled,
            reproducibility_notes: Some("Generated from deterministic private seeds.".to_string()),
            external_data: Vec::new(),
            cache_key_hint: None,
        }
    }

    #[tokio::test]
    async fn disabled_private_benchmark_bundle_does_not_require_directory() {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-disabled-private-benchmark-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = false;
        spec.datasets.private_benchmark_dir = Some("private-benchmark".to_string());
        create_bundle(&root, &spec);

        let result = validate_challenge_bundle(&root).await;
        let _ = std::fs::remove_dir_all(root);

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn source_backed_run_inputs_must_exist_under_bundle_root() {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-source-input-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = false;
        create_bundle(&root, &spec);
        std::fs::write(
            root.join("public/runs.json"),
            r#"{"runs":[{"run_id":"public-1","interface":"file_system","input_files":[{"path":"input.txt","source_path":"public/input.txt"}],"output_files":["answer.txt"]}]}"#,
        )
        .expect("failed to write source-backed runs");

        let missing_result = validate_challenge_bundle(&root).await;
        std::fs::write(root.join("public/input.txt"), "payload\n")
            .expect("failed to write source input");
        let present_result = validate_challenge_bundle(&root).await;
        let _ = std::fs::remove_dir_all(root);

        assert!(missing_result.is_err());
        assert!(present_result.is_ok());
    }

    #[tokio::test]
    async fn enabled_private_benchmark_bundle_requires_directory() {
        let root = std::env::temp_dir().join(format!(
            "agentics-bundle-enabled-private-benchmark-{}",
            uuid::Uuid::new_v4()
        ));
        let mut spec = base_spec();
        spec.datasets.private_benchmark_enabled = true;
        spec.datasets.private_benchmark_dir = Some("private-benchmark".to_string());
        create_bundle(&root, &spec);

        let result = validate_challenge_bundle(&root).await;
        let _ = std::fs::remove_dir_all(root);

        assert!(result.is_err());
    }
}
