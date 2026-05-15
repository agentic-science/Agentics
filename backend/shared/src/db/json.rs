use serde_json::Value;

use crate::error::{AppError, Result};

/// Handles decode optional json for this module.
pub(super) fn decode_optional_json<T>(value: Option<Value>, context: &str) -> Result<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    match value {
        Some(Value::Null) | None => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|e| AppError::Internal(format!("invalid {context}: {e}"))),
    }
}
