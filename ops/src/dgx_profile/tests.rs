use std::path::PathBuf;

use super::{
    InstallPlan, UninstallPlan, is_agentics_fstab_entry, remove_lines_matching_paths,
    validate_uninstall_roots,
};
use crate::dgx::DgxProfileConfig;

fn config() -> DgxProfileConfig {
    DgxProfileConfig {
        service_user: "agentics".to_string(),
        service_group: "agentics".to_string(),
        config_root: "/etc/agentics".into(),
        release_root: "/opt/agentics/current".into(),
        source_root: "/opt/agentics/current".into(),
        state_root: "/srv/agentics".into(),
        test_state_root: "/srv/agentics-test".into(),
        systemd_root: "/etc/systemd/system".into(),
        docker_host_uri: "unix:///run/agentics/docker.sock".to_string(),
    }
}

/// Verifies install can skip storage as a separate lifecycle decision.
#[test]
fn install_plan_can_skip_storage() {
    let plan = InstallPlan::from_config(&config(), true);
    assert!(
        !plan
            .actions
            .iter()
            .any(|action| action.describe().contains("quota storage"))
    );
}

/// Verifies purge adds identity and durable path removal to uninstall.
#[test]
fn purge_plan_removes_identity() {
    let plan = UninstallPlan::from_config(&config(), true);
    assert!(
        plan.actions
            .iter()
            .any(|action| action.describe().contains("service user"))
    );
}

/// Verifies purge refuses broad release/config overrides before deletion.
#[test]
fn purge_root_validation_rejects_broad_overrides() {
    let mut release_config = config();
    release_config.release_root = "/opt".into();
    assert!(validate_uninstall_roots(&release_config, true).is_err());

    let mut state_config = config();
    state_config.state_root = "/srv".into();
    assert!(validate_uninstall_roots(&state_config, false).is_err());
}

/// Verifies fstab cleanup targets only storage-prepared loop quota mounts.
#[test]
fn fstab_cleanup_matches_only_loop_quota_entries() {
    let root = std::path::PathBuf::from("/srv/agentics");
    let roots = [&root];

    assert!(is_agentics_fstab_entry(
        "/srv/agentics/loop-images/phase-solution-run.xfs /srv/agentics/phase-mounts/solution-run xfs loop,prjquota,nofail 0 0",
        &roots,
    ));
    assert!(!is_agentics_fstab_entry(
        "/dev/nvme0n1 /srv/agentics xfs defaults 0 0",
        &roots,
    ));
    assert!(!is_agentics_fstab_entry(
        "/srv/agentics/loop-images/unrelated.xfs /mnt/unrelated xfs loop,prjquota 0 0",
        &roots,
    ));
}

/// Verifies project-file cleanup rewrites only Agentics-owned lines and writes a backup.
#[tokio::test]
async fn project_cleanup_backs_up_and_preserves_unrelated_lines() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let projects = tempdir.path().join("projects");
    tokio::fs::write(
        &projects,
        "100001:/srv/agentics/phase-mounts/solution-run/slots/64mb/slot-001\n2:/var/lib/other\n",
    )
    .await
    .expect("write projects");
    let root = PathBuf::from("/srv/agentics");

    let message = remove_lines_matching_paths(&projects, &[&root])
        .await
        .expect("remove project entries");

    assert!(message.contains("backup"));
    assert_eq!(
        tokio::fs::read_to_string(&projects)
            .await
            .expect("read rewritten projects"),
        "2:/var/lib/other\n",
    );
    let backups = std::fs::read_dir(tempdir.path())
        .expect("read tempdir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("projects."))
        .count();
    assert_eq!(backups, 1);
}
