//! The debug-only dev seed behind `POST /api/crew/dev/seed-proposal` (KC9): a web real-service spec can make a skill
//! proposal without running three real confirmed escapes.
//!
//! The module, its method and the route exist only in a debug build; a release build has none of them. The route is
//! registered only when `ORBYN_DEV_SEED_PROPOSALS` is `1` or `true` at service start, so an ordinary debug run has no
//! such path. It writes generic escape-source lessons through the same store writer and the same proposal maker the
//! real learners use: no answer key, no task, no engine and no network. Every use leaves one audit row, written first
//! in the same transaction as its lesson rows, with counts only and never lesson text. Crew Preview applies as it does
//! to every Crew method.

use serde_json::json;

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{AuditRecord, LessonEvidence, proposal_group_key};

/// How many lessons one call may seed.
const LESSONS_MAX: u32 = 6;

/// What one debug seed wrote: the new lessons, oldest first, and the open proposal of their group when one exists after
/// the maker ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevSeedProposal {
    pub lessons: Vec<String>,
    pub proposal_id: Option<String>,
}

#[cfg(debug_assertions)]
impl Core {
    /// The debug-only test seam: writes `lessons` active escape lessons of `repo` under `area`, each with one
    /// `dev-seed-<n>` evidence id and no fingerprint, then calls the proposal maker once. Answers the written lesson
    /// ids and the open proposal of that group, if the group has one after the call.
    pub fn dev_seed_proposal(
        &self,
        repo: &str,
        lessons: u32,
        area: &str,
    ) -> Result<DevSeedProposal> {
        self.crew_preview_only()?;
        if !(1..=LESSONS_MAX).contains(&lessons) {
            return Err(CoreError::Invalid(format!(
                "lessons must be 1 to {LESSONS_MAX}"
            )));
        }
        self.store.project(repo)?;
        let rows: Vec<(String, Vec<LessonEvidence>)> = (1..=lessons)
            .map(|n| {
                (
                    format!("Dev seed lesson {n} for {area}."),
                    vec![LessonEvidence {
                        kind: "escape".to_owned(),
                        id: format!("dev-seed-{n}"),
                    }],
                )
            })
            .collect();
        let audit = AuditRecord {
            actor: "dev".to_owned(),
            action: "crew.dev.seed_proposal".to_owned(),
            target: repo.to_owned(),
            decision: None,
            source: "dev".to_owned(),
            thread_id: None,
            run_id: None,
            detail: json!({ "lessons": lessons }),
        };
        let written = self
            .store
            .write_escape_lessons(repo, area, &rows, Some(&audit))?;
        self.learn_skill_proposals(repo);
        let group_key = proposal_group_key("escape", None, Some(area));
        let proposal_id = self
            .store
            .skill_proposals(repo)?
            .into_iter()
            .find(|proposal| proposal.group_key == group_key && proposal.state == "open")
            .map(|proposal| proposal.id);
        Ok(DevSeedProposal {
            lessons: written.into_iter().map(|lesson| lesson.id).collect(),
            proposal_id,
        })
    }
}
