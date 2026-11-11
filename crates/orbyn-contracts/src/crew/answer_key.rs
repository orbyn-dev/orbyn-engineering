//! What the verifier checks. Only the core and the verifier ever see it (H1).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::check::CrewCheck;
use super::common::{Verdict, count, envelope};
use super::text::{CrewHash, CrewPathPattern};

crew_object! {
/// The checks for a brief, and the paths they run or read. Never sent to a worker.
/// Stored apart from the brief, so the type a worker reads cannot carry it.
pub struct AnswerKey {
    /// The brief this key belongs to.
    pub brief: CrewHash,
    pub checks: Vec<CrewCheck>,
    /// Every file the checks run or read. A brief's write leases never overlap them.
    pub protected_paths: Vec<CrewPathPattern>,
}
}

impl AnswerKey {
    pub fn validate(&self) -> Verdict {
        envelope(self.v, &self.author)?;
        count("checks", self.checks.len(), 1, usize::MAX)?;
        count("protected_paths", self.protected_paths.len(), 0, 256)?;
        self.checks.iter().try_for_each(CrewCheck::validate)
    }
}
