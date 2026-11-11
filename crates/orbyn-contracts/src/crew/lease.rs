//! A grant of paths to a member for a time (H10).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{CrewError, Verdict, count, envelope};
use super::text::{CrewId, CrewMember, CrewPathPattern, CrewShortText, CrewUtcSeconds};

wire_enum! {
pub enum LeaseMode {
    Read,
    Write,
}
}

wire_enum! {
pub enum LeaseState {
    Requested,
    Granted,
    Denied,
    Released,
    Transferred,
    Expired,
}
}

crew_object! {
pub struct Lease {
    pub run_id: CrewId,
    pub holder: CrewMember,
    pub paths: Vec<CrewPathPattern>,
    pub mode: LeaseMode,
    pub state: LeaseState,
    /// UTC, whole seconds, so two values compare as text.
    pub expires_at: CrewUtcSeconds,
    /// The new holder, when `state` is `transferred`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to: Option<CrewMember>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<CrewShortText>,
}
}

impl Lease {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        count("paths", self.paths.len(), 1, 256)?;
        match (self.state == LeaseState::Transferred, self.to.is_some()) {
            (true, false) => Err(CrewError::invalid(
                "a transferred lease names its new holder in `to`",
            )),
            (false, true) => Err(CrewError::invalid("only a transferred lease has `to`")),
            _ => Ok(()),
        }
    }

    /// Two leases conflict when their paths overlap, unless both only read or the same
    /// member holds both.
    pub fn conflicts_with(&self, other: &Self) -> bool {
        let both_read = self.mode == LeaseMode::Read && other.mode == LeaseMode::Read;
        !both_read
            && self.holder != other.holder
            && self
                .paths
                .iter()
                .any(|a| other.paths.iter().any(|b| a.overlaps(b)))
    }
}
