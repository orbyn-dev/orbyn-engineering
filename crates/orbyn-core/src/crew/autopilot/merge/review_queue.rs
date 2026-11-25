//! The after-merge Review queue: the person's stage. Every merge to main is one item, ranked by risk, with the diff, the
//! proof, what was not verified and the cost. The person answers "Looks good", "Comment" or "Revert". A revert is
//! opened and verified by the crew: it is made in a scratch checkout, the checks run on the result, and only then does the
//! branch move. Beside the merges the queue lists the background runs that stopped without a merge and wait for the person
//! (docs/crew/background-engine-start.md, section 4). The person settles one by acknowledging it through the background module
//! (`Core::background_acknowledge_review`): the queue itself only reads.

use std::collections::BTreeMap;
use std::sync::Arc;

use orbyn_contracts as wire;
use serde_json::{Value, json};

use super::queue::MergeTarget;
use super::risk::Risk;
use super::watch::revert_merge;
use crate::app::Core;
use crate::crew::capacity::{FIX_WAITING, paused_by_capacity};
use crate::crew::needs_you::Scope;
use crate::crew::wire_name;
use crate::error::{CoreError, Result};
use crate::store::MergeRecord;

/// One background run that stopped without a merge and waits for the person, with the crew it belongs to. The run has no
/// commit, diff or risk: its own state, why it stopped and what it spent are what a person reads.
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundReview {
    pub run_id: String,
    pub crew_id: String,
    pub project_id: String,
    pub crew_name: String,
    /// The stored template name; the wire item carries the enum.
    pub template: String,
    /// The stored state: `needs_review` or `stopped`.
    pub state: String,
    pub stop_reason: Option<String>,
    pub review_reason: Option<String>,
    pub turns_spent: u32,
    pub minutes_spent: u32,
    pub budget_turns: u32,
    pub budget_minutes: u32,
    pub started_at: String,
    pub ended_at: Option<String>,
}

/// What waits for the person: the merges to answer and, beside them, the background runs that stopped without a merge.
/// They are separate lists because a background run has no risk, commit or diff to rank it with a merge.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewQueue {
    /// The merges waiting for the person, most urgent first.
    pub merges: Vec<ReviewItem>,
    /// The background runs waiting for the person, oldest first.
    pub background: Vec<BackgroundReview>,
    /// The waits the one needs-you field (KC3) names that this queue does not answer: the Inbox's items this queue does not
    /// answer, by kind, most first then by kind. The page says "N more wait for you in the Inbox" from this, and it is the
    /// same field the Inbox and the Taskboard read, not a second rule (the counts need not be equal: the board cards only
    /// work tasks and this read is not scoped to one repository). A review wait this queue owns is not here.
    pub waiting_elsewhere: Vec<(wire::NeedsYouKind, u32)>,
}

/// One merge waiting for the person.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewItem {
    pub task_id: String,
    pub risk: Risk,
    pub reasons: Vec<String>,
    pub merge_commit: String,
    pub before_commit: String,
    pub candidate: String,
    pub files: u32,
    pub lines: u32,
    pub merged_unix: i64,
    /// When the review is due: high within 4 hours, medium within 24, low never.
    pub due_unix: Option<i64>,
    pub overdue: bool,
    /// The checks, the reviewers by engine, the findings and what was fixed.
    pub proof: Value,
    pub not_verified: Vec<String>,
    /// The task's own usage as the result recorded it; `None` when it did not.
    pub cost: Value,
    /// A break the crew was not certain enough to revert; the person decides.
    pub needs_person: Option<String>,
}

/// What the person says about a merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewAnswer {
    LooksGood,
    /// The person's comment, admitted as a normal scoped fix after it is stored.
    Comment(String),
    /// Open and verify a revert.
    Revert,
}

fn item(record: &MergeRecord, now: i64) -> ReviewItem {
    ReviewItem {
        task_id: record.task_id.clone(),
        risk: Risk::parse(&record.risk).unwrap_or(Risk::High),
        reasons: record
            .reasons
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r.as_str().map(str::to_owned))
            .collect(),
        merge_commit: record.merge_commit.clone(),
        before_commit: record.before_commit.clone(),
        candidate: record.candidate.clone(),
        files: record.files,
        lines: record.lines,
        merged_unix: record.merged_unix,
        due_unix: record.due_unix,
        overdue: record.due_unix.is_some_and(|due| now > due),
        cost: record.proof["cost"].clone(),
        proof: record.proof.clone(),
        not_verified: record
            .not_verified
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n.as_str().map(str::to_owned))
            .collect(),
        needs_person: record.needs_person.clone(),
    }
}

impl Core {
    /// What waits for the person: the merges, most urgent first (a break that needs a decision, then by risk, then oldest),
    /// and the background runs that stopped without a merge, oldest first. A background run stopped by the person's own
    /// switch, one still running, and one whose review the person already acknowledged is not waiting.
    pub fn crew_review_queue(&self, now: i64) -> Result<ReviewQueue> {
        let mut merges: Vec<ReviewItem> = self
            .store
            .crew_all_merges()?
            .iter()
            .filter(|m| m.review_state == "waiting")
            .map(|m| item(m, now))
            .collect();
        merges.sort_by(|a, b| {
            (
                b.needs_person.is_some(),
                b.risk,
                std::cmp::Reverse(a.merged_unix),
            )
                .cmp(&(
                    a.needs_person.is_some(),
                    a.risk,
                    std::cmp::Reverse(b.merged_unix),
                ))
        });
        // KC3: what waits elsewhere is read from the one needs-you field, never assembled here. The review waits this
        // queue answers (`MergeWaiting` and `BackgroundReview`) are left out: they are its own lists above.
        let mut elsewhere: BTreeMap<String, (wire::NeedsYouKind, u32)> = BTreeMap::new();
        for item in self.needs_you_all(Scope::Device)? {
            match item.kind {
                wire::NeedsYouKind::MergeWaiting | wire::NeedsYouKind::BackgroundReview => {}
                kind => {
                    let row = elsewhere.entry(wire_name(&kind)).or_insert((kind, 0));
                    row.1 = row.1.saturating_add(1);
                }
            }
        }
        let mut waiting_elsewhere: Vec<(wire::NeedsYouKind, u32)> =
            elsewhere.into_values().collect();
        waiting_elsewhere.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| wire_name(&a.0).cmp(&wire_name(&b.0)))
        });
        let background = self
            .store
            .background_reviews(None, None)?
            .into_iter()
            .map(|row| BackgroundReview {
                run_id: row.run.id,
                crew_id: row.run.background_id,
                project_id: row.project_id,
                crew_name: row.crew_name,
                template: row.template,
                state: row.run.state,
                stop_reason: row.run.stop_reason,
                review_reason: row.run.review_reason,
                turns_spent: row.run.turns_spent,
                minutes_spent: row.run.minutes_spent,
                budget_turns: row.run.budget_turns,
                budget_minutes: row.run.budget_minutes,
                started_at: row.run.started_at,
                ended_at: row.run.ended_at,
            })
            .collect();
        Ok(ReviewQueue {
            merges,
            background,
            waiting_elsewhere,
        })
    }

    /// The person's answer to a merge. A revert returns the revert commit; refused with the reason when it cannot be done safely.
    pub async fn crew_answer_review(
        self: &Arc<Self>,
        task_id: &str,
        answer: ReviewAnswer,
        target: &MergeTarget,
        now: i64,
    ) -> Result<Option<String>> {
        // One answer at a time, and the merge is read inside the lock: two reverts, or a "looks good" during a revert, cannot both
        // act on a record that was true when they started.
        let _one_at_a_time = self.crew_merges.lock().await;
        let record = self
            .store
            .crew_merge(task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("merge of task {task_id}")))?;
        if record.review_state == "reverted" {
            return Err(CoreError::Invalid("this merge was already reverted".into()));
        }
        match answer {
            ReviewAnswer::LooksGood => {
                self.store.set_merge_review(task_id, "looks_good", None)?;
                Ok(None)
            }
            ReviewAnswer::Comment(text) => {
                self.store
                    .set_merge_review(task_id, "commented", Some(&text))?;
                self.merge_event(
                    task_id,
                    "person_told",
                    &json!({ "about": "comment", "recorded": true }),
                );
                drop(_one_at_a_time);
                // Race: the locks are released here, so if a paused answer overlaps a successful start, the waiting note
                // can land after the start cleared it. The generic note had the same exposure; the next answer fixes it.
                let mut paused = false;
                let admitted: crate::error::Result<_> = async {
                    let run = self
                        .store
                        .crew_run(task_id)?
                        .ok_or_else(|| CoreError::NotFound("merge lead".into()))?;
                    let thread = self.store.thread(&run.lead_thread_id)?;
                    self.crew_merge_comment_fix(&thread, task_id)
                        .await
                        .map_err(|error| {
                            paused = paused_by_capacity(&error);
                            CoreError::Invalid(error.message)
                        })
                }
                .await;
                if let Err(error) = admitted {
                    // Admission can include an engine failure. Persist only code-owned text, never its output.
                    let why = if paused {
                        // Not a failure and never retried here: the Comment stays stored, and answering it again (or the
                        // lead's `crew_answer_review`) starts the fix once capacity frees.
                        FIX_WAITING
                    } else if matches!(error, CoreError::NotFound(_)) {
                        "Comment fix was not admitted: the merge lead or thread was not found."
                    } else {
                        "Comment fix was not admitted: review the task context before retrying."
                    };
                    self.store.set_merge_needs_person(task_id, Some(why))?;
                }
                Ok(None)
            }
            ReviewAnswer::Revert if self.store.crew_forge_merge(task_id)?.is_some() => {
                drop(_one_at_a_time);
                self.prepare_forge_correction(task_id, target, now).await?;
                Ok(None)
            }
            ReviewAnswer::Revert => match revert_merge(self, &record, target, now).await? {
                Ok(commit) => Ok(Some(commit)),
                Err(why) => Err(CoreError::Invalid(why)),
            },
        }
    }
}
