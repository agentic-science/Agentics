//! Shared text validation for public request and manifest fields.

use agentics_error::{Result, ServiceError};

/// Validate that a string field contains visible non-whitespace content.
pub fn require_non_empty(value: &str, field: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(ServiceError::Validation(format!(
            "{field} must not be empty"
        )));
    }

    Ok(())
}

/// Validate display text that is bounded by UTF-8 bytes and excludes binary controls.
pub fn validate_bounded_display_text(value: &str, field: &str, max_bytes: usize) -> Result<()> {
    if value.len() > max_bytes {
        return Err(ServiceError::Validation(format!(
            "{field} must be at most {max_bytes} UTF-8 bytes"
        )));
    }
    if value.chars().any(is_disallowed_display_text_char) {
        return Err(ServiceError::Validation(format!(
            "{field} must not contain non-text control characters"
        )));
    }

    Ok(())
}

/// Validate submitter-visible note text from `agentics.solution.json`.
pub fn validate_solution_note(note: &str, max_bytes: usize) -> Result<()> {
    validate_bounded_display_text(note, "note", max_bytes)
}

/// Return whether a decoded text character is not safe display text.
fn is_disallowed_display_text_char(ch: char) -> bool {
    ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')
}

#[cfg(test)]
mod tests {
    use super::{require_non_empty, validate_solution_note};

    #[test]
    fn validates_display_text_bounds_and_controls() {
        validate_solution_note("normal note\nwith tab\t", 1024).expect("text note should pass");

        let oversized = "x".repeat(1025);
        assert!(validate_solution_note(&oversized, 1024).is_err());
        assert!(validate_solution_note("bad\u{0007}", 1024).is_err());
    }

    #[test]
    fn rejects_empty_visible_text() {
        assert!(require_non_empty("value", "field").is_ok());
        assert!(require_non_empty(" \n\t", "field").is_err());
    }
}
