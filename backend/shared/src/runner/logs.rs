use crate::models::evaluation::ScoringMode;
use crate::zip_project::ZipProjectPhaseName;

const OFFICIAL_LOG_REDACTION_NOTICE: &str =
    "[agentics] logs redacted for official private benchmark execution";
pub(super) const EVALUATION_LOG_BYTES_PER_RUN: u64 = 1024 * 1024;

/// Bounded persisted runner log accumulator for one evaluation.
#[derive(Debug)]
pub(super) struct EvaluationLogs {
    bytes: Vec<u8>,
    limit: usize,
    truncated: bool,
}

impl EvaluationLogs {
    /// Build a log accumulator capped by bytes.
    pub(super) fn new(limit_bytes: u64) -> Self {
        Self {
            bytes: Vec::new(),
            limit: usize::try_from(limit_bytes).unwrap_or(usize::MAX),
            truncated: false,
        }
    }

    /// Adjust the cap after the concrete run count is known.
    pub(super) fn set_limit(&mut self, limit_bytes: u64) {
        self.limit = usize::try_from(limit_bytes).unwrap_or(usize::MAX);
        if self.bytes.len() > self.limit {
            self.bytes.truncate(self.limit);
            self.mark_truncated();
        }
    }

    /// Borrow the accumulated bytes for durable storage.
    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Append bytes while preserving the configured cap.
    fn append_bytes(&mut self, bytes: &[u8]) {
        if self.truncated || bytes.is_empty() {
            return;
        }
        let remaining = self.limit.saturating_sub(self.bytes.len());
        if bytes.len() <= remaining {
            self.bytes.extend_from_slice(bytes);
            return;
        }
        self.bytes.extend(bytes.iter().copied().take(remaining));
        self.mark_truncated();
    }

    /// Add one truncation notice while keeping the final byte cap.
    fn mark_truncated(&mut self) {
        if self.truncated {
            return;
        }
        self.truncated = true;
        let notice = format!(
            "\n[agentics] evaluation logs truncated at {} bytes\n",
            self.limit
        );
        let notice = notice.as_bytes();
        if self.limit == 0 {
            self.bytes.clear();
            return;
        }
        if notice.len() >= self.limit {
            self.bytes.truncate(self.limit);
            return;
        }
        let Some(content_limit) = self.limit.checked_sub(notice.len()) else {
            self.bytes.truncate(self.limit);
            return;
        };
        if self.bytes.len() > content_limit {
            self.bytes.truncate(content_limit);
        }
        self.bytes.extend_from_slice(notice);
    }
}

/// Handles append phase logs for this module.
pub(super) fn append_phase_logs(
    logs: &mut EvaluationLogs,
    phase: ZipProjectPhaseName,
    content: &str,
) {
    append_named_logs(logs, &format!("phase:{}", phase_name(&phase)), content);
}

/// Handles append run logs for this module.
pub(super) fn append_run_logs(logs: &mut EvaluationLogs, run_name: &str, content: &str) {
    append_named_logs(logs, &format!("run:{run_name}"), content);
}

/// Handles append named logs for this module.
pub(super) fn append_named_logs(logs: &mut EvaluationLogs, name: &str, content: &str) {
    logs.append_bytes(b"\n===== ");
    logs.append_bytes(name.as_bytes());
    logs.append_bytes(b" =====\n");
    logs.append_bytes(content.as_bytes());
    if !content.ends_with('\n') {
        logs.append_bytes(b"\n");
    }
}

/// Return log content that is safe to persist for an evaluation mode.
pub(super) fn visible_log_content(eval_type: ScoringMode, content: &str) -> &str {
    match eval_type {
        ScoringMode::Validation => content,
        ScoringMode::Official => OFFICIAL_LOG_REDACTION_NOTICE,
    }
}

/// Return whether runner errors may include container log excerpts.
pub(super) fn include_log_excerpts(eval_type: ScoringMode) -> bool {
    matches!(eval_type, ScoringMode::Validation)
}

/// Handles append log excerpt for this module.
pub(super) fn append_log_excerpt(message: &str, logs: &str, include_logs: bool) -> String {
    if !include_logs {
        return message.to_string();
    }
    let trimmed = logs.trim();
    if trimmed.is_empty() {
        return message.to_string();
    }
    let excerpt: String = trimmed.chars().take(500).collect();
    format!("{message}; logs: {excerpt}")
}

/// Handles phase name for this module.
pub(super) fn phase_name(phase: &ZipProjectPhaseName) -> &'static str {
    match phase {
        ZipProjectPhaseName::Setup => "setup",
        ZipProjectPhaseName::Build => "build",
        ZipProjectPhaseName::Run => "run",
    }
}

#[cfg(test)]
mod tests {
    use crate::models::evaluation::ScoringMode;

    use super::{
        EvaluationLogs, append_log_excerpt, append_named_logs, include_log_excerpts,
        visible_log_content,
    };

    /// Verifies official runs never persist raw private benchmark logs.
    #[test]
    fn official_logs_are_redacted() {
        let sentinel = "PRIVATE_SENTINEL";
        let visible = visible_log_content(ScoringMode::Official, sentinel);

        assert!(!visible.contains(sentinel));
        assert!(visible.contains("redacted"));
        assert!(!include_log_excerpts(ScoringMode::Official));
    }

    /// Verifies official runner errors exclude raw log excerpts.
    #[test]
    fn official_error_excerpts_are_suppressed() {
        let message = append_log_excerpt("phase exited with status 1", "PRIVATE_SENTINEL", false);

        assert_eq!(message, "phase exited with status 1");
    }

    /// Verifies persisted evaluation logs are capped by bytes.
    #[test]
    fn evaluation_logs_truncate_with_notice() {
        let mut logs = EvaluationLogs::new(96);

        append_named_logs(&mut logs, "phase:setup", &"x".repeat(120));
        append_named_logs(&mut logs, "phase:build", "this should not fit");

        let text = String::from_utf8_lossy(logs.as_bytes());
        assert!(text.contains("truncated at 96 bytes"));
        assert!(logs.as_bytes().len() <= 96);
    }

    /// Verifies lowering the cap after prepare logs preserves the final bound.
    #[test]
    fn evaluation_logs_set_limit_truncates_existing_content() {
        let mut logs = EvaluationLogs::new(256);

        append_named_logs(&mut logs, "prepare", &"x".repeat(120));
        logs.set_limit(80);

        let text = String::from_utf8_lossy(logs.as_bytes());
        assert!(text.contains("truncated at 80 bytes"));
        assert!(logs.as_bytes().len() <= 80);
    }
}
