//! Maintenance queries used by server startup and worker liveness.

use std::path::{Path, PathBuf};

use sqlx::{PgPool, Row};

use super::ids::{solution_submission_id_from_row, uuid_string_from_row};
use super::leaderboard::repair_leaderboard_entry_for_solution_submission_tx;
use crate::db::challenges::PublishChallengeInput;
use agentics_domain::error::{Result, ServiceError};
use agentics_domain::models::evaluation::ScoringMode;
use agentics_domain::models::ids::{EvaluationJobId, SolutionSubmissionId};
use agentics_domain::models::request::AdminServiceHeartbeatDto;
use agentics_storage::{
    Storage, StorageError, StorageKey, StorageWriteIntent, pack_directory_to_tar, storage_work_root,
};

/// JSON payload stored with each service heartbeat.
///
/// Optional fields are omitted to keep the admin-facing heartbeat document
/// compact and compatible with the relaxed JSON shape used elsewhere.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HeartbeatPayload {
    pub status: String,
    pub accelerators: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<EvaluationJobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solution_submission_id: Option<SolutionSubmissionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_completed_job_id: Option<EvaluationJobId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_failed_job_id: Option<EvaluationJobId>,
}

/// Insert or refresh the latest heartbeat for a named service instance.
pub async fn upsert_service_heartbeat(
    pool: &PgPool,
    service_name: &str,
    payload: &HeartbeatPayload,
) -> Result<()> {
    let payload_json =
        serde_json::to_value(payload).map_err(|e| ServiceError::Internal(e.to_string()))?;

    sqlx::query(
        r#"
        INSERT INTO service_heartbeats (service_name, last_seen_at, payload)
        VALUES ($1, NOW(), $2)
        ON CONFLICT (service_name)
        DO UPDATE SET last_seen_at = EXCLUDED.last_seen_at, payload = EXCLUDED.payload
        "#,
    )
    .bind(service_name)
    .bind(&payload_json)
    .execute(pool)
    .await?;

    Ok(())
}

/// List latest service heartbeats for the admin operations console.
pub async fn list_service_heartbeats(pool: &PgPool) -> Result<Vec<AdminServiceHeartbeatDto>> {
    let rows = sqlx::query_as::<_, (String, chrono::DateTime<chrono::Utc>, serde_json::Value)>(
        r#"
        SELECT service_name, last_seen_at, payload
        FROM service_heartbeats
        ORDER BY last_seen_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(service_name, last_seen_at, payload)| AdminServiceHeartbeatDto {
                service_name,
                last_seen_at: last_seen_at.to_rfc3339(),
                payload,
            },
        )
        .collect())
}

/// Seed or refresh published challenges by scanning a bundle root.
///
/// Each immediate child directory may contain one or more bundle directories.
/// Directories without `spec.json` are ignored so local notes or partial bundles
/// do not block startup.
pub async fn ensure_challenges_seeded_from_root(
    pool: &PgPool,
    config: &agentics_config::Config,
    storage: &dyn Storage,
    challenges_root: &str,
) -> Result<usize> {
    tokio::fs::create_dir_all(challenges_root).await?;
    let mut entries = tokio::fs::read_dir(challenges_root).await?;
    let mut challenge_dirs = Vec::new();
    let mut synced = 0usize;

    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            challenge_dirs.push(entry.path());
        }
    }
    challenge_dirs.sort();

    for challenge_root in challenge_dirs {
        let mut bundles = tokio::fs::read_dir(&challenge_root).await?;
        let mut bundle_dirs: Vec<PathBuf> = Vec::new();

        while let Some(bundle_entry) = bundles.next_entry().await? {
            if !bundle_entry.file_type().await?.is_dir() {
                continue;
            }
            let bundle_dir = bundle_entry.path();
            if tokio::fs::try_exists(bundle_dir.join("spec.json")).await? {
                bundle_dirs.push(bundle_dir);
            }
        }
        bundle_dirs.sort();

        for bundle_dir in bundle_dirs {
            agentics_contracts::challenge_bundle::validate_challenge_bundle(&bundle_dir).await?;
            let spec =
                agentics_contracts::challenge_bundle::read_challenge_bundle_spec(&bundle_dir)
                    .await?;
            let bundle_digest =
                agentics_contracts::challenge_bundle::challenge_bundle_tree_sha256(&bundle_dir)
                    .await?;
            let private_bundle_key = bundle_storage_key(
                "challenge-bundles",
                spec.challenge_name.as_str(),
                &bundle_digest.to_hex(),
            )?;
            let public_bundle_dir =
                seeded_public_bundle_dir(config, &bundle_dir, &spec, &bundle_digest.to_hex())
                    .await?;
            let public_digest = agentics_contracts::challenge_bundle::challenge_bundle_tree_sha256(
                &public_bundle_dir,
            )
            .await?;
            let public_bundle_key = bundle_storage_key(
                "challenge-public-bundles",
                spec.challenge_name.as_str(),
                &public_digest.to_hex(),
            )?;
            let statement_key = StorageKey::try_new(format!(
                "challenge-statements/{}/{}.md",
                spec.challenge_name,
                bundle_digest.to_hex()
            ))?;
            put_bundle_archive_if_missing(
                storage,
                config,
                &private_bundle_key,
                &bundle_dir,
                "seeded-private",
            )
            .await?;
            put_bundle_archive_if_missing(
                storage,
                config,
                &public_bundle_key,
                &public_bundle_dir,
                "seeded-public",
            )
            .await?;
            put_statement_if_missing(
                storage,
                config,
                &statement_key,
                &bundle_dir.join("statement.md"),
            )
            .await?;
            let challenge_name = &spec.challenge_name;

            if crate::db::challenges::publish_challenge(
                pool,
                &PublishChallengeInput {
                    challenge_name,
                    bundle_key: &private_bundle_key,
                    public_bundle_key: &public_bundle_key,
                    statement_key: &statement_key,
                    spec: &spec,
                    title: &spec.challenge_title,
                    summary: &spec.summary,
                },
            )
            .await
            .is_err()
            {
                sqlx::query(
                    r#"
                    UPDATE challenges
                    SET title = $2,
                        summary = $3,
                        bundle_key = $4,
                        public_bundle_key = $5,
                        statement_key = $6,
                        spec_json = $7,
                        status = 'active',
                        updated_at = NOW()
                    WHERE challenge_name = $1
                    "#,
                )
                .bind(challenge_name.as_str())
                .bind(&spec.challenge_title)
                .bind(
                    serde_json::to_value(&spec.summary)
                        .map_err(|e| ServiceError::Internal(e.to_string()))?,
                )
                .bind(private_bundle_key.as_str())
                .bind(public_bundle_key.as_str())
                .bind(statement_key.as_str())
                .bind(
                    serde_json::to_value(&spec)
                        .map_err(|e| ServiceError::Internal(e.to_string()))?,
                )
                .execute(pool)
                .await?;
            }

            synced = synced.checked_add(1).ok_or_else(|| {
                ServiceError::Internal("challenge sync count overflow".to_string())
            })?;
        }
    }

    Ok(synced)
}

/// Return a public-only bundle directory for a seeded challenge.
async fn seeded_public_bundle_dir(
    config: &agentics_config::Config,
    bundle_dir: &Path,
    spec: &agentics_domain::models::challenge::ChallengeBundleSpec,
    digest: &str,
) -> Result<PathBuf> {
    if !spec.datasets.private_benchmark_enabled {
        return Ok(bundle_dir.to_path_buf());
    }

    let target = storage_work_root(config)?
        .join("seeded-public-bundles")
        .join(spec.challenge_name.as_str())
        .join(digest);
    if let Some(private_benchmark_dir) = &spec.datasets.private_benchmark_dir {
        agentics_contracts::challenge_bundle::copy_challenge_bundle_dir_excluding(
            bundle_dir,
            &target,
            private_benchmark_dir.as_path(),
            true,
        )
        .await?;
    } else {
        agentics_contracts::challenge_bundle::copy_challenge_bundle_dir(bundle_dir, &target, true)
            .await?;
    }

    Ok(target)
}

async fn put_bundle_archive_if_missing(
    storage: &dyn Storage,
    config: &agentics_config::Config,
    key: &StorageKey,
    bundle_dir: &Path,
    label: &str,
) -> Result<()> {
    if storage.exists(key).await? {
        return Ok(());
    }
    let archive_path = storage_work_root(config)?
        .join("_tmp")
        .join(format!("{label}-{}.tar", uuid::Uuid::new_v4()));
    pack_directory_to_tar(
        bundle_dir,
        &archive_path,
        StorageWriteIntent::new(
            "challenge bundle archive",
            config.storage.max_bundle_archive_bytes,
        ),
    )
    .await?;
    let result = storage
        .put_file(
            key,
            &archive_path,
            StorageWriteIntent::new(
                "challenge bundle archive",
                config.storage.max_bundle_archive_bytes,
            ),
        )
        .await;
    let cleanup = tokio::fs::remove_file(&archive_path).await;
    if let Err(error) = cleanup
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error.into());
    }
    match result {
        Ok(_) => Ok(()),
        Err(StorageError::ObjectConflict(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn put_statement_if_missing(
    storage: &dyn Storage,
    config: &agentics_config::Config,
    key: &StorageKey,
    statement_path: &Path,
) -> Result<()> {
    if storage.exists(key).await? {
        return Ok(());
    }
    let bytes = tokio::fs::read(statement_path).await?;
    match storage
        .put(
            key,
            &bytes,
            StorageWriteIntent::new("challenge statement", config.storage.max_statement_bytes),
        )
        .await
    {
        Ok(_) | Err(StorageError::ObjectConflict(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn bundle_storage_key(prefix: &str, challenge_name: &str, digest: &str) -> Result<StorageKey> {
    StorageKey::try_new(format!("{prefix}/{challenge_name}/{digest}.tar")).map_err(Into::into)
}

/// Summary of stale job recovery work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StaleJobReapResult {
    pub requeued: u64,
    pub failed: u64,
}

/// Recover running jobs whose worker lease has expired.
///
/// Jobs with attempts remaining return to the queue. Jobs that have exhausted
/// their retry budget move to `failed` together with their associated
/// evaluation and solution submission.
pub async fn reap_stuck_jobs(pool: &PgPool, timeout_minutes: i32) -> Result<StaleJobReapResult> {
    let mut tx = pool.begin().await?;

    let staged_jobs = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'failed',
            finished_at = NOW(),
            last_error = 'staged job was not promoted before timeout',
            worker_id = NULL,
            claimed_at = NULL
        WHERE status = 'staged'
          AND scheduled_at < NOW() - INTERVAL '1 minute' * $1
        RETURNING id, solution_submission_id, eval_type
        "#,
    )
    .bind(timeout_minutes)
    .fetch_all(&mut *tx)
    .await?;

    for row in &staged_jobs {
        let solution_submission_id =
            solution_submission_id_from_row(row, "solution_submission_id")?;
        sqlx::query(
            r#"
            UPDATE solution_submissions
            SET status = 'failed',
                visible_after_eval = FALSE,
                updated_at = NOW()
            WHERE id = $1::uuid
            "#,
        )
        .bind(solution_submission_id.as_str())
        .execute(&mut *tx)
        .await?;
    }

    let requeued_jobs = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'queued', worker_id = NULL, claimed_at = NULL
        WHERE status = 'running'
          AND claimed_at < NOW() - INTERVAL '1 minute' * $1
          AND attempt_count < max_attempts
        RETURNING id, solution_submission_id, eval_type
        "#,
    )
    .bind(timeout_minutes)
    .fetch_all(&mut *tx)
    .await?;

    for row in &requeued_jobs {
        let job_id = uuid_string_from_row(row, "id")?;
        let solution_submission_id =
            solution_submission_id_from_row(row, "solution_submission_id")?;
        let eval_type = eval_type_from_row(row, "eval_type")?;
        sqlx::query("DELETE FROM evaluations WHERE job_id = $1::uuid AND status = 'running'")
            .bind(&job_id)
            .execute(&mut *tx)
            .await?;
        if preserve_visible_official_result_tx(&mut tx, &solution_submission_id, &job_id, eval_type)
            .await?
        {
            continue;
        }
        let was_visible =
            hide_reaped_solution_submission_tx(&mut tx, &solution_submission_id, "queued").await?;
        if was_visible {
            repair_leaderboard_entry_for_solution_submission_tx(&mut tx, &solution_submission_id)
                .await?;
        }
    }

    let failed_jobs = sqlx::query(
        r#"
        UPDATE evaluation_jobs
        SET status = 'failed',
            finished_at = NOW(),
            last_error = 'worker lease expired after max attempts',
            worker_id = NULL,
            claimed_at = NULL
        WHERE status = 'running'
          AND claimed_at < NOW() - INTERVAL '1 minute' * $1
          AND attempt_count >= max_attempts
        RETURNING id, solution_submission_id, eval_type
        "#,
    )
    .bind(timeout_minutes)
    .fetch_all(&mut *tx)
    .await?;

    for row in &failed_jobs {
        let job_id = uuid_string_from_row(row, "id")?;
        let solution_submission_id =
            solution_submission_id_from_row(row, "solution_submission_id")?;
        let eval_type = eval_type_from_row(row, "eval_type")?;
        sqlx::query(
            r#"
            UPDATE evaluations
            SET status = 'failed',
                finished_at = NOW()
            WHERE job_id = $1::uuid
              AND status = 'running'
            "#,
        )
        .bind(&job_id)
        .execute(&mut *tx)
        .await?;

        if preserve_visible_official_result_tx(&mut tx, &solution_submission_id, &job_id, eval_type)
            .await?
        {
            continue;
        }
        let was_visible =
            hide_reaped_solution_submission_tx(&mut tx, &solution_submission_id, "failed").await?;
        if was_visible {
            repair_leaderboard_entry_for_solution_submission_tx(&mut tx, &solution_submission_id)
                .await?;
        }
    }

    tx.commit().await?;

    Ok(StaleJobReapResult {
        requeued: u64::try_from(requeued_jobs.len())
            .map_err(|_| ServiceError::Internal("requeued job count overflow".to_string()))?,
        failed: u64::try_from(failed_jobs.len().saturating_add(staged_jobs.len()))
            .map_err(|_| ServiceError::Internal("failed job count overflow".to_string()))?,
    })
}

/// Parse one persisted evaluation type from a maintenance query row.
fn eval_type_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<ScoringMode> {
    let value: String = row.try_get(column)?;
    ScoringMode::from_storage_value(&value)
        .ok_or_else(|| ServiceError::Internal(format!("unknown stored {column} `{value}`")))
}

/// Keep an older completed official result visible while a stale official rerun is repaired.
async fn preserve_visible_official_result_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    stale_job_id: &str,
    eval_type: ScoringMode,
) -> Result<bool> {
    if eval_type != ScoringMode::Official {
        return Ok(false);
    }

    let has_prior_completed_official = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM evaluations
            WHERE solution_submission_id = $1::uuid
              AND eval_type = 'official'
              AND status = 'completed'
              AND job_id <> $2::uuid
        )
        "#,
    )
    .bind(solution_submission_id.as_str())
    .bind(stale_job_id)
    .fetch_one(&mut **tx)
    .await?;
    if !has_prior_completed_official {
        return Ok(false);
    }

    sqlx::query(
        r#"
        UPDATE solution_submissions
        SET status = 'completed',
            visible_after_eval = TRUE,
            updated_at = NOW()
        WHERE id = $1::uuid
        "#,
    )
    .bind(solution_submission_id.as_str())
    .execute(&mut **tx)
    .await?;

    Ok(true)
}

/// Apply the original stale-job fallback and report whether public visibility changed.
async fn hide_reaped_solution_submission_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    solution_submission_id: &SolutionSubmissionId,
    next_status: &str,
) -> Result<bool> {
    let was_visible = sqlx::query_scalar::<_, bool>(
        "SELECT visible_after_eval FROM solution_submissions WHERE id = $1::uuid FOR UPDATE",
    )
    .bind(solution_submission_id.as_str())
    .fetch_optional(&mut **tx)
    .await?
    .unwrap_or(false);

    sqlx::query(
        r#"
        UPDATE solution_submissions
        SET status = $2,
            visible_after_eval = FALSE,
            updated_at = NOW()
        WHERE id = $1::uuid
        "#,
    )
    .bind(solution_submission_id.as_str())
    .bind(next_status)
    .execute(&mut **tx)
    .await?;

    Ok(was_visible)
}
