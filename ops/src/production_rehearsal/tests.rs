use serde_json::json;

use super::{GpuMode, RehearsalStatus, heartbeat_check};

/// Verifies GPU-required rehearsal accepts a GPU heartbeat even when CPU worker appears first.
#[test]
fn heartbeat_check_scans_all_workers_for_gpu_capability() {
    let check = heartbeat_check(
        json!({
            "items": [
                {
                    "service_name": "worker-cpu",
                    "payload": { "accelerators": ["none"] }
                },
                {
                    "service_name": "worker-gpu",
                    "payload": { "accelerators": ["none", "gpu"] }
                }
            ]
        }),
        GpuMode::Require,
    );
    assert_eq!(check.status, RehearsalStatus::Passed);
}

/// Verifies GPU-required rehearsal fails when only CPU worker heartbeats are present.
#[test]
fn heartbeat_check_requires_gpu_when_requested() {
    let check = heartbeat_check(
        json!({
            "items": [
                {
                    "service_name": "worker-cpu",
                    "payload": { "accelerators": ["none"] }
                }
            ]
        }),
        GpuMode::Require,
    );
    assert_eq!(check.status, RehearsalStatus::Failed);
}

/// Verifies GPU-required rehearsal does not accept malformed string payloads.
#[test]
fn heartbeat_check_rejects_stringified_gpu_payload() {
    let check = heartbeat_check(
        json!({
            "items": [
                {
                    "service_name": "worker-gpu",
                    "payload": { "accelerators": "gpu" }
                }
            ]
        }),
        GpuMode::Require,
    );
    assert_eq!(check.status, RehearsalStatus::Failed);
}
