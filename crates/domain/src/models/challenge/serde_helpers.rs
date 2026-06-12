//! Serde helpers for source challenge contracts.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Deserialize a field that must be present but may be JSON null.
pub(crate) fn required_nullable<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// Emit a schema for a field that must be present but may be JSON null.
pub(crate) fn required_nullable_schema<T>(generator: &mut SchemaGenerator) -> Schema
where
    T: JsonSchema,
{
    let value_schema = generator.subschema_for::<T>();
    json_schema!({
        "x-agentics-preserve-null": true,
        "oneOf": [
            { "type": "null" },
            value_schema
        ]
    })
}

/// Deserialize a required nullable vector where JSON null means no entries.
///
/// Empty arrays are rejected so authors use `null` for the semantic absence case
/// and a non-empty array only when entries are present.
pub(crate) fn required_nullable_non_empty_vec<'de, T, D>(
    deserializer: D,
) -> Result<Vec<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    match Option::<Vec<T>>::deserialize(deserializer)? {
        None => Ok(Vec::new()),
        Some(values) if values.is_empty() => Err(D::Error::custom(
            "must be null when absent or a non-empty array when present",
        )),
        Some(values) => Ok(values),
    }
}

/// Emit a schema for a required nullable vector where present arrays are non-empty.
pub(crate) fn required_nullable_non_empty_vec_schema<T>(generator: &mut SchemaGenerator) -> Schema
where
    T: JsonSchema,
{
    let item_schema = generator.subschema_for::<T>();
    json_schema!({
        "x-agentics-preserve-null": true,
        "oneOf": [
            { "type": "null" },
            {
                "type": "array",
                "minItems": 1,
                "items": item_schema
            }
        ]
    })
}

/// Serialize an empty vector as JSON null for required-nullable source fields.
pub(crate) fn serialize_empty_vec_as_null<T, S>(
    values: &[T],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    if values.is_empty() {
        serializer.serialize_none()
    } else {
        values.serialize(serializer)
    }
}
