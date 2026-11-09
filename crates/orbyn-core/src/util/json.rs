//! One string out of a JSON object: missing and non-string both read as nothing.

use serde_json::Value;

/// A string field, or the empty string when it is missing or not a string.
pub(crate) fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A non-empty string field, or nothing.
pub(crate) fn optional(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A string field as its own string, for values kept rather than borrowed.
pub(crate) fn text(value: &Value, key: &str) -> String {
    field(value, key).to_owned()
}
