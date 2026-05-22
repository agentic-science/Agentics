use std::collections::HashMap;

use bollard::Docker;
use bollard::container::LogOutput;
use bollard::models::{DeviceRequest, HostConfigLogConfig};
use bollard::query_parameters::LogsOptionsBuilder;
use futures::StreamExt;

use crate::error::{Result, ServiceError};
use crate::models::challenge::TargetAccelerator;

/// Handles docker log config for this module.
pub(super) fn docker_log_config(limit_bytes: u64) -> HostConfigLogConfig {
    let mut config = std::collections::HashMap::new();
    config.insert("max-size".to_string(), format!("{}b", limit_bytes.max(1)));
    config.insert("max-file".to_string(), "1".to_string());

    HostConfigLogConfig {
        typ: Some("json-file".to_string()),
        config: Some(config),
    }
}

/// Handles docker storage opt for this module.
pub(super) fn docker_storage_opt(limit_mb: Option<u64>) -> Option<HashMap<String, String>> {
    limit_mb.map(|limit_mb| {
        let mut storage_opt = HashMap::new();
        storage_opt.insert("size".to_string(), format!("{limit_mb}m"));
        storage_opt
    })
}

/// Handles accelerator device requests for this module.
pub(super) fn accelerator_device_requests(
    accelerator: TargetAccelerator,
    accelerator_count: Option<u32>,
) -> Result<Option<Vec<DeviceRequest>>> {
    match accelerator {
        TargetAccelerator::None => Ok(None),
        TargetAccelerator::Gpu => {
            let count = accelerator_count.ok_or_else(|| {
                ServiceError::Runner(
                    "accelerator `gpu` requires resource_profile.hardware_metadata.gpu_count"
                        .to_string(),
                )
            })?;
            let count = i64::from(count);
            Ok(Some(vec![DeviceRequest {
                driver: Some("nvidia".to_string()),
                count: Some(count),
                capabilities: Some(vec![vec!["gpu".to_string()]]),
                ..Default::default()
            }]))
        }
    }
}

/// Handles collect container logs for this module.
pub(super) async fn collect_container_logs(
    docker: &Docker,
    container_id: &str,
    limit_bytes: u64,
) -> Result<(String, bool)> {
    let opts = LogsOptionsBuilder::default()
        .stdout(true)
        .stderr(true)
        .tail("all")
        .build();
    let mut logs = docker.logs(container_id, Some(opts));
    let mut output = Vec::new();
    let mut truncated = false;
    let limit = usize::try_from(limit_bytes).unwrap_or(usize::MAX);

    while let Some(chunk) = logs.next().await {
        match chunk {
            Ok(LogOutput::StdOut { message })
            | Ok(LogOutput::StdErr { message })
            | Ok(LogOutput::Console { message }) => {
                append_bounded_log_bytes(&mut output, &message, limit, &mut truncated);
                if output.len() >= limit {
                    truncated = true;
                    break;
                }
            }
            Err(e) => {
                return Err(ServiceError::Docker(format!(
                    "collect container logs failed: {e}"
                )));
            }
            _ => {}
        }
    }

    let mut output = String::from_utf8_lossy(&output).into_owned();
    if truncated {
        output.push_str(&format!(
            "\n[agentics] container logs truncated at {limit_bytes} bytes\n"
        ));
    }

    Ok((output, truncated))
}

/// Handles append bounded log bytes for this module.
pub(super) fn append_bounded_log_bytes(
    output: &mut Vec<u8>,
    chunk: &[u8],
    limit: usize,
    truncated: &mut bool,
) {
    if output.len() >= limit {
        *truncated = !chunk.is_empty();
        return;
    }

    let remaining = limit.saturating_sub(output.len());
    if chunk.len() > remaining {
        output.extend(chunk.iter().take(remaining).copied());
        *truncated = true;
    } else {
        output.extend_from_slice(chunk);
    }
}
