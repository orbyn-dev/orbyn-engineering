//! The shared board: typed facts, and claims with the check that would prove them.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::check::CrewCheck;
use super::common::{CrewError, Verdict, count, envelope, reject_floats};
use super::text::{CrewFactKey, CrewHash, CrewId, CrewShortText};

wire_enum! {
pub enum FactClass {
    Contract,
    Decision,
    Observation,
}
}

crew_object! {
/// A contract fact carries the check every consumer gets.
pub struct Fact {
    pub run_id: CrewId,
    pub key: CrewFactKey,
    pub class: FactClass,
    /// A string, integer, boolean, object or array. Never null, never a float.
    pub value: Value,
    /// The revision this write creates.
    #[ts(type = "number")]
    pub rev: u64,
    /// The revision the writer saw; 0 for a new key.
    #[ts(type = "number")]
    pub base_rev: u64,
    pub evidence: Vec<CrewHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub check: Option<CrewCheck>,
}
}

impl Fact {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        if self.value.is_null() || self.value.is_f64() {
            return Err(CrewError::invalid(
                "a fact value is a string, integer, boolean, object or array",
            ));
        }
        reject_floats(&self.value)?;
        if self.rev < 1 {
            return Err(CrewError::invalid("rev starts at 1"));
        }
        count("evidence", self.evidence.len(), 0, 32)?;
        match (&self.check, self.class == FactClass::Contract) {
            (None, true) => Err(CrewError::invalid(
                "a contract fact carries the check its consumers get",
            )),
            (Some(check), _) => check.validate(),
            _ => Ok(()),
        }
    }
}

crew_object! {
/// A statement a member makes. Without a check it shows as unverified.
pub struct Claim {
    pub run_id: CrewId,
    pub statement: CrewShortText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub check: Option<CrewCheck>,
}
}

impl Claim {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        self.check.as_ref().map_or(Ok(()), CrewCheck::validate)
    }
}
