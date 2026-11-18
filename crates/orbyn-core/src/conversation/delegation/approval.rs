//! Admission for a short approval run. Unlike ordinary delegation this never
//! queues work, resumes a session, registers MCP, or reports a transcript to a parent.

use std::sync::Arc;

use orbyn_contracts::{AgUiEvent, RunState, Thread};
use tokio::sync::mpsc;

use crate::agents::driver::{ApprovalSpec, EngineEvent, EngineOutcome, RunHandle};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{Admission, NewThread, Store};

const REPLY_BYTES: usize = 4096;

/// Owns both the durable admission and the driver task, including when the
/// requesting gate future is dropped. No detached approval can survive its caller.
struct Running {
    store: Arc<Store>,
    run: Option<String>,
    task: String,
    crew: String,
    approval: String,
    usage_known: bool,
    handle: Option<RunHandle>,
    closed: bool,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            handle.cancel.cancel();
            handle.task.abort();
        }
        if !self.closed
            && let Some(run) = &self.run
        {
            let _ = self.store.finish_run(run, RunState::Cancelled, None);
        }
        let _ = self.store.add_crew_autopilot_event(
            &self.task,
            &self.crew,
            "approval_ended",
            &serde_json::json!({ "approval": self.approval, "usage_known": self.usage_known }),
        );
    }
}

impl Core {
    /// The selection and parent are snapshots of this request, never reread from
    /// a mutable lead while the provider answers. `live` checks the originating
    /// task, cancellation intent, run identity and current permission ceiling.
    pub(crate) async fn delegated_approval(
        &self,
        parent: &Thread,
        engine: &str,
        mut spec: ApprovalSpec,
        task_id: &str,
        attempt: u32,
        live: impl Fn() -> bool,
    ) -> Result<String> {
        let unavailable =
            || CoreError::EngineUnavailable("The approval could not complete.".into());
        if !self.config.crew_preview || !live() || spec.deadline <= std::time::Instant::now() {
            return Err(CoreError::RunClosed);
        }
        let _operation = self.engines.begin_native_operation(engine)?;
        let driver = self.engines.driver(engine)?;
        if !driver.supports_approval() || driver.family() != spec.family {
            return Err(CoreError::Unsupported(
                "This engine cannot run a bounded tool-free approval.".into(),
            ));
        }
        if !self.store.project(&parent.project_id)?.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        self.engines.check_enabled(engine)?;
        driver.check_launch_available()?;
        let off = self.preference("disabled_engines", Some(&parent.project_id))?;
        if off
            .as_array()
            .is_some_and(|off| off.iter().any(|id| id.as_str() == Some(engine)))
        {
            return Err(unavailable());
        }
        let status = self.engines.status(engine).await?;
        if !driver.supports_approval_version(status.version.as_deref()) {
            return Err(unavailable());
        }
        spec.engine_version.clone_from(&status.version);
        let view = crate::crew::EngineView::from_status(&status, false).ok_or_else(unavailable)?;
        if !view.worth_reading()
            || self.cached_limits(engine).is_some_and(|limits| {
                limits
                    .windows
                    .iter()
                    .any(|window| window.used_percent >= 100.0)
            })
        {
            return Err(unavailable());
        }
        if !live() || spec.deadline <= std::time::Instant::now() {
            return Err(CoreError::RunClosed);
        }
        self.engines.check_selection(
            engine,
            &orbyn_contracts::ModelSelection {
                model: Some(spec.model.clone()),
                effort: spec.effort.clone(),
                options: spec.options.clone(),
            },
        )?;
        let approval = crate::util::new_id();
        self.store
            .reserve_crew_approval(task_id, attempt, &approval, engine)?;
        let task = self.store.crew_task(task_id)?.ok_or(CoreError::RunClosed)?;
        let mut running = Running {
            store: self.store.clone(),
            run: None,
            task: task_id.to_owned(),
            crew: task.crew_id,
            approval: approval.clone(),
            usage_known: false,
            handle: None,
            closed: false,
        };
        let child = self.store.create_registered_thread(
            &approval,
            NewThread {
                project_id: parent.project_id.clone(),
                title: "Autopilot approval".into(),
                engine: engine.to_owned(),
                model: Some(spec.model.clone()),
                cwd: parent.cwd.clone(),
                worktree: None,
                skill_profile: None,
                parent_thread_id: Some(parent.id.clone()),
                relation: Some("subagent".into()),
                scratch: parent.scratch,
            },
            None,
        )?;
        let run = self.store.admit_run(Admission {
            thread_id: &child.id,
            prompt: &spec.prompt,
            actor: "crew:approval",
        })?;
        running.run = Some(run.id.clone());
        self.store.mark_starting(&run.id)?;
        spec.run_id.clone_from(&run.id);
        spec.thread_id.clone_from(&child.id);
        let (send, mut receive) = mpsc::channel(32);
        let deadline = tokio::time::Instant::from_std(spec.deadline);
        let max_reported_tokens = spec.max_reported_tokens;
        let spend_stop = spec.estimated_spend_stop_usd;
        let answer = {
            let work = async {
                running.handle = Some(driver.start_approval(spec, send).await?);
                self.store.mark_running(&run.id)?;
                let mut text = String::new();
                let mut message = None;
                let mut ended = false;
                while let Some(event) = receive.recv().await {
                    if (!live() || !self.store.crew_approval_live(&run.id)?)
                        && !matches!(&event, EngineEvent::Usage(_))
                    {
                        return Err(CoreError::RunClosed);
                    }
                    match event {
                        EngineEvent::Agui(AgUiEvent::TextMessageStart { message_id, role })
                            if message.is_none() && role == "assistant" =>
                        {
                            message = Some(message_id);
                        }
                        EngineEvent::Agui(AgUiEvent::TextMessageContent { message_id, delta })
                            if !ended && message.as_ref() == Some(&message_id) =>
                        {
                            if text.len().saturating_add(delta.len()) > REPLY_BYTES {
                                return Err(unavailable());
                            }
                            text.push_str(&delta);
                        }
                        EngineEvent::Agui(AgUiEvent::TextMessageEnd { message_id })
                            if !ended && message.as_ref() == Some(&message_id) =>
                        {
                            ended = true
                        }
                        EngineEvent::Usage(usage) => {
                            let cost = usage
                                .raw
                                .get("estimated_cost_usd")
                                .and_then(serde_json::Value::as_f64)
                                .filter(|cost| cost.is_finite() && *cost >= 0.0);
                            self.store.add_crew_autopilot_event(task_id, &running.crew, "approval_usage", &serde_json::json!({ "approval": approval, "total_tokens": usage.total_tokens, "input_tokens": usage.raw.get("input_tokens").and_then(serde_json::Value::as_u64), "output_tokens": usage.raw.get("output_tokens").and_then(serde_json::Value::as_u64), "estimated_cost_usd": cost }))?;
                            running.usage_known = usage.total_tokens.is_some() && cost.is_some();
                            self.store.record_usage(&run.id, &usage)?;
                            if !usage
                                .total_tokens
                                .is_some_and(|tokens| tokens <= max_reported_tokens)
                                || cost.is_none_or(|cost| cost > spend_stop)
                            {
                                return Err(unavailable());
                            }
                        }
                        EngineEvent::Finished(EngineOutcome::Completed)
                            if ended && !text.is_empty() && running.usage_known =>
                        {
                            return Ok(text);
                        }
                        // Dropping Permission/Question replies denies them. Any tool,
                        // session, malformed stream, refusal or budget excess fails closed.
                        _ => return Err(unavailable()),
                    }
                }
                Err(unavailable())
            };
            tokio::pin!(work);
            let mut check = tokio::time::interval(std::time::Duration::from_millis(100));
            check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    biased;
                    () = tokio::time::sleep_until(deadline) => break Err(unavailable()),
                    _ = check.tick() => if !live() || !self.store.crew_approval_live(&run.id)? { break Err(CoreError::RunClosed); },
                    answer = &mut work => break answer,
                }
            }
        };
        // No successful late answer survives task cancellation or a mode change.
        let answer = answer.and_then(|text| {
            if live() {
                Ok(text)
            } else {
                Err(CoreError::RunClosed)
            }
        });
        if let Some(handle) = running.handle.take() {
            handle.cancel.cancel();
            handle.task.abort();
            let _ = handle.task.await;
        }
        let state = if answer.is_ok() {
            RunState::Completed
        } else {
            RunState::Failed
        };
        let (finished, _) = self.store.finish_run(&run.id, state, None)?;
        running.closed = true;
        if finished.state != state {
            return Err(CoreError::RunClosed);
        }
        answer
    }
}
