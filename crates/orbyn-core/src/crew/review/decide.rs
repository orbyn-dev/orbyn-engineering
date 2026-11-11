//! The finding check: code decides whether a finding is confirmed, never a model.
//!
//! - With a `repro`, the lead's check states the correct behaviour and the core runs it in the checkout. If it
//!   fails, the finding is real: confirmed. If it passes, the claim does not hold: unconfirmed.
//! - Without one, an independent second review of the same lane runs (a different run, routed to another vendor
//!   than the first reviewer where one is configured, never shown the finding). The finding is confirmed only if
//!   that run reports the same file within a few lines. Where it does not, the finding is unconfirmed.
//!
//! Either way the decision is a Gate at stage `finding_check` stored with the reviewed task, citing Evidence the core
//! wrote, so it can be added after the task's run has ended (H7). An unconfirmed finding blocks nothing.

use std::sync::Arc;

use orbyn_contracts::{
    CheckKind, CheckOutcome, CrewCheck, CrewCommit, CrewError, CrewHash, CrewId, CrewObject,
    ErrorCode, Evidence, Finding, Gate, GateDecider, GateDecision, GateStage,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::app::Core;
use crate::crew::checks::{Site, run_check, tail};
use crate::crew::guard::{redact, refuse_secret};
use crate::crew::leases::{core_author, internal, timestamp};
use crate::crew::tool::invalid;
use crate::crew::verify::{head, short};
use crate::store::{CrewReview, CrewTask, FindingRepro, StoredFinding};
use crate::tools::mcp::McpBinding;

/// A repro is a quick check, not the task's whole budget.
const REPRO_TIME: std::time::Duration = std::time::Duration::from_secs(120);
/// The most findings a second run may file in the checked file. More is noise, and noise agrees with anything.
const MOST_IN_FILE: usize = 3;

/// How many lines apart two reviewers' findings in one file may be and still be the same place.
pub const SAME_PLACE: u32 = 3;

/// Whether `second` is the same finding as `first`: the same file, within [`SAME_PLACE`] lines. The claim is
/// text a model wrote and is not compared.
pub fn agrees(first: &Finding, second: &Finding) -> bool {
    first.path == second.path && first.line.abs_diff(second.line) <= SAME_PLACE
}

fn zero() -> Result<CrewHash, CrewError> {
    CrewHash::new(format!("sha256:{}", "0".repeat(64))).map_err(invalid)
}

fn sha(text: &str) -> Result<CrewHash, CrewError> {
    CrewHash::new(format!(
        "sha256:{}",
        crate::util::hex(&Sha256::digest(text.as_bytes()))
    ))
    .map_err(invalid)
}

/// What a piece of Evidence says.
pub(super) struct Said<'a> {
    pub check: &'a str,
    pub command: &'a str,
    pub outcome: CheckOutcome,
    pub exit_code: Option<u32>,
    pub duration_ms: u64,
    pub commit: CrewCommit,
    pub log: &'a str,
    pub proves: &'a str,
    pub does_not_prove: &'a str,
}

impl Core {
    /// Stores Evidence the core wrote under `task`, so it is part of that task's record.
    pub(super) fn write_core_evidence(
        &self,
        task: &CrewTask,
        said: &Said<'_>,
    ) -> Result<CrewHash, CrewError> {
        let run = CrewId::new(task.run_id.clone()).map_err(invalid)?;
        let mut evidence = Evidence {
            v: 0,
            id: zero()?,
            crew_id: CrewId::new(task.crew_id.clone()).map_err(invalid)?,
            author: core_author(),
            created_at: timestamp(crate::util::unix_now())?,
            run_id: run,
            check: CrewId::new(said.check).map_err(invalid)?,
            claim: None,
            command: short(said.command)?,
            outcome: said.outcome,
            exit_code: said.exit_code,
            duration_ms: said.duration_ms,
            commit: said.commit.clone(),
            log_hash: sha(said.log)?,
            excerpt: redact(&tail(said.log, 4000)),
            artifacts: Vec::new(),
            proves: short(said.proves)?,
            does_not_prove: short(said.does_not_prove)?,
        };
        evidence.validate()?;
        evidence.id = CrewObject::Evidence(evidence.clone()).computed_id()?;
        let object = CrewObject::Evidence(evidence.clone());
        refuse_secret(&object.to_value())?;
        self.store
            .add_crew_object(&task.id, &object)
            .map_err(internal)?;
        Ok(evidence.id)
    }

    /// Stores the `finding_check` Gate for `finding` under the reviewed task. Decided by code, citing `evidence`.
    pub(super) fn write_finding_gate(
        &self,
        subject: &CrewTask,
        finding: &Finding,
        decision: GateDecision,
        evidence: Vec<CrewHash>,
        reason: &str,
    ) -> Result<CrewHash, CrewError> {
        let mut gate = Gate {
            v: 0,
            id: zero()?,
            crew_id: CrewId::new(subject.crew_id.clone()).map_err(invalid)?,
            author: core_author(),
            created_at: timestamp(crate::util::unix_now())?,
            run_id: CrewId::new(subject.run_id.clone()).map_err(invalid)?,
            stage: GateStage::FindingCheck,
            subject: finding.id.clone(),
            decision,
            decided_by: GateDecider::Code,
            evidence,
            reason: short(reason)?,
            lane: None,
        };
        gate.validate()?;
        gate.id = CrewObject::Gate(gate.clone()).computed_id()?;
        let stored = self
            .store
            .add_finding_gate_once(&subject.id, &gate)
            .map_err(internal)?;
        if !stored {
            return Err(invalid("finding: it has already been checked"));
        }
        Ok(gate.id)
    }

    /// The engine that routed a review run, for the record.
    fn reviewer_engine(&self, task_id: &str) -> String {
        self.store
            .crew_route_decisions(task_id)
            .ok()
            .and_then(|d| d.last().map(|d| d.engine.as_str().to_owned()))
            .unwrap_or_else(|| "no engine".to_owned())
    }

    /// Whether the finding already has a decision, or a second run that has not ended.
    fn finding_is_taken(&self, subject: &CrewTask, finding: &Finding) -> Result<bool, CrewError> {
        let decided = self
            .store
            .crew_gates(&subject.id)
            .map_err(internal)?
            .iter()
            .any(|g| g.stage == GateStage::FindingCheck && g.subject == finding.id);
        if decided {
            return Ok(true);
        }
        for second in self
            .store
            .crew_second_runs(finding.id.as_str())
            .map_err(internal)?
        {
            let state = self.store.crew_task(&second.task_id).map_err(internal)?;
            if state
                .is_some_and(|t| !crate::crew::lifecycle::is_final(&t.state) || t.state == "passed")
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) async fn crew_check_finding_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let (thread, stamper) = self.review_lead(binding, "crew_check_finding")?;
        let Some(fields) = args.as_object() else {
            return Err(invalid("arguments: an object"));
        };
        if let Some(extra) = fields
            .keys()
            .find(|k| !["finding", "repro"].contains(&k.as_str()))
        {
            return Err(invalid(format!(
                "{extra}: not an argument of crew_check_finding"
            )));
        }
        refuse_secret(args)?;
        let id = fields
            .get("finding")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("finding: required"))?;
        let found = self
            .store
            .crew_finding(id)
            .map_err(internal)?
            .filter(|f| f.finding.crew_id.as_str() == thread.id)
            .ok_or_else(|| invalid(format!("{id}: no such finding in this crew")))?;
        let (subject, subject_run) = self.review_subject(&thread, &found.subject_id)?;
        // A finding from a review run that did not finish its read-only check is not one the core confirms.
        let filed_by = self.store.crew_task(&found.task_id).map_err(internal)?;
        if filed_by.is_none_or(|t| t.state != "passed") {
            return Err(invalid(
                "finding: its review run did not pass its read-only check, so it cannot be confirmed",
            ));
        }
        if self.finding_is_taken(&subject, &found.finding)? {
            return Err(invalid(
                "finding: it has already been checked, or a second review of it is running",
            ));
        }
        let Some(draft) = fields.get("repro").filter(|v| !v.is_null()) else {
            let started = self
                .start_review_run(
                    &thread,
                    &stamper,
                    &subject,
                    &orbyn_contracts::CrewToken::new(found.lane.clone()).map_err(invalid)?,
                    found.finding.commit.as_str(),
                    super::lanes::Run::Second { finding: id },
                )
                .await?;
            return Ok(json!({
                "finding": id,
                "decision": "pending",
                "note": "An independent second review is running. The core decides when it ends.",
                "second_review": started,
            }));
        };
        let check: CrewCheck =
            serde_json::from_value(draft.clone()).map_err(|e| invalid(format!("repro: {e}")))?;
        check.validate()?;
        if !matches!(
            check.kind,
            CheckKind::Command
                | CheckKind::Test
                | CheckKind::Lint
                | CheckKind::Typecheck
                | CheckKind::CustomScript
                | CheckKind::FileScope
                | CheckKind::DiffSize
        ) {
            return Err(invalid(
                "repro: this version runs command, test, lint, typecheck, custom_script, file_scope and diff_size checks",
            ));
        }
        let worktree = subject_run
            .worktree
            .clone()
            .unwrap_or_else(|| thread.cwd.clone());
        let site = Site {
            worktree: std::path::Path::new(&worktree),
            deadline: std::time::Instant::now() + REPRO_TIME,
            base: subject_run.base_commit.as_deref(),
        };
        let ran = run_check(&check, &site).await;
        let (decision, reason) = match ran.outcome {
            CheckOutcome::Fail => (
                GateDecision::Confirmed,
                "The repro check fails, so the finding holds.",
            ),
            CheckOutcome::Pass => (
                GateDecision::Unconfirmed,
                "The repro check passes, so the finding is not confirmed.",
            ),
            CheckOutcome::Error | CheckOutcome::Skipped => {
                return Err(invalid(format!(
                    "repro: it could not run, so it decides nothing ({})",
                    ran.reason.as_deref().unwrap_or("not run by this version")
                )));
            }
        };
        let commit = head(std::path::Path::new(&worktree))
            .await
            .unwrap_or(found.finding.commit.clone());
        let evidence = self.write_core_evidence(
            &subject,
            &Said {
                check: check.id.as_str(),
                command: &ran.command,
                outcome: ran.outcome,
                exit_code: ran.exit_code,
                duration_ms: ran.duration_ms,
                commit,
                log: &ran.log,
                proves: check.proves.as_str(),
                does_not_prove: "Anything the repro does not exercise.",
            },
        )?;
        let repro = serde_json::to_string(&check).map_err(|e| invalid(e.to_string()))?;
        self.store
            .add_finding_repro(&FindingRepro {
                finding_id: id.to_owned(),
                subject_id: subject.id.clone(),
                repro,
            })
            .map_err(|e| invalid(e.public_message()))?;
        let gate = self.write_finding_gate(
            &subject,
            &found.finding,
            decision,
            vec![evidence.clone()],
            reason,
        )?;
        let fix = if decision == GateDecision::Confirmed {
            Some(
                self.start_fix(&thread, &stamper, &found, &subject, Some(check))
                    .await?,
            )
        } else {
            None
        };
        Ok(json!({
            "finding": id,
            "decision": if decision == GateDecision::Confirmed { "confirmed" } else { "unconfirmed" },
            "gate": gate,
            "evidence": evidence,
            "fix": fix,
        }))
    }

    /// A second run ended: the finding it checks is confirmed if it reported the same place, else it is not.
    /// A run that did not pass decides nothing; the finding can be checked again.
    pub(super) async fn decide_second_run(self: &Arc<Self>, review: &CrewReview) {
        let Some(first_id) = review.check_of.as_deref() else {
            return;
        };
        if let Err(error) = self.decide_second_run_inner(review, first_id).await {
            tracing::warn!(%error, "could not decide a finding from its second review");
        }
    }

    async fn decide_second_run_inner(
        self: &Arc<Self>,
        review: &CrewReview,
        first_id: &str,
    ) -> Result<(), CrewError> {
        let second_task = self
            .store
            .crew_task(&review.task_id)
            .map_err(internal)?
            .ok_or_else(|| invalid("the second review is gone"))?;
        if second_task.state != "passed" {
            return Ok(());
        }
        let Some(first) = self.store.crew_finding(first_id).map_err(internal)? else {
            return Ok(());
        };
        let Some(subject) = self.store.crew_task(&first.subject_id).map_err(internal)? else {
            return Ok(());
        };
        let theirs: Vec<StoredFinding> = self
            .store
            .crew_findings_of_task(&review.task_id)
            .map_err(internal)?;
        let in_file = theirs
            .iter()
            .filter(|s| s.finding.path == first.finding.path)
            .count();
        let noisy = in_file > MOST_IN_FILE;
        let matched = theirs
            .iter()
            .filter(|s| agrees(&first.finding, &s.finding))
            .min_by_key(|s| s.finding.line.abs_diff(first.finding.line))
            .filter(|_| !noisy);
        let engines = format!(
            "first reviewer on {}, second on {}",
            self.reviewer_engine(&first.task_id),
            self.reviewer_engine(&review.task_id)
        );
        let (decision, reason, said) = match matched {
            Some(other) => (
                GateDecision::Confirmed,
                format!("An independent second review reported the same place ({engines})."),
                format!("{}\n{}\n", first.finding.id, other.finding.id),
            ),
            None => (
                GateDecision::Unconfirmed,
                if noisy {
                    format!(
                        "The second review filed {in_file} findings in that file, too many to count as agreement ({engines})."
                    )
                } else {
                    format!(
                        "An independent second review reported {} findings, none at the same place ({engines}).",
                        theirs.len()
                    )
                },
                format!("{}\nnone of {}\n", first.finding.id, theirs.len()),
            ),
        };
        let evidence = self.write_core_evidence(
            &subject,
            &Said {
                check: "finding-agreement",
                command: &format!("an independent second review of the same lane, compared by file and line ({engines})"),
                outcome: if matched.is_some() { CheckOutcome::Pass } else { CheckOutcome::Fail },
                exit_code: None,
                duration_ms: 0,
                commit: first.finding.commit.clone(),
                log: &said,
                proves: "Whether two independent reviews reported the same place.",
                does_not_prove: "That the claim is right; only that two reviewers saw the same place.",
            },
        )?;
        self.write_finding_gate(&subject, &first.finding, decision, vec![evidence], &reason)?;
        if decision == GateDecision::Confirmed {
            let lead = self
                .store
                .crew_run(&subject.id)
                .map_err(internal)?
                .map(|r| r.lead_thread_id)
                .ok_or_else(|| invalid("the task has no lead"))?;
            let thread = self
                .store
                .thread(&lead)
                .map_err(|e| invalid(e.public_message()))?;
            let stamper = self.review_stamper(&thread, &thread.engine)?;
            if let Err(error) = self
                .start_fix(&thread, &stamper, &first, &subject, None)
                .await
                && error.code != ErrorCode::NotAuthorized
            {
                tracing::warn!(%error, "could not send a fix brief");
            }
        }
        Ok(())
    }
}
