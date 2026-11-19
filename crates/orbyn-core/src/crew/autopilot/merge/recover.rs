//! Recovery of local queue entries a crash left `merging`. A local merge writes `merging`, merges in a scratch checkout, then moves
//! the branch with one compare-and-swap `update-ref` and records the merge. A crash anywhere in that leaves the row `merging` for
//! good, which the queue never reads again and which refuses a withdraw of an escape check. The only effect on the repository is the
//! `update-ref`, so git tells which way it went: the branch holds the merge commit the queue makes (recorded now), or it does not
//! (nothing landed and the entry is `queued` again; a scratch checkout the crash left is only removed by git's own `worktree prune`), or git cannot say (a person decides).
//!
//! In flight is the merge lane's lock: `crew_process_merges` holds it from before it writes `merging` until the entry leaves it, so
//! with the lock held here no `merging` row is live. A pass waits for the lock (a live merge holds it, so a pass can wait for one merge and then hold the lane for up to 16 x 30 s), and passes repeat until one moves nothing, at most 8. A forge entry is
//! never read: its own intent has its own reconcile (`recover_crew_forge_merges`).

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;

use super::gitops;
use crate::app::Core;
use crate::error::Result;
use crate::store::MergeEntry;

/// The entries one pass looks at; the rest wait for the next pass.
const PER_PASS: u32 = 16;
/// Passes in one recovery: up to 128 entries.
const MAX_PASSES: usize = 8;
/// The time git gets for one entry.
const PER_ENTRY: Duration = Duration::from_secs(30);

/// What git said about one entry.
enum Verdict {
    Landed { merge: String, before: String },
    NotLanded,
    Unknown(String),
}

impl Core {
    /// Settles the local entries a crash left `merging`: passes of at most [`PER_PASS`] entries, at most [`MAX_PASSES`] of them, each
    /// under the lane's lock and stopping at the first that moves nothing. Returns how many entries it moved. Reads git only; never
    /// touches a forge. A failure on one entry leaves it for a person and does not stop the rest.
    pub(crate) async fn recover_crew_merging(self: &Arc<Self>, now: i64) -> Result<usize> {
        let mut total = 0;
        for _ in 0..MAX_PASSES {
            let moved = self.recover_merging_pass(now).await?;
            total += moved;
            if moved == 0 {
                break;
            }
        }
        Ok(total)
    }

    async fn recover_merging_pass(self: &Arc<Self>, now: i64) -> Result<usize> {
        let _lane = self.crew_merges.lock().await;
        let mut moved = 0;
        for entry in self.store.merging_local_entries(PER_PASS)? {
            let verdict = match tokio::time::timeout(PER_ENTRY, self.merging_verdict(&entry)).await
            {
                Ok(Ok(verdict)) => verdict,
                Ok(Err(error)) => Verdict::Unknown(error.public_message()),
                Err(_) => Verdict::Unknown("git did not answer in time".into()),
            };
            match self.settle_merging(&entry, verdict, now) {
                Ok(true) => moved += 1,
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(task = %entry.task_id, %error, "a merge a crash interrupted could not be settled");
                }
            }
        }
        Ok(moved)
    }

    async fn merging_verdict(&self, entry: &MergeEntry) -> Result<Verdict> {
        let repo = std::path::Path::new(&entry.repo);
        gitops::rev(repo, &entry.branch).await?;
        gitops::rev(repo, &entry.candidate).await?;
        if !gitops::already_on(repo, &entry.branch, &entry.candidate).await? {
            return Ok(Verdict::NotLanded);
        }
        let subject = Self::merge_subject(&entry.task_id);
        Ok(
            match gitops::landed_merge(repo, &entry.branch, &entry.candidate, &subject).await? {
                Some((merge, before)) => Verdict::Landed { merge, before },
                None => Verdict::Unknown(format!(
                    "{} holds the candidate, but no merge the queue made is among its last {} commits",
                    entry.branch,
                    gitops::LANDED_WINDOW
                )),
            },
        )
    }

    fn settle_merging(&self, entry: &MergeEntry, verdict: Verdict, now: i64) -> Result<bool> {
        let task = entry.task_id.as_str();
        match verdict {
            Verdict::Landed { merge, before } => {
                self.record_merge(
                    entry,
                    &before,
                    &merge,
                    0,
                    now,
                    Some("Recorded after a crash: the merge is on the branch, so a person should look at it."),
                )?;
                self.merge_event(
                    task,
                    "merge_recovered",
                    &json!({ "outcome": "merged", "merge": merge }),
                );
                Ok(true)
            }
            Verdict::NotLanded => {
                let moved = self.merge_state(
                    task,
                    &["merging"],
                    "queued",
                    Some("A crash interrupted this merge before the branch moved, so nothing landed; it is queued again."),
                )?;
                if moved {
                    self.merge_event(task, "merge_recovered", &json!({ "outcome": "queued" }));
                }
                Ok(moved)
            }
            Verdict::Unknown(why) => {
                let why = format!(
                    "A crash interrupted this merge and git cannot say whether it landed: {why}. A person decides."
                );
                let moved = self.merge_state(task, &["merging"], "waiting", Some(&why))?;
                if moved {
                    self.merge_event(
                        task,
                        "merge_waiting",
                        &json!({ "why": why, "recovered": true, "risk": entry.risk }),
                    );
                }
                Ok(moved)
            }
        }
    }
}
