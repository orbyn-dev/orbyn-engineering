//! The envelope every object carries, the refusal object and the checks shared by the
//! per-type `validate` functions.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::text::{CrewHash, CrewMember, CrewToken};

wire_enum! {
pub enum CrewRole {
    Lead,
    Worker,
    Reviewer,
    Core,
    Person,
}
}

impl CrewRole {
    /// A member is a lead, a worker or a reviewer; the core and a person have no member id.
    pub fn is_member(self) -> bool {
        matches!(self, Self::Lead | Self::Worker | Self::Reviewer)
    }
}

wire! {
/// Who wrote an object. The core stamps it from the authenticated caller.
#[serde(deny_unknown_fields)]
pub struct CrewAuthor {
    pub role: CrewRole,
    pub engine: CrewToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub member: Option<CrewMember>,
}
}

wire_enum! {
/// The tool refusals of SPEC.md section 9.
pub enum ErrorCode {
    InvalidObject,
    UnsupportedVersion,
    IdMismatch,
    SecretDetected,
    AnswerKeyForbidden,
    ProtectedPathConflict,
    UnknownTool,
    NotAuthorized,
    DepthExceeded,
    RunEnded,
    HopLimitExceeded,
    PostLimitExceeded,
    LeaseConflict,
    LeaseNotHeld,
    LeaseExpired,
    BoardConflict,
    GateNeedsEvidence,
    InvalidTransition,
    InvalidToken,
    UnknownMember,
}
}

wire! {
/// What a crew tool returns when it refuses. A tool result, not a ledger object:
/// it has no id and is never hashed.
#[serde(deny_unknown_fields)]
pub struct CrewError {
    pub v: u8,
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub object: Option<CrewHash>,
}
}

impl CrewError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            v: 0,
            code,
            message: message.into(),
            object: None,
        }
    }

    pub(super) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidObject, message)
    }

    /// Reads a refusal object, naming the field that fails, then checks it.
    pub fn parse(value: &Value) -> Result<Self, CrewError> {
        let error: Self = serde_path_to_error::deserialize(value)
            .map_err(|e| Self::invalid(format!("{}: {}", e.path(), e.inner())))?;
        error.validate()?;
        Ok(error)
    }

    /// The error object itself must be a valid v0 object.
    pub fn validate(&self) -> Verdict {
        version(self.v)?;
        text("message", &self.message, 1000)
    }
}

impl std::fmt::Display for CrewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for CrewError {}

/// The outcome of a rule check: the refusal to return, or nothing.
pub type Verdict = Result<(), CrewError>;

/// A field that must be present but may be null (`"route": null`). Serde would read a
/// missing `Option` as `None`; this keeps a missing field an error.
pub(super) fn required<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

pub(super) fn version(v: u8) -> Verdict {
    if v == 0 {
        return Ok(());
    }
    Err(CrewError::new(
        ErrorCode::UnsupportedVersion,
        format!("v: protocol version {v} is not supported; this is v0"),
    ))
}

pub(super) fn text(what: &str, value: &str, max: usize) -> Verdict {
    let length = value.chars().count();
    if (1..=max).contains(&length) {
        return Ok(());
    }
    Err(CrewError::invalid(format!(
        "{what}: must be 1 to {max} characters, not {length}"
    )))
}

pub(super) fn count(what: &str, items: usize, min: usize, max: usize) -> Verdict {
    if (min..=max).contains(&items) {
        return Ok(());
    }
    Err(CrewError::invalid(format!(
        "{what}: must have {min} to {max} items, not {items}"
    )))
}

pub(super) fn within(what: &str, value: u64, min: u64, max: u64) -> Verdict {
    if (min..=max).contains(&value) {
        return Ok(());
    }
    Err(CrewError::invalid(format!(
        "{what}: must be {min} to {max}, not {value}"
    )))
}

pub(super) fn unique<T: PartialEq>(what: &str, items: &[T]) -> Verdict {
    if items
        .iter()
        .enumerate()
        .all(|(i, item)| !items[..i].contains(item))
    {
        return Ok(());
    }
    Err(CrewError::invalid(format!(
        "{what}: must not repeat an item"
    )))
}

/// The author rule: a member id exactly for a lead, a worker or a reviewer.
pub(super) fn author(author: &CrewAuthor) -> Verdict {
    if author.role.is_member() == author.member.is_some() {
        return Ok(());
    }
    Err(CrewError::invalid(if author.role.is_member() {
        "author: a lead, worker or reviewer names its member"
    } else {
        "author: the core and a person have no member id"
    }))
}

pub(super) fn written_by(what: &str, author: &CrewAuthor, allowed: &[CrewRole]) -> Verdict {
    if allowed.contains(&author.role) {
        return Ok(());
    }
    Err(CrewError::invalid(format!(
        "author.role: {what} is not written by a {:?}",
        author.role
    )))
}

/// The envelope: version, author and nothing else a type does not repeat.
pub(super) fn envelope(v: u8, author_of: &CrewAuthor) -> Verdict {
    version(v)?;
    author(author_of)
}

/// Integers only (SPEC.md section 2.3): a float anywhere makes an object invalid. The
/// message names the field it found it in.
pub fn reject_floats(value: &Value) -> Verdict {
    walk_floats(value, "")
}

fn walk_floats(value: &Value, path: &str) -> Verdict {
    match value {
        Value::Number(n) if !(n.is_i64() || n.is_u64()) => Err(CrewError::invalid(format!(
            "{}: floating-point number {n}: objects hold integers only",
            if path.is_empty() { "." } else { path }
        ))),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .try_for_each(|(i, item)| walk_floats(item, &format!("{path}[{i}]"))),
        Value::Object(map) => map.iter().try_for_each(|(key, item)| {
            let at = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            walk_floats(item, &at)
        }),
        _ => Ok(()),
    }
}
