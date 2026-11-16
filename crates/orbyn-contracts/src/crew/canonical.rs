//! The canonical form and the content address (SPEC.md section 2.3).

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::common::CrewError;
use super::text::CrewHash;

/// Compact JSON, object keys sorted by UTF-16 code unit, integers only. A float is an
/// invalid object, so every implementation computes the same hash.
pub fn canonical_json(value: &Value) -> Result<String, CrewError> {
    let mut out = String::new();
    write(value, &mut out)?;
    Ok(out)
}

fn write(value: &Value, out: &mut String) -> Result<(), CrewError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => out.push_str(&i.to_string()),
            (None, Some(u)) => out.push_str(&u.to_string()),
            _ => return Err(CrewError::invalid(format!("floating-point number {n}"))),
        },
        Value::String(s) => out.push_str(&serde_json::to_string(s).expect("a string is JSON")),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("a key is JSON"));
                out.push(':');
                write(&map[key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// `sha256:` and the lowercase hex hash of the canonical object without its `id`.
pub fn object_id(object: &Value) -> Result<CrewHash, CrewError> {
    let mut body = object.clone();
    body.as_object_mut()
        .ok_or_else(|| CrewError::invalid("an object is a JSON object"))?
        .remove("id");
    let digest = Sha256::digest(canonical_json(&body)?.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    CrewHash::new(format!("sha256:{hex}")).map_err(CrewError::invalid)
}
