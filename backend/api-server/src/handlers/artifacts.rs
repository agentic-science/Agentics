//! Artifact and log helpers for solution-submission routes.

use axum::Json;

use crate::error::ApiResult as Result;
use agentics_contracts::validation::archive::inspect_zip_bytes;
use agentics_contracts::zip_project::{MAX_ZIP_PROJECT_ARTIFACT_BYTES, zip_project_archive_policy};
use agentics_domain::error::ServiceError;
use agentics_domain::models::request::{
    SolutionSubmissionArtifactFileDto, SolutionSubmissionArtifactResponse,
    SolutionSubmissionLogsResponse,
};
use agentics_persistence::SolutionSubmissionRecord;

use crate::state::AppState;

const MAX_INLINE_TEXT_BYTES: u64 = 200_000;
const MAX_TOTAL_INLINE_TEXT_BYTES: u64 = 1_000_000;

/// Summarize a solution submission ZIP for safe public code browsing.
pub(super) async fn read_solution_submission_artifact_summary(
    artifact_key: &str,
    artifact_bytes: Vec<u8>,
) -> Result<SolutionSubmissionArtifactResponse> {
    let artifact_key = artifact_key.to_string();
    tokio::task::spawn_blocking(move || {
        read_solution_submission_artifact_summary_blocking(&artifact_key, artifact_bytes)
    })
    .await
    .map_err(|e| ServiceError::Internal(format!("artifact summary task failed: {e}")))?
}

/// Read a submitter-visible validation log response, truncating the payload for transport.
///
/// Official logs can contain evaluator output from private benchmark execution, so
/// they are intentionally not part of the participant-facing log surface.
pub(super) async fn read_solution_submission_logs(
    state: &AppState,
    solution_submission: &SolutionSubmissionRecord,
) -> Result<Json<SolutionSubmissionLogsResponse>> {
    const MAX_LOG_RESPONSE_BYTES: usize = 200_000;

    let log_key = solution_submission
        .validation_evaluation
        .as_ref()
        .and_then(|evaluation| evaluation.log_key.clone());

    let Some(log_key) = log_key else {
        return Ok(Json(SolutionSubmissionLogsResponse {
            solution_submission_id: solution_submission.id.clone(),
            log_key: None,
            content: None,
            truncated: false,
        }));
    };

    let max_stored_log_bytes = state
        .config
        .runner_max_runs
        .checked_mul(1024 * 1024)
        .ok_or_else(|| ServiceError::Internal("runner log byte budget overflow".to_string()))?;
    let bytes = state
        .storage
        .get(
            &log_key,
            agentics_storage::StorageWriteIntent::new("runner log", max_stored_log_bytes),
        )
        .await?;
    let truncated = bytes.len() > MAX_LOG_RESPONSE_BYTES;
    let visible_bytes = if truncated {
        bytes
            .get(..MAX_LOG_RESPONSE_BYTES)
            .ok_or_else(|| ServiceError::Internal("log truncation range invalid".to_string()))?
    } else {
        bytes.as_slice()
    };
    let content = String::from_utf8_lossy(visible_bytes).to_string();

    Ok(Json(SolutionSubmissionLogsResponse {
        solution_submission_id: solution_submission.id.clone(),
        log_key: Some(log_key),
        content: Some(content),
        truncated,
    }))
}

pub(super) fn solution_artifact_intent() -> agentics_storage::StorageWriteIntent {
    agentics_storage::StorageWriteIntent::new(
        "solution artifact ZIP",
        MAX_ZIP_PROJECT_ARTIFACT_BYTES,
    )
}

/// Perform ZIP parsing and safe summary construction on a blocking thread.
fn read_solution_submission_artifact_summary_blocking(
    artifact_key: &str,
    artifact_bytes: Vec<u8>,
) -> Result<SolutionSubmissionArtifactResponse> {
    let envelope = inspect_zip_bytes(&artifact_bytes, &zip_project_archive_policy())?;
    let archive_size = envelope.archive_size();
    let reader = std::io::Cursor::new(artifact_bytes);
    let mut archive = zip::ZipArchive::new(reader)?;

    let mut files = Vec::new();
    let mut total_inline_text_bytes = 0u64;

    for entry in envelope.entries() {
        if entry.is_dir() {
            continue;
        }

        let mut file = archive.by_index(entry.index())?;
        let entry_path = entry.path().as_str().to_string();
        let size = entry.size();

        let mut buf = Vec::new();
        let compressed_size = i64::try_from(entry.compressed_size()).map_err(|_| {
            ServiceError::BadRequest(
                "artifact ZIP entry compressed size exceeds supported range".to_string(),
            )
        })?;
        let projected_inline_text_bytes = total_inline_text_bytes.checked_add(size);
        let should_try_inline = size <= MAX_INLINE_TEXT_BYTES
            && projected_inline_text_bytes
                .is_some_and(|projected| projected <= MAX_TOTAL_INLINE_TEXT_BYTES);
        if should_try_inline {
            std::io::Read::read_to_end(&mut file, &mut buf)?;
        }

        let inline_text = if should_try_inline {
            std::str::from_utf8(&buf).ok()
        } else {
            None
        };
        let is_text = inline_text.is_some() || is_text_like_path(&entry_path);

        let content = if let Some(text) = inline_text {
            total_inline_text_bytes = total_inline_text_bytes
                .checked_add(u64::try_from(buf.len()).map_err(|_| {
                    ServiceError::BadRequest(
                        "artifact inline text size exceeds supported range".to_string(),
                    )
                })?)
                .ok_or_else(|| {
                    ServiceError::BadRequest("artifact inline text budget overflow".to_string())
                })?;
            Some(text.to_string())
        } else {
            None
        };

        files.push(SolutionSubmissionArtifactFileDto {
            path: entry_path.clone(),
            size: i64::try_from(size).map_err(|_| {
                ServiceError::BadRequest(
                    "artifact ZIP entry size exceeds supported range".to_string(),
                )
            })?,
            compressed_size,
            language: Some(infer_language(&entry_path)),
            is_text,
            content,
        });
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(SolutionSubmissionArtifactResponse {
        archive_name: std::path::Path::new(artifact_key)
            .file_name()
            .map(|file_name| file_name.to_string_lossy().to_string())
            .unwrap_or_default(),
        archive_size: i64::try_from(archive_size).map_err(|_| {
            ServiceError::BadRequest("artifact ZIP size exceeds supported range".to_string())
        })?,
        file_count: i64::try_from(files.len()).map_err(|_| {
            ServiceError::BadRequest("artifact ZIP file count exceeds supported range".to_string())
        })?,
        total_uncompressed_size: i64::try_from(envelope.expanded_size()).map_err(|_| {
            ServiceError::BadRequest(
                "artifact ZIP expanded size exceeds supported range".to_string(),
            )
        })?,
        files,
    })
}

/// Infer whether a file should be rendered as text even when inlining is skipped.
fn is_text_like_path(file_path: &str) -> bool {
    !matches!(infer_language(file_path).as_str(), "plaintext")
        || matches!(
            std::path::Path::new(file_path)
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("txt")
        )
}

/// Infer a display language from a source file extension.
fn infer_language(file_path: &str) -> String {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "py" => "python",
        "json" => "json",
        "md" => "markdown",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "yml" | "yaml" => "yaml",
        "toml" => "ini",
        "sh" => "shell",
        "sql" => "sql",
        "txt" => "plaintext",
        _ => "plaintext",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::PathBuf;

    use agentics_contracts::zip_project::MAX_ZIP_PROJECT_FILE_COUNT;
    use uuid::Uuid;

    use super::*;

    /// Build a temporary ZIP path for artifact summary tests.
    fn temp_zip_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("agentics-{name}-{}.zip", Uuid::new_v4()))
    }

    /// Write a small ZIP file containing the supplied entries.
    fn write_zip(path: &PathBuf, entries: Vec<(String, Vec<u8>)>) {
        let file = std::fs::File::create(path).expect("failed to create test zip");
        let mut archive = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, bytes) in entries {
            archive
                .start_file(name, options)
                .expect("failed to start zip entry");
            archive
                .write_all(&bytes)
                .expect("failed to write zip entry");
        }

        archive.finish().expect("failed to finish test zip");
    }

    #[tokio::test]
    /// Verifies unsafe archive entry names are rejected from artifact previews.
    async fn artifact_summary_rejects_unsafe_entry_names() {
        let path = temp_zip_path("unsafe-entry");
        write_zip(
            &path,
            vec![
                ("../escape.py".to_string(), b"print('bad')\n".to_vec()),
                ("main.py".to_string(), b"print('ok')\n".to_vec()),
            ],
        );

        let bytes = std::fs::read(&path).expect("failed to read test zip");
        let result =
            read_solution_submission_artifact_summary(&path.to_string_lossy(), bytes).await;
        drop(std::fs::remove_file(path));

        let error = result.expect_err("unsafe archive entry should be rejected");
        assert!(
            matches!(error.as_service_error(), ServiceError::Validation(message) if message.contains("unsafe"))
        );
    }

    #[tokio::test]
    /// Verifies previews reject archives that exceed the configured file-count limit.
    async fn artifact_summary_rejects_too_many_entries() {
        let path = temp_zip_path("too-many");
        let entries = (0..=MAX_ZIP_PROJECT_FILE_COUNT)
            .map(|i| (format!("file-{i}.txt"), Vec::new()))
            .collect();
        write_zip(&path, entries);

        let bytes = std::fs::read(&path).expect("failed to read test zip");
        let result =
            read_solution_submission_artifact_summary(&path.to_string_lossy(), bytes).await;
        drop(std::fs::remove_file(path));

        let error = result.expect_err("oversized archive should be rejected");
        assert!(
            matches!(error.as_service_error(), ServiceError::Validation(message) if message.contains("at most"))
        );
    }

    #[tokio::test]
    /// Verifies large text files are listed without inlining their contents.
    async fn artifact_summary_does_not_inline_large_text_entries() {
        let path = temp_zip_path("large-text");
        write_zip(
            &path,
            vec![(
                "main.py".to_string(),
                vec![b'a'; (MAX_INLINE_TEXT_BYTES + 1) as usize],
            )],
        );

        let bytes = std::fs::read(&path).expect("failed to read test zip");
        let summary = read_solution_submission_artifact_summary(&path.to_string_lossy(), bytes)
            .await
            .expect("summary should succeed");
        drop(std::fs::remove_file(path));

        assert_eq!(summary.file_count, 1);
        assert_eq!(summary.files[0].path, "main.py");
        assert!(summary.files[0].is_text);
        assert!(summary.files[0].content.is_none());
    }
}
