use crate::models::evaluation::ScoringMode;
use crate::zip_project::ZipProjectPhaseName;

const OFFICIAL_LOG_REDACTION_NOTICE: &str =
    "[agentics] logs redacted for official private benchmark execution";

/// Handles append phase logs for this module.
pub(super) fn append_phase_logs(logs: &mut String, phase: ZipProjectPhaseName, content: &str) {
    append_named_logs(logs, &format!("phase:{}", phase_name(&phase)), content);
}

/// Handles append run logs for this module.
pub(super) fn append_run_logs(logs: &mut String, run_name: &str, content: &str) {
    append_named_logs(logs, &format!("run:{run_name}"), content);
}

/// Handles append named logs for this module.
pub(super) fn append_named_logs(logs: &mut String, name: &str, content: &str) {
    logs.push_str("\n===== ");
    logs.push_str(name);
    logs.push_str(" =====\n");
    logs.push_str(content);
    if !content.ends_with('\n') {
        logs.push('\n');
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

    use super::{append_log_excerpt, include_log_excerpts, visible_log_content};

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
}
