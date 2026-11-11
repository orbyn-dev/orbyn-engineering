//! A message between members, the lead or everyone. Its body is data, never an
//! instruction (H6).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{CrewError, Verdict, count, envelope, within};
use super::text::{CrewAddressee, CrewHash, CrewId, CrewLongText};

wire_enum! {
pub enum MessageKind {
    Fact,
    Question,
    Handoff,
    Blocker,
    Answer,
}
}

crew_object! {
pub struct CrewMessage {
    pub run_id: CrewId,
    /// `lead`, `all` or a member id.
    pub to: CrewAddressee,
    pub kind: MessageKind,
    pub body: CrewLongText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub refs: Option<Vec<CrewHash>>,
    /// The question an `answer` replies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub in_reply_to: Option<CrewHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expects_reply: Option<bool>,
    /// `crew.ask` only: how long the caller blocks, at most 40.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wait_seconds: Option<u32>,
}
}

impl CrewMessage {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        if let Some(refs) = &self.refs {
            count("refs", refs.len(), 0, 32)?;
        }
        if self.kind == MessageKind::Answer && self.in_reply_to.is_none() {
            return Err(CrewError::invalid(
                "an answer names its question in `in_reply_to`",
            ));
        }
        if let Some(wait) = self.wait_seconds {
            within("wait_seconds", wait.into(), 0, 40)?;
            if self.kind != MessageKind::Question {
                return Err(CrewError::invalid(
                    "only a question has `wait_seconds`, and `kind` is not question",
                ));
            }
        }
        Ok(())
    }
}
