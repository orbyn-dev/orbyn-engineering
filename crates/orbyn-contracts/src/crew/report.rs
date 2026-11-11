//! What a worker hands back through `crew.report` at the end of an attempt. The
//! verifier runs after every report.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{Verdict, count, envelope, text};
use super::text::{CrewBranch, CrewCommit, CrewHash, CrewId, CrewShortText};

crew_object! {
pub struct Report {
    pub run_id: CrewId,
    /// The brief this report answers. The core fills it in.
    pub brief: CrewHash,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub branch: Option<CrewBranch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub commit: Option<CrewCommit>,
    pub claims: Vec<CrewHash>,
    /// What the worker knows no check covers.
    pub not_verified: Vec<CrewShortText>,
}
}

impl Report {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        text("summary", &self.summary, 2000)?;
        count("claims", self.claims.len(), 0, 256)?;
        count("not_verified", self.not_verified.len(), 0, 64)
    }
}
