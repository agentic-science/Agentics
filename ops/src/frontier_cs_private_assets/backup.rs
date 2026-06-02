use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, anyhow};
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::primitives::ByteStream;
use secrecy::{ExposeSecret, SecretString};
use sha2::{Digest, Sha256};
use url::Url;

use super::generate::GeneratedArtifact;
use super::{
    BACKUP_CREDENTIAL_PROVIDER_NAME, DEFAULT_BACKUP_API_PORT, DEFAULT_BACKUP_ENDPOINT_HOST,
    DEFAULT_REGION, PRIVATE_ZIP_NAME,
};

pub(super) async fn upload_generated_artifact(
    backup: &BackupClient,
    artifact: &GeneratedArtifact,
    confirm_overwrite: bool,
) -> anyhow::Result<()> {
    let key = format!("{}/{}", artifact.challenge_name, PRIVATE_ZIP_NAME);
    let new_digest = sha256_hex(&artifact.zip_bytes);
    if let Some(existing) = backup.get_object(&key).await? {
        if sha256_hex(&existing) == new_digest {
            return Ok(());
        }
        if !confirm_overwrite {
            anyhow::bail!(
                "backup object `{key}` already exists with different bytes; rerun with --confirm-overwrite"
            );
        }
    }
    backup.put_object(&key, &artifact.zip_bytes).await?;
    let uploaded = backup.get_object(&key).await?;
    let Some(uploaded) = uploaded else {
        anyhow::bail!("uploaded backup object `{key}` was not readable");
    };
    if uploaded.len() != artifact.zip_bytes.len() || sha256_hex(&uploaded) != new_digest {
        anyhow::bail!("uploaded backup object `{key}` failed length/hash verification");
    }
    Ok(())
}

pub(super) struct BackupClient {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl BackupClient {
    async fn get_object(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let output = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;
        let output = match output {
            Ok(output) => output,
            Err(error) if s3_get_object_error_is_not_found(&error) => return Ok(None),
            Err(error) => return Err(anyhow!("failed to read backup object `{key}`: {error:?}")),
        };
        let bytes = output
            .body
            .collect()
            .await
            .map_err(|error| anyhow!("failed to collect backup object `{key}`: {error:?}"))?
            .into_bytes()
            .to_vec();
        Ok(Some(bytes))
    }

    async fn put_object(&self, key: &str, bytes: &[u8]) -> anyhow::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(bytes.to_vec()))
            .send()
            .await
            .map_err(|error| anyhow!("failed to upload backup object `{key}`: {error:?}"))?;
        Ok(())
    }
}

pub(super) struct BackupEndpoint {
    endpoint_url: Url,
    bucket: String,
    region: String,
    force_path_style: bool,
    access_key: SecretString,
    secret_key: SecretString,
}

impl BackupEndpoint {
    pub(super) fn from_env(
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
            bucket: required_env_value(env, "AGENTICS_RUSTFS_BACKUP_BUCKET")?,
            region: env_value(env, "AGENTICS_RUSTFS_BACKUP_REGION")
                .unwrap_or_else(|| DEFAULT_REGION.to_string()),
            force_path_style: env_bool(env, "AGENTICS_RUSTFS_BACKUP_FORCE_PATH_STYLE")
                .unwrap_or(true),
            access_key: SecretString::from(required_env_value(
                env,
                "AGENTICS_RUSTFS_BACKUP_ACCESS_KEY",
            )?),
            secret_key: SecretString::from(required_env_value(
                env,
                "AGENTICS_RUSTFS_BACKUP_SECRET_KEY",
            )?),
        })
    }

    pub(super) async fn client(&self) -> anyhow::Result<BackupClient> {
        let credentials = Credentials::new(
            self.access_key.expose_secret(),
            self.secret_key.expose_secret(),
            None,
            None,
            BACKUP_CREDENTIAL_PROVIDER_NAME,
        );
        let shared_config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(self.region.clone()))
            .endpoint_url(self.endpoint_url.as_str())
            .credentials_provider(credentials)
            .load()
            .await;
        let mut config = aws_sdk_s3::config::Builder::from(&shared_config);
        if self.force_path_style {
            config = config.force_path_style(true);
        }
        Ok(BackupClient {
            client: aws_sdk_s3::Client::from_conf(config.build()),
            bucket: self.bucket.clone(),
        })
    }
}

fn s3_get_object_error_is_not_found(error: &SdkError<GetObjectError>) -> bool {
    if error
        .raw_response()
        .is_some_and(|response| response.status().as_u16() == 404)
    {
        return true;
    }
    matches!(error.code(), Some("NoSuchKey" | "NotFound" | "404"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn load_env_file(path: &Path) -> anyhow::Result<HashMap<String, String>> {
    let mut values = HashMap::new();
    if !path.try_exists()? {
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

fn env_value(env: &HashMap<String, String>, name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .or_else(|| env.get(name).cloned())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn required_env_value(env: &HashMap<String, String>, name: &str) -> anyhow::Result<String> {
    env_value(env, name).ok_or_else(|| anyhow!("{name} must be set"))
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
