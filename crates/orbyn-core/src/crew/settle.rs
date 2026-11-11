//! What follows a verification: the task passes, or fails and the loop guard names what happens
//! next (SPEC.md section 6). The worker is told only the reason a check failed, through the
//! thread's queue, after the same leak and secret checks every object that reaches a worker
//! passes (H1, H2). A result goes to the lead when the run ends or needs a person.

use std::sync::Arc;

use orbyn_contracts::{
    CrewError, CrewHash, CrewId, CrewObject, CrewResult, CrewShortText, ErrorCode, ResultState,
    ResultUsage, RunState, StopReason,
};
use serde_json::json;

use super::guard::{key_needles, refuse_leak, refuse_secret};
use super::leases::{core_author, internal, timestamp};
use super::lifecycle::Next;
use super::verify::Judgement;
use crate::app::Core;
use crate::store::{CrewRun, CrewTask};

/// The most of failure text one prompt carries.
const REASON_CHARS: usize = 2500;

fn short(text: &str) -> CrewShortText {
    let cut: String = text.chars().take(1000).collect();
    CrewShortText::new(if cut.trim().is_empty() {
        "(empty)".to_owned()
    } else {
        cut
    })
    .expect("1 to 1000 characters")
}

fn stop_name(reason: StopReason) -> &'static str {
    match reason {
        StopReason::RepeatedFailure => "repeated_failure",
        StopReason::Budget => "budget",
        StopReason::TooManyDenials => "too_many_denials",
        StopReason::InjectionSuspected => "injection_suspected",
        StopReason::EngineError => "engine_error",
        StopReason::LeadStopped => "lead_stopped",
    }
}

/// The lead's copy of a result: data, with `&` and `<` escaped so it cannot close its wrapper.
fn lead_message(result: &CrewResult) -> String {
    let body = serde_json::to_string(&CrewObject::Result(result.clone()).to_value())
        .unwrap_or_default()
        .replace('&', "&amp;")
        .replace('<', "&lt;");
    format!(
        "<crew-result id=\"{}\" brief=\"{}\" state=\"{}\" final=\"{}\" trust=\"data\">\n{body}\n</crew-result>",
        result.id,
        result.brief,
        super::wire_name(&result.state),
        result.r#final
    )
}

fn retry_prompt(reason: &str) -> String {
    format!(
        "Verification failed. Fix this and call crew_report again.\n\n{reason}\n\nThe core runs \
         the checks; you do not see them. Do not widen your changes beyond the paths you were given."
    )
}

fn change_prompt(reason: &str) -> String {
    format!(
        "The same failure happened twice, so repeating the same approach will not help.\n\n{reason}\n\n\
         Take a different approach, then call crew_report again. This is the one change of \
         approach you get: if it fails the same way again, the task stops."
    )
}

impl Core {
    /// Builds the result of the task as it stands, stores it and returns it.
    fn record_result(
        &self,
        task: &CrewTask,
        state: ResultState,
        stop: Option<StopReason>,
        summary: &str,
        judged: Option<&Judgement>,
    ) -> Result<CrewResult, CrewError> {
        let run = self
            .store
            .crew_run(&task.id)
            .map_err(internal)?
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no run"))?;
        let report = self
            .store
            .crew_reports(&task.id)
            .map_err(internal)?
            .into_iter()
            .next_back();
        let route = self
            .store
            .crew_route_decisions(&task.id)
            .map_err(internal)?
            .into_iter()
            .next_back()
            .map(|d| d.id);
        let invalid = |why: String| CrewError::new(ErrorCode::InvalidObject, why);
        let created = crate::util::unix_now();
        let minutes = crate::util::parse_time(&task.created_at).map_or(0, |at| {
            u64::try_from((created - at).max(0) / 60).unwrap_or(0)
        });
        let mut not_verified: Vec<CrewShortText> = judged
            .map(|j| j.not_verified.iter().map(|s| short(s)).collect())
            .unwrap_or_default();
        if let Some(report) = &report {
            not_verified.extend(report.not_verified.iter().cloned());
        }
        not_verified.truncate(64);
        let mut result = CrewResult {
            v: 0,
            id: CrewHash::new(format!("sha256:{}", "0".repeat(64))).map_err(invalid)?,
            crew_id: CrewId::new(task.crew_id.clone()).map_err(invalid)?,
            author: core_author(),
            created_at: timestamp(created)?,
            run_id: CrewId::new(task.run_id.clone()).map_err(invalid)?,
            brief: CrewHash::new(task.id.clone()).map_err(invalid)?,
            state,
            r#final: state.is_final(),
            stop_reason: stop,
            summary: summary.chars().take(2000).collect(),
            branch: report.as_ref().and_then(|r| r.branch.clone()),
            commit: report
                .as_ref()
                .and_then(|r| r.commit.clone())
                .or_else(|| judged.map(|j| j.commit.clone())),
            checks: judged.map(|j| j.checks.clone()).unwrap_or_default(),
            not_verified,
            attempts: u64::from(run.attempts),
            usage: ResultUsage {
                turns: None,
                minutes: Some(minutes),
                tokens_in: None,
                tokens_out: None,
                cost_micros: None,
            },
            route,
            report: report.map(|r| r.id),
        };
        result.validate()?;
        result.id = CrewObject::Result(result.clone()).computed_id()?;
        self.store
            .add_crew_object(&task.id, &CrewObject::Result(result.clone()))
            .map_err(internal)?;
        Ok(result)
    }

    /// Tells the lead, through its queue, that the run ended or needs a person.
    fn tell_lead(&self, run: &CrewRun, result: &CrewResult) {
        if !result.r#final && result.state != ResultState::NeedsPerson {
            return;
        }
        if let Err(error) = self.runs.enqueue(
            &run.lead_thread_id,
            &format!(
                "{}{}",
                lead_message(result),
                self.take_held_for_lead(run, result.crew_id.as_str())
            ),
            &[],
        ) {
            tracing::warn!(%error, "could not tell the lead a crew task's result");
        }
    }

    /// The task ends: its leases are released, the worker's run is stopped, a final result is
    /// written and the lead is told. Safe to call on a task already moved to its final state.
    pub(super) fn end_crew_task(
        &self,
        task: &CrewTask,
        state: ResultState,
        stop: Option<StopReason>,
        summary: &str,
        judged: Option<&Judgement>,
    ) {
        if let Err(error) = self.release_crew_leases(&task.id) {
            tracing::warn!(%error, "could not release a crew task's leases");
        }
        // A `crew_ask` waiting for this run's member stops waiting (H7).
        self.crew_bus.wake_all();
        let run = self.store.crew_run(&task.id).ok().flatten();
        if let Some(run) = &run
            && let Some(active) = run
                .worker_thread_id
                .as_deref()
                .and_then(|thread| self.store.thread(thread).ok())
                .and_then(|thread| thread.active_run)
        {
            let _ = self.runs.cancel(&active.id);
        }
        match self.record_result(task, state, stop, summary, judged) {
            Ok(result) => {
                if let Some(run) = &run {
                    self.tell_lead(run, &result);
                }
            }
            Err(error) => tracing::warn!(%error, "could not write a crew task's result"),
        }
    }

    /// Moves a task one step and swallows an invalid move: someone else (a cancel) got there.
    fn step_crew_task(&self, id: &str, to: &str, stop: Option<&str>) -> Option<CrewTask> {
        match self.store.move_crew_task(id, to, stop) {
            Ok(task) => Some(task),
            Err(error) => {
                tracing::debug!(%error, task = id, to, "a crew task did not move");
                None
            }
        }
    }

    /// The task waits for a person. `tell` queues the result for the lead; a caller that is
    /// answering the lead in the same call passes false.
    pub(super) fn crew_needs_person(&self, task_id: &str, why: &str, tell: bool) {
        if let Some(task) = self.step_crew_task(task_id, "needs_person", None) {
            let run = self.store.crew_run(task_id).ok().flatten();
            match self.record_result(&task, ResultState::NeedsPerson, None, why, None) {
                Ok(result) => {
                    if let (true, Some(run)) = (tell, &run) {
                        self.tell_lead(run, &result);
                    }
                }
                Err(error) => tracing::warn!(%error, "could not write a crew task's result"),
            }
        }
    }

    /// Applies a judgement to a task that is `verifying`.
    pub(super) fn settle_crew_task(self: &Arc<Self>, task_id: &str, judged: &Judgement) {
        let Ok(Some(task)) = self.store.crew_task(task_id) else {
            return;
        };
        if judged.passed() {
            let ids: Vec<String> = judged
                .evidence
                .iter()
                .map(|id| id.as_str().to_owned())
                .collect();
            if let Ok(task) = self.store.pass_crew_task(task_id, &ids) {
                let summary = format!("{} checks passed.", judged.passes);
                self.end_crew_task(&task, ResultState::Passed, None, &summary, Some(judged));
            }
        } else if judged.undecided() {
            self.crew_needs_person(
                task_id,
                "No check this version can run decided the task, so a person decides.",
                true,
            );
        } else {
            let reasons: Vec<String> = judged
                .failures
                .iter()
                .enumerate()
                .map(|(n, f)| format!("Check {} failed: {}", n + 1, f.reason))
                .collect();
            self.settle_failure(
                &task,
                &reasons.join("\n\n"),
                &judged.fingerprint(),
                Some(judged),
            );
        }
    }

    /// A failed verification, or an attempt that never reported. The task is `verifying`.
    pub(super) fn settle_failure(
        self: &Arc<Self>,
        task: &CrewTask,
        reason: &str,
        fingerprint: &str,
        judged: Option<&Judgement>,
    ) {
        let reason: String = reason.chars().take(REASON_CHARS).collect();
        if self.over_budget(task) {
            if let Some(task) = self.step_crew_task(&task.id, "stopped", Some("budget")) {
                self.end_crew_task(
                    &task,
                    ResultState::Stopped,
                    Some(StopReason::Budget),
                    "The task ran past its time budget.",
                    judged,
                );
            }
            return;
        }
        let (moved, next) = match self.store.fail_crew_verification(&task.id, fingerprint) {
            Ok(done) => done,
            Err(error) => {
                tracing::warn!(%error, "could not apply the loop guard");
                return;
            }
        };
        match next {
            Next::Stop => self.end_crew_task(
                &moved,
                ResultState::Stopped,
                Some(StopReason::RepeatedFailure),
                "The same failure happened again after the one change of approach, so the task stopped.",
                judged,
            ),
            Next::Retry | Next::ChangeApproach => {
                let summary = format!("Verification failed; the worker retries. {reason}");
                if let Err(error) = self.record_result(&moved, ResultState::Failed, None, &summary, judged) {
                    tracing::warn!(%error, "could not write a crew task's result");
                }
                let prompt = if next == Next::ChangeApproach {
                    change_prompt(&reason)
                } else {
                    retry_prompt(&reason)
                };
                // The worker runs again before it is told, so its next report finds the task
                // `running` however fast the engine answers.
                let resumed = next == Next::Retry
                    || self.step_crew_task(&moved.id, "running", None).is_some();
                if resumed
                    && self.send_to_worker(&moved, &prompt).is_ok()
                {
                    return;
                }
                if let Some(task) =
                    self.step_crew_task(&moved.id, "stopped", Some("engine_error"))
                {
                    self.end_crew_task(
                        &task,
                        ResultState::Stopped,
                        Some(StopReason::EngineError),
                        "The worker could not be told what to fix.",
                        judged,
                    );
                }
            }
        }
    }

    fn over_budget(&self, task: &CrewTask) -> bool {
        let Ok(Some(brief)) = self.store.crew_brief(&task.id) else {
            return false;
        };
        crate::util::parse_time(&task.created_at)
            .is_some_and(|at| crate::util::unix_now() - at > i64::from(brief.budget.minutes) * 60)
    }

    /// Queues `text` on the worker's thread. It is checked for the key's text and for secrets
    /// first (H1, H2); a prompt that fails either is replaced by a plain one.
    fn send_to_worker(&self, task: &CrewTask, text: &str) -> Result<(), CrewError> {
        let run = self
            .store
            .crew_run(&task.id)
            .map_err(internal)?
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no run"))?;
        let thread = run
            .worker_thread_id
            .clone()
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no worker"))?;
        let needles = match self.store.crew_answer_key(&task.id).map_err(internal)? {
            Some(key) => key_needles(&key),
            None => Vec::new(),
        };
        let value = json!({ "prompt": text });
        let text = if refuse_leak(&value, &needles).is_err() || refuse_secret(&value).is_err() {
            "Verification failed. The details are withheld; fix your work and call crew_report again."
        } else {
            text
        };
        // Messages from other members that arrived while the worker was not in a turn go with
        // this prompt, each already checked the way a live one is.
        let held = self.take_held_messages(task, &run);
        let text = format!("{text}{held}");
        self.runs
            .enqueue(&thread, &text, &[])
            .map(|_| ())
            .map_err(internal)
    }

    /// A worker's run ended. A report made in it is verified now, and not before: the worker
    /// cannot change files while the checks run. If it never reported, that is a failed
    /// attempt; if the run itself failed or was cancelled, the task ends.
    pub(crate) async fn crew_worker_run_ended(self: Arc<Self>, thread_id: String, run_id: String) {
        let Ok(Some((task, run))) = self.store.crew_task_for_worker(&thread_id) else {
            return;
        };
        let Ok(ended) = self.store.run(&run_id) else {
            return;
        };
        let reported = run.last_report_run.as_deref() == Some(run_id.as_str());
        if task.state == "verifying" && reported {
            match ended.state {
                RunState::Completed => self.verify_crew_task(&task.id).await,
                _ => self.end_for_run(&task.id, ended.state),
            }
            return;
        }
        // Only a task still waiting on this run's report is affected.
        if task.state != "running" || reported {
            return;
        }
        if ended.state != RunState::Completed {
            self.end_for_run(&task.id, ended.state);
            return;
        }
        if self
            .store
            .begin_crew_verification(&task.id, &run_id, None)
            .is_err()
        {
            return;
        }
        let Ok(Some(task)) = self.store.crew_task(&task.id) else {
            return;
        };
        let reason = "You ended your turn without calling crew_report. Report your \
                      result with crew_report when you finish.";
        self.settle_failure(&task, reason, "no_report", None);
    }

    /// The worker's run was cancelled or failed: the task ends with it.
    pub(super) fn end_for_run(&self, task_id: &str, state: RunState) {
        if state == RunState::Cancelled {
            if let Some(task) = self.step_crew_task(task_id, "cancelled", None) {
                self.end_crew_task(
                    &task,
                    ResultState::Cancelled,
                    None,
                    "The worker's run was cancelled.",
                    None,
                );
            }
        } else if let Some(task) =
            self.step_crew_task(task_id, "stopped", Some(stop_name(StopReason::EngineError)))
        {
            self.end_crew_task(
                &task,
                ResultState::Stopped,
                Some(StopReason::EngineError),
                "The worker's engine stopped with an error.",
                None,
            );
        }
    }

    /// The task's time budget ended while it was running: it stops, and the worker's run with
    /// it. A task already past `running` or `verifying`, or ended, is left alone.
    pub(crate) fn crew_budget_ended(&self, task_id: &str) {
        if let Some(task) = self.step_crew_task(task_id, "stopped", Some("budget")) {
            self.end_crew_task(
                &task,
                ResultState::Stopped,
                Some(StopReason::Budget),
                "The task ran past its time budget.",
                None,
            );
        }
    }
}
