//! Crew Protocol v0 objects (`docs/crew/protocol`). The schemas there are the check:
//! `tests/crew_protocol` reads every example into these types and back, and refuses
//! every invalid one. Field names are the wire names, every object is closed to unknown
//! fields, and numbers are integers.
//!
//! An object carries the envelope (`v`, `id`, `crew_id`, `author`, `created_at`) and its
//! own fields; [`CrewObject`] adds the `type` tag. A `Brief` has no field for the answer
//! key, and [`AnswerKey`] is its own type with its own store table (H1).

/// Declares an object: the envelope fields first, then the object's own.
macro_rules! crew_object {
    ($(#[$meta:meta])* pub struct $name:ident { $($body:tt)* }) => {
        wire! {
            $(#[$meta])*
            #[serde(deny_unknown_fields)]
            pub struct $name {
                /// The protocol major version. Always 0.
                pub v: u8,
                /// The content address of the object without its `id`.
                pub id: $crate::crew::CrewHash,
                pub crew_id: $crate::crew::CrewId,
                pub author: $crate::crew::CrewAuthor,
                pub created_at: $crate::crew::CrewTimestamp,
                $($body)*
            }
        }
    };
}

mod answer_key;
mod board;
mod brief;
mod canonical;
mod check;
mod common;
mod evidence;
mod lease;
mod message;
mod report;
mod result;
mod review;
mod route;
mod text;

pub use answer_key::*;
pub use board::*;
pub use brief::*;
pub use canonical::{canonical_json, object_id};
pub use check::*;
pub use common::{CrewAuthor, CrewError, CrewRole, ErrorCode, Verdict, reject_floats};
pub use evidence::*;
pub use lease::*;
pub use message::*;
pub use report::*;
pub use result::*;
pub use review::*;
pub use route::*;
pub use text::*;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

wire! {
/// Any v0 object, selected by `type`.
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CrewObject {
    Brief(Brief),
    AnswerKey(AnswerKey),
    Lease(Lease),
    Message(CrewMessage),
    Fact(Fact),
    Claim(Claim),
    Report(Report),
    Evidence(Evidence),
    Finding(Finding),
    Disagreement(Disagreement),
    Gate(Gate),
    Result(CrewResult),
    RouteDecision(RouteDecision),
}
}

/// Runs `$body` with `$o` bound to whichever object `$self` holds.
macro_rules! each {
    ($self:expr, $o:ident => $body:expr) => {
        match $self {
            CrewObject::Brief($o) => $body,
            CrewObject::AnswerKey($o) => $body,
            CrewObject::Lease($o) => $body,
            CrewObject::Message($o) => $body,
            CrewObject::Fact($o) => $body,
            CrewObject::Claim($o) => $body,
            CrewObject::Report($o) => $body,
            CrewObject::Evidence($o) => $body,
            CrewObject::Finding($o) => $body,
            CrewObject::Disagreement($o) => $body,
            CrewObject::Gate($o) => $body,
            CrewObject::Result($o) => $body,
            CrewObject::RouteDecision($o) => $body,
        }
    };
}

impl CrewObject {
    pub fn id(&self) -> &CrewHash {
        each!(self, o => &o.id)
    }

    pub fn crew_id(&self) -> &CrewId {
        each!(self, o => &o.crew_id)
    }

    pub fn author(&self) -> &CrewAuthor {
        each!(self, o => &o.author)
    }

    /// The run the object belongs to. A brief and an answer key precede a run.
    pub fn run_id(&self) -> Option<&CrewId> {
        match self {
            Self::Brief(_) | Self::AnswerKey(_) => None,
            Self::Lease(o) => Some(&o.run_id),
            Self::Message(o) => Some(&o.run_id),
            Self::Fact(o) => Some(&o.run_id),
            Self::Claim(o) => Some(&o.run_id),
            Self::Report(o) => Some(&o.run_id),
            Self::Evidence(o) => Some(&o.run_id),
            Self::Finding(o) => Some(&o.run_id),
            Self::Disagreement(o) => Some(&o.run_id),
            Self::Gate(o) => Some(&o.run_id),
            Self::Result(o) => Some(&o.run_id),
            Self::RouteDecision(o) => Some(&o.run_id),
        }
    }

    /// The `type` tag, such as `route_decision`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Brief(_) => "brief",
            Self::AnswerKey(_) => "answer_key",
            Self::Lease(_) => "lease",
            Self::Message(_) => "message",
            Self::Fact(_) => "fact",
            Self::Claim(_) => "claim",
            Self::Report(_) => "report",
            Self::Evidence(_) => "evidence",
            Self::Finding(_) => "finding",
            Self::Disagreement(_) => "disagreement",
            Self::Gate(_) => "gate",
            Self::Result(_) => "result",
            Self::RouteDecision(_) => "route_decision",
        }
    }

    /// The rules JSON Schema states and serde cannot: ranges, lengths, cross-field rules
    /// and who may author each type.
    pub fn validate(&self) -> Verdict {
        each!(self, o => o.validate())
    }

    /// The canonical JSON of the object, with its `id`.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("a crew object is always JSON")
    }

    /// The content address this object should carry (SPEC.md section 2.3).
    pub fn computed_id(&self) -> Result<CrewHash, CrewError> {
        object_id(&self.to_value())
    }

    /// Reads and checks an object in the order the spec's errors apply: an unsupported
    /// `v`, then an invalid object (a float, an unknown type or field, a failed rule),
    /// then an `id` that is not the hash of the object.
    pub fn parse(value: &Value) -> Result<Self, CrewError> {
        if let Some(v) = value.get("v").and_then(Value::as_u64).filter(|v| *v != 0) {
            return Err(CrewError::new(
                ErrorCode::UnsupportedVersion,
                format!("protocol version {v} is not supported; this is v0"),
            ));
        }
        reject_floats(value)?;
        let object: Self = serde_json::from_value(value.clone())
            .map_err(|e| CrewError::new(ErrorCode::InvalidObject, e.to_string()))?;
        object.validate()?;
        let computed = object.computed_id()?;
        if &computed != object.id() {
            let mut error = CrewError::new(
                ErrorCode::IdMismatch,
                format!(
                    "id {} is not the hash of the object, which is {computed}",
                    object.id()
                ),
            );
            error.object = Some(object.id().clone());
            return Err(error);
        }
        Ok(object)
    }
}
