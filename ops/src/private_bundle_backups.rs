//! Copy migrated-challenge private bundle backups between RustFS/S3 stores.
//!
//! The persistent backup RustFS is intentionally not the Agentics durable
//! storage backend. This command copies backup objects into a production
//! rehearsal bucket so operators can reuse or inspect migrated private bundle
//! ZIPs without relying on host-local scratch files. The copy is idempotent:
//! existing destination objects are downloaded and SHA-256 verified before they
//! are skipped. New uploads are verified by downloading the destination object
//! after the put completes.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use agentics_domain::models::names::ChallengeName;
use agentics_domain::storage::StorageKey;
use anyhow::{Context, anyhow};
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use clap::Parser;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

use crate::support::{ReportLine, print_reports, run_with_ctrl_c};

const PREFIX: &str = "agentics-copy-private-bundle-backups";
const DEFAULT_BACKUP_ENV_FILE: &str = "deploy/compose/env/rustfs-private-backup.env";
const DEFAULT_PROD_ENV_FILE: &str = "deploy/compose/env/prod.env";
const DEFAULT_REGION: &str = "us-east-1";
const DEFAULT_BACKUP_ENDPOINT_HOST: &str = "127.0.0.1";
const DEFAULT_BACKUP_API_PORT: u16 = 9100;
const DEFAULT_DESTINATION_PREFIX: &str = "private-bundle-backups";
const DEFAULT_MAX_OBJECT_BYTES: u64 = 1024 * 1024 * 1024;
const COPY_CREDENTIAL_PROVIDER_NAME: &str = "agentics-private-bundle-backup-copy";

/// CLI for copying private bundle backups into production object storage.
#[derive(Debug, Parser)]
#[command(
    about = "Copies migrated challenge private bundle backups into production RustFS/S3.",
    long_about = "Copies objects from the persistent private-bundle backup RustFS bucket into the production object-storage bucket. The command never logs credentials, writes downloads through private temporary files, verifies SHA-256 before skipping existing objects, and verifies destination contents after upload."
)]
pub struct Cli {
    /// Backup RustFS env file.
    #[arg(long, default_value = DEFAULT_BACKUP_ENV_FILE)]
    backup_env_file: PathBuf,

    /// Production Compose env file.
    #[arg(long, default_value = DEFAULT_PROD_ENV_FILE)]
    prod_env_file: PathBuf,

    /// Source S3 endpoint URL. Defaults to AGENTICS_RUSTFS_BACKUP_ENDPOINT_URL or localhost backup port.
    #[arg(long)]
    source_endpoint_url: Option<Url>,

    /// Destination S3 endpoint URL. Defaults to AGENTICS_S3_ENDPOINT_URL from the production env.
    #[arg(long)]
    destination_endpoint_url: Option<Url>,

    /// Source object prefix to copy. Repeat for multiple prefixes.
    #[arg(long)]
    source_prefix: Vec<StorageKey>,

    /// Copy only the backup directory for one migrated challenge. Repeatable.
    #[arg(long)]
    challenge: Vec<ChallengeName>,

    /// Logical destination prefix under AGENTICS_S3_PREFIX.
    #[arg(long, default_value = DEFAULT_DESTINATION_PREFIX)]
    destination_prefix: String,

    /// Maximum accepted object size in bytes.
    #[arg(long, default_value_t = DEFAULT_MAX_OBJECT_BYTES)]
    max_object_bytes: u64,

    /// Replace destination objects that differ from the source.
    #[arg(long)]
    overwrite: bool,

    /// List and verify planned work without uploading.
    #[arg(long)]
    dry_run: bool,

    /// Directory for temporary object downloads.
    #[arg(long)]
    work_dir: Option<PathBuf>,
}

/// Run this command from process args and env.
pub async fn run_from_process() -> ExitCode {
    let cli = Cli::parse();
    run_with_ctrl_c(PREFIX, async move {
        match run(cli).await {
            Ok(reports) => print_reports(PREFIX, &reports),
            Err(error) => {
                eprintln!("[{PREFIX}] ERROR: {error:#}");
                ExitCode::from(2)
            }
        }
    })
    .await
}

async fn run(cli: Cli) -> anyhow::Result<Vec<ReportLine>> {
    if cli.max_object_bytes == 0 {
        anyhow::bail!("--max-object-bytes must be greater than zero");
    }
    let backup_env = load_env_file(&cli.backup_env_file)?;
    let prod_env = load_env_file(&cli.prod_env_file)?;
    let source = EndpointConfig::backup(&backup_env, cli.source_endpoint_url)?;
    let destination = EndpointConfig::production(&prod_env, cli.destination_endpoint_url)?;
    let source_client = source.client().await?;
    let destination_client = destination.client().await?;
    let destination_logical_prefix = normalized_optional_prefix(&cli.destination_prefix)?;
    let source_prefixes = source_prefixes(&cli.source_prefix, &cli.challenge);
    let work_dir = cli
        .work_dir
        .unwrap_or_else(|| std::env::temp_dir().join("agentics-private-bundle-backup-copy"));
    ensure_private_work_dir(&work_dir).await?;

    let source_keys = list_source_keys(&source_client, &source.bucket, &source_prefixes).await?;
    if source_keys.is_empty() {
        return Ok(vec![ReportLine::skip(
            "copy",
            "no source backup objects matched the requested prefixes",
        )]);
    }

    let mut stats = CopyStats::default();
    for source_key in source_keys {
        let source_storage_key = StorageKey::try_new(&source_key)
            .with_context(|| format!("source backup key `{source_key}` is unsafe"))?;
        let destination_key = destination_key(
            destination.root_prefix.as_deref(),
            destination_logical_prefix.as_deref(),
            source_storage_key.as_str(),
        )?;
        copy_one_object(
            &source_client,
            &destination_client,
            CopyObjectRequest {
                source_bucket: &source.bucket,
                source_key: source_storage_key.as_str(),
                destination_bucket: &destination.bucket,
                destination_key: destination_key.as_str(),
                work_dir: &work_dir,
                max_object_bytes: cli.max_object_bytes,
                overwrite: cli.overwrite,
                dry_run: cli.dry_run,
            },
            &mut stats,
        )
        .await?;
    }

    Ok(vec![ReportLine::pass(
        "copy",
        stats.summary(cli.dry_run, &destination.bucket),
    )])
}

fn load_env_file(path: &Path) -> anyhow::Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    if !path
        .try_exists()
        .with_context(|| format!("failed to inspect env file {}", path.display()))?
    {
        return Ok(values);
    }
    for item in dotenvy::from_path_iter(path)
        .with_context(|| format!("failed to read env file {}", path.display()))?
    {
        let (key, value) =
            item.with_context(|| format!("failed to parse env file {}", path.display()))?;
        values.insert(key, value);
    }
    Ok(values)
}

#[derive(Clone)]
struct EndpointConfig {
    endpoint_url: Url,
    bucket: String,
    root_prefix: Option<String>,
    region: String,
    force_path_style: bool,
    access_key: SecretString,
    secret_key: SecretString,
}

impl EndpointConfig {
    fn backup(
        env: &HashMap<String, String>,
        endpoint_override: Option<Url>,
    ) -> anyhow::Result<Self> {
        let endpoint_url = if let Some(endpoint_url) = endpoint_override
            .or_else(|| process_url("AGENTICS_RUSTFS_BACKUP_ENDPOINT_URL"))
            .or_else(|| env_url(env, "AGENTICS_RUSTFS_BACKUP_ENDPOINT_URL"))
        {
            endpoint_url
        } else {
            let port =
                env_u16(env, "AGENTICS_RUSTFS_BACKUP_API_PORT").unwrap_or(DEFAULT_BACKUP_API_PORT);
            Url::parse(&format!("http://{DEFAULT_BACKUP_ENDPOINT_HOST}:{port}"))
                .context("built-in backup endpoint must be valid")?
        };
        Ok(Self {
            endpoint_url,
            bucket: required_value(env, "AGENTICS_RUSTFS_BACKUP_BUCKET")?,
            root_prefix: None,
            region: env_value(env, "AGENTICS_RUSTFS_BACKUP_REGION")
                .unwrap_or_else(|| DEFAULT_REGION.to_string()),
            force_path_style: env_bool(env, "AGENTICS_RUSTFS_BACKUP_FORCE_PATH_STYLE")
                .unwrap_or(true),
            access_key: SecretString::from(required_value(
                env,
                "AGENTICS_RUSTFS_BACKUP_ACCESS_KEY",
            )?),
            secret_key: SecretString::from(required_value(
                env,
                "AGENTICS_RUSTFS_BACKUP_SECRET_KEY",
            )?),
        })
    }

    fn production(
        env: &HashMap<String, String>,
        endpoint_override: Option<Url>,
    ) -> anyhow::Result<Self> {
        let endpoint_url = endpoint_override
            .or_else(|| process_url("AGENTICS_S3_ENDPOINT_URL"))
            .or_else(|| env_url(env, "AGENTICS_S3_ENDPOINT_URL"))
            .ok_or_else(|| anyhow!("AGENTICS_S3_ENDPOINT_URL must be set"))?;
        let access_key =
            first_required_value(env, &["AWS_ACCESS_KEY_ID", "AGENTICS_RUSTFS_ACCESS_KEY"])?;
        let secret_key = first_required_value(
            env,
            &["AWS_SECRET_ACCESS_KEY", "AGENTICS_RUSTFS_SECRET_KEY"],
        )?;
        Ok(Self {
            endpoint_url,
            bucket: required_value(env, "AGENTICS_S3_BUCKET")?,
            root_prefix: normalized_optional_prefix(
                &env_value(env, "AGENTICS_S3_PREFIX").unwrap_or_default(),
            )?,
            region: env_value(env, "AGENTICS_S3_REGION")
                .unwrap_or_else(|| DEFAULT_REGION.to_string()),
            force_path_style: env_bool(env, "AGENTICS_S3_FORCE_PATH_STYLE").unwrap_or(true),
            access_key: SecretString::from(access_key),
            secret_key: SecretString::from(secret_key),
        })
    }

    async fn client(&self) -> anyhow::Result<aws_sdk_s3::Client> {
        let credentials = Credentials::new(
            self.access_key.expose_secret(),
            self.secret_key.expose_secret(),
            None,
            None,
            COPY_CREDENTIAL_PROVIDER_NAME,
        );
        let loader = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(self.region.clone()))
            .endpoint_url(self.endpoint_url.as_str())
            .credentials_provider(credentials);
        let shared_config = loader.load().await;
        let mut s3_config = aws_sdk_s3::config::Builder::from(&shared_config);
        if self.force_path_style {
            s3_config = s3_config.force_path_style(true);
        }
        Ok(aws_sdk_s3::Client::from_conf(s3_config.build()))
    }
}

fn env_value(env: &HashMap<String, String>, name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .or_else(|| env.get(name).cloned())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn process_url(name: &str) -> Option<Url> {
    std::env::var(name)
        .ok()
        .and_then(|value| Url::parse(value.trim()).ok())
}

fn env_url(env: &HashMap<String, String>, name: &str) -> Option<Url> {
    env_value(env, name).and_then(|value| Url::parse(&value).ok())
}

fn env_u16(env: &HashMap<String, String>, name: &str) -> Option<u16> {
    env_value(env, name).and_then(|value| value.parse::<u16>().ok())
}

fn env_bool(env: &HashMap<String, String>, name: &str) -> Option<bool> {
    env_value(env, name).and_then(|value| match value.as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    })
}

fn required_value(env: &HashMap<String, String>, name: &str) -> anyhow::Result<String> {
    env_value(env, name).ok_or_else(|| anyhow!("{name} must be set"))
}

fn first_required_value(env: &HashMap<String, String>, names: &[&str]) -> anyhow::Result<String> {
    for name in names {
        if let Some(value) = env_value(env, name) {
            return Ok(value);
        }
    }
    anyhow::bail!("one of {} must be set", names.join(", "))
}

fn normalized_optional_prefix(value: &str) -> anyhow::Result<Option<String>> {
    let value = value.trim().trim_matches('/');
    if value.is_empty() {
        return Ok(None);
    }
    StorageKey::try_new(value)
        .map(|key| Some(key.to_string()))
        .map_err(|error| anyhow!(error))
}

fn source_prefixes(prefixes: &[StorageKey], challenges: &[ChallengeName]) -> Vec<String> {
    let mut values = prefixes
        .iter()
        .map(|prefix| prefix.as_str().trim_end_matches('/').to_string())
        .collect::<Vec<_>>();
    values.extend(
        challenges
            .iter()
            .map(|challenge| format!("{}/", challenge.as_str())),
    );
    if values.is_empty() {
        values.push(String::new());
    }
    values.sort();
    values.dedup();
    values
}

async fn list_source_keys(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    prefixes: &[String],
) -> anyhow::Result<Vec<String>> {
    let mut keys = BTreeSet::new();
    for prefix in prefixes {
        let mut continuation_token = None;
        loop {
            let output = client
                .list_objects_v2()
                .bucket(bucket)
                .prefix(prefix)
                .set_continuation_token(continuation_token.clone())
                .send()
                .await
                .map_err(|error| anyhow!("failed to list backup prefix `{prefix}`: {error:?}"))?;
            for object in output.contents() {
                if let Some(key) = object.key()
                    && !key.ends_with('/')
                {
                    keys.insert(key.to_string());
                }
            }
            continuation_token = output.next_continuation_token().map(ToOwned::to_owned);
            if continuation_token.is_none() {
                break;
            }
        }
    }
    Ok(keys.into_iter().collect())
}

fn destination_key(
    root_prefix: Option<&str>,
    destination_prefix: Option<&str>,
    source_key: &str,
) -> anyhow::Result<StorageKey> {
    let mut parts = Vec::new();
    if let Some(prefix) = root_prefix {
        parts.push(prefix.trim_matches('/'));
    }
    if let Some(prefix) = destination_prefix {
        parts.push(prefix.trim_matches('/'));
    }
    parts.push(source_key.trim_matches('/'));
    StorageKey::try_new(parts.join("/")).map_err(|error| anyhow!(error))
}

struct CopyObjectRequest<'a> {
    source_bucket: &'a str,
    source_key: &'a str,
    destination_bucket: &'a str,
    destination_key: &'a str,
    work_dir: &'a Path,
    max_object_bytes: u64,
    overwrite: bool,
    dry_run: bool,
}

async fn copy_one_object(
    source_client: &aws_sdk_s3::Client,
    destination_client: &aws_sdk_s3::Client,
    request: CopyObjectRequest<'_>,
    stats: &mut CopyStats,
) -> anyhow::Result<()> {
    let temp_path = request
        .work_dir
        .join(format!("private-bundle-copy-{}", uuid::Uuid::new_v4()));
    let source_snapshot = download_object_to_file(
        source_client,
        request.source_bucket,
        request.source_key,
        request.max_object_bytes,
        &temp_path,
    )
    .await?;
    stats.objects_seen = stats
        .objects_seen
        .checked_add(1)
        .ok_or_else(|| anyhow!("private bundle copy object count overflow"))?;
    stats.bytes_seen = stats
        .bytes_seen
        .checked_add(source_snapshot.len)
        .ok_or_else(|| anyhow!("private bundle copy byte count overflow"))?;

    let destination_snapshot = object_snapshot(
        destination_client,
        request.destination_bucket,
        request.destination_key,
        request.max_object_bytes,
    )
    .await?;

    if let Some(existing) = destination_snapshot {
        if existing == source_snapshot {
            println!(
                "[{PREFIX}] SKIP {} -> {} already matches",
                request.source_key, request.destination_key
            );
            stats.objects_skipped = stats
                .objects_skipped
                .checked_add(1)
                .ok_or_else(|| anyhow!("private bundle copy skipped count overflow"))?;
            cleanup_temp_file(&temp_path).await?;
            return Ok(());
        }
        if !request.overwrite {
            cleanup_temp_file(&temp_path).await?;
            anyhow::bail!(
                "destination object `{}` already exists but differs from source; rerun with --overwrite to replace it",
                request.destination_key
            );
        }
    }

    if request.dry_run {
        println!(
            "[{PREFIX}] DRY-RUN {} -> {} ({} bytes)",
            request.source_key, request.destination_key, source_snapshot.len
        );
        stats.objects_copied = stats
            .objects_copied
            .checked_add(1)
            .ok_or_else(|| anyhow!("private bundle copy copied count overflow"))?;
        cleanup_temp_file(&temp_path).await?;
        return Ok(());
    }

    upload_file(
        destination_client,
        request.destination_bucket,
        request.destination_key,
        &temp_path,
        source_snapshot.len,
        !request.overwrite,
    )
    .await?;

    let verified = object_snapshot(
        destination_client,
        request.destination_bucket,
        request.destination_key,
        request.max_object_bytes,
    )
    .await?
    .ok_or_else(|| anyhow!("uploaded destination object disappeared"))?;
    if verified != source_snapshot {
        anyhow::bail!(
            "destination verification failed for `{}` after upload",
            request.destination_key
        );
    }
    println!(
        "[{PREFIX}] COPY {} -> {} ({} bytes)",
        request.source_key, request.destination_key, source_snapshot.len
    );
    stats.objects_copied = stats
        .objects_copied
        .checked_add(1)
        .ok_or_else(|| anyhow!("private bundle copy copied count overflow"))?;
    cleanup_temp_file(&temp_path).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObjectSnapshot {
    len: u64,
    sha256_hex: String,
}

async fn object_snapshot(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    max_object_bytes: u64,
) -> anyhow::Result<Option<ObjectSnapshot>> {
    let output = match client.get_object().bucket(bucket).key(key).send().await {
        Ok(output) => output,
        Err(error) if s3_error_is_not_found(&error) => return Ok(None),
        Err(error) => anyhow::bail!("failed to read S3 object `{key}`: {error:?}"),
    };
    let mut body = output.body.into_async_read();
    let mut hasher = Sha256::new();
    let mut len = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = body
            .read(&mut buffer)
            .await
            .with_context(|| format!("failed to read S3 object `{key}`"))?;
        if read == 0 {
            break;
        }
        let read_u64 = u64::try_from(read).context("S3 read chunk length overflow")?;
        len = len
            .checked_add(read_u64)
            .ok_or_else(|| anyhow!("S3 object `{key}` length overflow"))?;
        if len > max_object_bytes {
            anyhow::bail!("S3 object `{key}` exceeds --max-object-bytes");
        }
        hasher.update(
            buffer
                .get(..read)
                .ok_or_else(|| anyhow!("S3 read chunk range invalid"))?,
        );
    }
    Ok(Some(ObjectSnapshot {
        len,
        sha256_hex: hex::encode(hasher.finalize()),
    }))
}

async fn download_object_to_file(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    max_object_bytes: u64,
    destination: &Path,
) -> anyhow::Result<ObjectSnapshot> {
    let output = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .with_context(|| format!("failed to download backup object `{key}`"))?;
    let mut body = output.body.into_async_read();
    let mut file = create_private_file(destination).await?;
    let mut hasher = Sha256::new();
    let mut len = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = body
            .read(&mut buffer)
            .await
            .with_context(|| format!("failed to read backup object `{key}`"))?;
        if read == 0 {
            break;
        }
        let read_u64 = u64::try_from(read).context("S3 read chunk length overflow")?;
        len = len
            .checked_add(read_u64)
            .ok_or_else(|| anyhow!("S3 object `{key}` length overflow"))?;
        if len > max_object_bytes {
            anyhow::bail!("S3 object `{key}` exceeds --max-object-bytes");
        }
        let chunk = buffer
            .get(..read)
            .ok_or_else(|| anyhow!("S3 read chunk range invalid"))?;
        hasher.update(chunk);
        file.write_all(chunk)
            .await
            .with_context(|| format!("failed to write {}", destination.display()))?;
    }
    file.flush()
        .await
        .with_context(|| format!("failed to flush {}", destination.display()))?;
    Ok(ObjectSnapshot {
        len,
        sha256_hex: hex::encode(hasher.finalize()),
    })
}

async fn upload_file(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    source: &Path,
    len: u64,
    fail_if_exists: bool,
) -> anyhow::Result<()> {
    let body = ByteStream::from_path(source)
        .await
        .with_context(|| format!("failed to open {}", source.display()))?;
    let content_length = i64::try_from(len).context("object length exceeds S3 i64 range")?;
    let mut request = client
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_length(content_length)
        .body(body);
    if fail_if_exists {
        request = request.if_none_match("*");
    }
    request
        .send()
        .await
        .with_context(|| format!("failed to upload destination object `{key}`"))?;
    Ok(())
}

async fn ensure_private_work_dir(path: &Path) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(path)
        .await
        .with_context(|| format!("failed to create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .await
            .with_context(|| format!("failed to restrict {}", path.display()))?;
    }
    Ok(())
}

async fn create_private_file(path: &Path) -> anyhow::Result<tokio::fs::File> {
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    options
        .open(path)
        .await
        .with_context(|| format!("failed to create {}", path.display()))
}

async fn cleanup_temp_file(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn s3_error_is_not_found<E: std::fmt::Debug + std::fmt::Display>(error: &E) -> bool {
    let text = format!("{error} {error:?}");
    text.contains("NotFound")
        || text.contains("NoSuchKey")
        || text.contains("NoSuchBucket")
        || text.contains("404")
}

#[derive(Default)]
struct CopyStats {
    objects_seen: u64,
    objects_copied: u64,
    objects_skipped: u64,
    bytes_seen: u64,
}

impl CopyStats {
    fn summary(&self, dry_run: bool, destination_bucket: &str) -> String {
        let action = if dry_run { "would copy" } else { "copied" };
        format!(
            "{action} {} object(s), skipped {} verified existing object(s), scanned {} object(s) / {} byte(s) into bucket `{destination_bucket}`",
            self.objects_copied, self.objects_skipped, self.objects_seen, self.bytes_seen
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge(value: &str) -> ChallengeName {
        ChallengeName::try_new(value).expect("valid challenge name")
    }

    fn key(value: &str) -> StorageKey {
        StorageKey::try_new(value).expect("valid storage key")
    }

    #[test]
    fn source_prefixes_default_to_entire_bucket() {
        assert_eq!(source_prefixes(&[], &[]), vec![String::new()]);
    }

    #[test]
    fn source_prefixes_include_challenge_directories() {
        assert_eq!(
            source_prefixes(
                &[key("frontier-cs-migrations-20260525")],
                &[challenge("sample-sum")]
            ),
            vec![
                "frontier-cs-migrations-20260525".to_string(),
                "sample-sum/".to_string(),
            ]
        );
    }

    #[test]
    fn destination_key_uses_prod_and_backup_prefixes() {
        assert_eq!(
            destination_key(
                Some("prod"),
                Some("private-bundle-backups"),
                "sample-sum/official-runs.zip"
            )
            .expect("destination key")
            .as_str(),
            "prod/private-bundle-backups/sample-sum/official-runs.zip"
        );
    }
}
