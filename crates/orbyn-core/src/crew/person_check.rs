//! The check a person makes (issue #794): a `manual` check of an answer key waits for a person, and their decision is what the verifier
//! reads on the next pass.
//!
//! - **The wait.** `verify` parks a `manual` check nobody decided (`store::park_manual_check`) and lists it on its judgement's `awaiting`;
//!   `settle` then holds the task in `needs_person` instead of passing it on the checks around it.
//! - **The decision** (`crew_decide_manual_check`). This is a person's act and one immediate transaction: the audit row first, then the
//!   check's row from `awaiting` to `passed` or `failed` (`store::decide_manual_check`). Only then is the person's `Gate` written, at the
//!   `checks` stage, `decided_by` a person, with the check's id as its subject and no Evidence of its own; the contract already allows a
//!   person's gate at every stage (`Gate::validate`) and nothing wrote one before this.
//! - **The resume.** When no other `manual` check of the task is waiting, the task moves `needs_person` to `verifying` (the one edge this
//!   slice adds to the lifecycle) and the verifier runs the key again in the task's own checkout, in what is left of its budget. The
//!   decided check now passes or fails from the person's answer, and the ordinary judgement applies: a pass ends the task `passed`, and a
//!   denial is the person's reason for the worker, through the loop guard every other failure goes through.
//!
//! Nothing here is reachable from a Crew tool: only the person's own route (`POST /api/crew/tasks/{id}/checks/{check}/decide`) calls it,
//! and that route is classed `Person` in `routes/classes.rs`, so a worker holding the service token gets `403 person_required`.
//!
//! An answer is the ids of the task's `manual` checks still waiting: empty means the task has carried on.

use orbyn_contracts::{
    CheckKind, CrewAuthor, CrewError, CrewHash, CrewId, CrewObject, CrewRole, CrewShortText,
    CrewToken, ErrorCode, Gate, GateDecider, GateDecision, GateStage,
};
use sha2::{Digest, Sha256};

use super::leases::{internal, timestamp};
use super::verify::Judgement;
use crate::app::Core;

/// The prefix that makes a check's subject hash unlike any other subject a gate names.
const SUBJECT_PREFIX: &str = "orbyn:crew:manual-check:";

fn invalid(message: impl Into<String>) -> CrewError {
    CrewError::new(ErrorCode::InvalidObject, message)
}

/// The subject a `manual` check's gate names: a hash of the check's id, so a gate and a check can be matched without the key's text.
pub(crate) fn check_subject(check_id: &str) -> Result<CrewHash, CrewError> {
    let digest = Sha256::digest(format!("{SUBJECT_PREFIX}{check_id}").as_bytes());
    CrewHash::new(format!("sha256:{}", crate::util::hex(&digest))).map_err(invalid)
}

/// A person's own author record: no member, and an engine token that says a person decided, never a model.
fn person_author() -> Result<CrewAuthor, CrewError> {
    Ok(CrewAuthor {
        role: CrewRole::Person,
        engine: CrewToken::new("person").map_err(invalid)?,
        member: None,
    })
}

impl Core {
    /// A person passes (`pass`) or denies one `manual` check of `task_id`, with an optional note. Writes the audit row and the check's
    /// state, then the person's gate, then resumes the task when nothing else waits. Returns the ids of the task's checks still waiting.
    pub async fn crew_decide_manual_check(
        self: &std::sync::Arc<Self>,
        task_id: &str,
        check_id: &str,
        pass: bool,
        note: Option<&str>,
        actor: &str,
    ) -> Result<Vec<String>, CrewError> {
        let task = self
            .store
            .crew_task(task_id)
            .map_err(internal)?
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task is not found"))?;
        if task.state != "needs_person" {
            return Err(invalid(
                "This task is not waiting for a person, so there is no check to decide.",
            ));
        }
        let key = self
            .store
            .crew_answer_key(&task.id)
            .map_err(internal)?
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no key"))?;
        let check = key
            .checks
            .iter()
            .find(|held| held.id.as_str() == check_id)
            .ok_or_else(|| {
                CrewError::new(ErrorCode::InvalidObject, "the task has no such check")
            })?;
        if check.kind != CheckKind::Manual {
            return Err(invalid(
                "Only a check a person makes can be decided this way; the core runs the others.",
            ));
        }
        let waiting = self
            .store
            .crew_manual_checks_waiting(&task.id)
            .map_err(internal)?;
        if !waiting.iter().any(|id| id == check_id) {
            return Err(invalid(
                "This check is not waiting for a person, so it cannot be decided now.",
            ));
        }
        let at = crate::util::unix_now();
        let changed = self
            .store
            .decide_manual_check(&task.id, check_id, pass, note, at)
            .map_err(internal)?;
        if changed {
            self.store_person_check_gate(&task, check_id, pass, note, at)?;
        }
        tracing::debug!(task = %task.id, check = check_id, actor, "a person decided a manual check");
        let waiting = self
            .store
            .crew_manual_checks_waiting(&task.id)
            .map_err(internal)?;
        if !waiting.is_empty() {
            return Ok(waiting);
        }
        self.resume_after_manual_checks(&task.id).await?;
        Ok(waiting)
    }

    /// The person's gate at the `checks` stage for one check: `pass` or `fail`, decided by a person, with no Evidence of its own (a
    /// person's gate needs none), and a reason a person would say. Written once per decision.
    fn store_person_check_gate(
        self: &std::sync::Arc<Self>,
        task: &crate::store::CrewTask,
        check_id: &str,
        pass: bool,
        note: Option<&str>,
        at: i64,
    ) -> Result<(), CrewError> {
        let key = self
            .store
            .crew_answer_key(&task.id)
            .map_err(internal)?
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no key"))?;
        let reason = note
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if pass {
                    "A person checked this, and it holds.".to_owned()
                } else {
                    "A person checked this, and it does not hold.".to_owned()
                }
            });
        let mut gate = Gate {
            v: 0,
            id: key.id.clone(),
            crew_id: key.crew_id.clone(),
            author: person_author()?,
            created_at: timestamp(at)?,
            run_id: CrewId::new(task.run_id.clone()).map_err(invalid)?,
            stage: GateStage::Checks,
            subject: check_subject(check_id)?,
            decision: if pass {
                GateDecision::Pass
            } else {
                GateDecision::Fail
            },
            decided_by: GateDecider::Person,
            evidence: Vec::new(),
            reason: CrewShortText::new(reason.chars().take(1000).collect::<String>())
                .map_err(invalid)?,
            lane: None,
        };
        gate.validate()?;
        gate.id = CrewObject::Gate(gate.clone()).computed_id()?;
        self.store
            .add_crew_object(&task.id, &CrewObject::Gate(gate))
            .map_err(internal)
    }

    /// Runs the key again after its `manual` checks are decided, from the task's own checkout, and applies the judgement. The task is
    /// `needs_person` when this is called and no check of it waits any more.
    async fn resume_after_manual_checks(
        self: &std::sync::Arc<Self>,
        task_id: &str,
    ) -> Result<(), CrewError> {
        let Some(task) = self.store.move_crew_task(task_id, "verifying", None).ok() else {
            return Err(invalid(
                "The task could not go back to its checks; it may have ended already.",
            ));
        };
        let run = self
            .store
            .crew_run(&task.id)
            .map_err(internal)?
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no run"))?;
        let judged: Judgement = self.crew_verify(&task, &run).await?;
        self.settle_crew_task(&task.id, &judged);
        Ok(())
    }
}
