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
mod flow;
mod flow_board;
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
pub use flow::*;
pub use flow_board::*;
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
/// Any v0 object, selected by `type`. It includes the answer key, so it belongs to the
/// core: never return it to a worker or a member (H1). Use [`WorkerObject`] there.
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

wire! {
/// The objects a worker is meant to see: the brief, facts and messages. Nothing that
/// carries the answer key's checks (evidence, gates, results, findings) is in it, so a
/// reader that returns it cannot leak them (H1). The H1 string check still runs on every
/// object before it reaches a worker, because a lead's message or a fact can quote a check.
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerObject {
    Brief(Brief),
    Message(CrewMessage),
    Fact(Fact),
}
}

impl TryFrom<CrewObject> for WorkerObject {
    type Error = CrewError;

    /// Refuses the answer key with `answer_key_forbidden` and any other type a worker is
    /// not shown with `not_authorized`.
    fn try_from(object: CrewObject) -> Result<Self, CrewError> {
        match object {
            CrewObject::Brief(o) => Ok(Self::Brief(o)),
            CrewObject::Message(o) => Ok(Self::Message(o)),
            CrewObject::Fact(o) => Ok(Self::Fact(o)),
            CrewObject::AnswerKey(_) => Err(CrewError::new(
                ErrorCode::AnswerKeyForbidden,
                "the answer key is never shown to a worker",
            )),
            other => Err(CrewError::new(
                ErrorCode::NotAuthorized,
                format!("a {} is not shown to a worker", other.type_name()),
            )),
        }
    }
}

impl From<WorkerObject> for CrewObject {
    fn from(object: WorkerObject) -> Self {
        match object {
            WorkerObject::Brief(o) => Self::Brief(o),
            WorkerObject::Message(o) => Self::Message(o),
            WorkerObject::Fact(o) => Self::Fact(o),
        }
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

    /// Reads the object by its `type`, keeping the path of the field that fails, which
    /// serde loses inside an internally tagged enum.
    fn read(value: &Value) -> Result<Self, CrewError> {
        fn body<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, CrewError> {
            let mut value = value.clone();
            if let Some(map) = value.as_object_mut() {
                map.remove("type");
            }
            serde_path_to_error::deserialize(value).map_err(|e| {
                CrewError::new(
                    ErrorCode::InvalidObject,
                    format!("{}: {}", e.path(), e.inner()),
                )
            })
        }
        let tag = value.get("type").and_then(Value::as_str);
        Ok(match tag {
            Some("brief") => Self::Brief(body(value)?),
            Some("answer_key") => Self::AnswerKey(body(value)?),
            Some("lease") => Self::Lease(body(value)?),
            Some("message") => Self::Message(body(value)?),
            Some("fact") => Self::Fact(body(value)?),
            Some("claim") => Self::Claim(body(value)?),
            Some("report") => Self::Report(body(value)?),
            Some("evidence") => Self::Evidence(body(value)?),
            Some("finding") => Self::Finding(body(value)?),
            Some("disagreement") => Self::Disagreement(body(value)?),
            Some("gate") => Self::Gate(body(value)?),
            Some("result") => Self::Result(body(value)?),
            Some("route_decision") => Self::RouteDecision(body(value)?),
            other => {
                return Err(CrewError::invalid(format!(
                    "type {other:?} is not a v0 object"
                )));
            }
        })
    }

    /// Reads and checks an object in the order the spec's errors apply: an unsupported
    /// `v`, then an invalid object (a float, an unknown type or field, a failed rule),
    /// then an `id` that is not the hash of the object.
    pub fn parse(value: &Value) -> Result<Self, CrewError> {
        if let Some(v) = value.get("v").and_then(Value::as_u64).filter(|v| *v != 0) {
            return Err(CrewError::new(
                ErrorCode::UnsupportedVersion,
                format!("v: protocol version {v} is not supported; this is v0"),
            ));
        }
        reject_floats(value)?;
        let object = Self::read(value)?;
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
