//! The result of one check run. Written only by the core.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::{CrewRole, Verdict, count, envelope, written_by};
use super::text::{CrewCommit, CrewHash, CrewId, CrewShortText};

wire_enum! {
pub enum CheckOutcome {
    Pass,
    Fail,
    Error,
    Skipped,
}
}

wire_enum! {
pub enum ArtifactKind {
    Screenshot,
    Diff,
    Report,
    Log,
}
}

wire! {
#[serde(deny_unknown_fields)]
pub struct EvidenceArtifact {
    pub kind: ArtifactKind,
    pub hash: CrewHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<CrewShortText>,
}
}

crew_object! {
/// What ran, on which commit, and what it proves and does not prove.
pub struct Evidence {
    pub run_id: CrewId,
    pub check: CrewId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub claim: Option<CrewHash>,
    /// The exact command, or the check's own description when nothing ran.
    pub command: CrewShortText,
    pub outcome: CheckOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_code: Option<u32>,
    #[ts(type = "number")]
    pub duration_ms: u64,
    pub commit: CrewCommit,
    pub log_hash: CrewHash,
    /// Redacted by the core before it is stored; at most 4000 characters.
    pub excerpt: String,
    pub artifacts: Vec<EvidenceArtifact>,
    pub proves: CrewShortText,
    pub does_not_prove: CrewShortText,
}
}

impl Evidence {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        written_by("evidence", &self.author, &[CrewRole::Core])?;
        if self.excerpt.chars().count() > 4000 {
            return Err(super::common::CrewError::invalid(
                "excerpt is at most 4000 characters",
            ));
        }
        count("artifacts", self.artifacts.len(), 0, 64)
    }
}
