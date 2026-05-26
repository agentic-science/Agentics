//! Shared field-level validators used by DTO `garde` derives.

use garde::Error;

/// Require a string to contain visible non-whitespace content.
pub(crate) fn trimmed_non_empty(value: &str, _ctx: &()) -> Result<(), Error> {
    if value.trim().is_empty() {
        return Err(Error::new("must not be empty"));
    }
    Ok(())
}

/// Require an optional string, when present, to contain visible content.
pub(crate) fn optional_trimmed_non_empty(value: &Option<String>, _ctx: &()) -> Result<(), Error> {
    if let Some(value) = value
        && value.trim().is_empty()
    {
        return Err(Error::new("must not be empty when present"));
    }
    Ok(())
}

/// Reject NUL bytes in command argv parts.
pub(crate) fn no_nul(value: &str, _ctx: &()) -> Result<(), Error> {
    if value.contains('\0') {
        return Err(Error::new("must not contain NUL bytes"));
    }
    Ok(())
}
