//! One run over a connection: it starts the turn, turns the host's frames into engine events,
//! carries cancel, steer and stop-task to the host, and ends with exactly one `Finished`.
//! A `permission` becomes a permission event for the approval gate and a `question` a question
//! event for the desk; the answer goes back as `respond`. Policy is the core's, the host only waits.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::StreamExt;
use futures_util::future::BoxFuture;
use futures_util::stream::FuturesUnordered;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::connection::{ClosedReason, Connection, RunFrame, RunRoute};
use super::delta::DeltaMapper;
use super::frames::{AskFrame, ErrorKind, Reason};
use super::outcome::{host_stopped, map_family};
use super::pool::Pool;
use crate::agents::driver::{
    EngineEvent, EngineOutcome, PermissionRequest, QuestionAsk, RunControl, SteerMessage,
};
use crate::conversation::questions::Answers;
use crate::governance::approvals::Verdict;
use crate::tools::mcp::{OWN_TOOL_PREFIX, classify_tool, parse_questions};

/// An ask the core is deciding: the request id it answers and the `respond` decision it becomes.
/// Dropping the set drops every hold, so a verdict that comes later finds nobody to tell.
type Waiting = FuturesUnordered<BoxFuture<'static, (String, Value)>>;

pub(super) struct Run {
    pub pool: Arc<Pool>,
    pub connection: Arc<Connection>,
    pub route: RunRoute,
    pub send_id: u64,
    pub session: String,
    /// The engine's resumable session, when the `open` reply already named it.
    pub open_session: Option<String>,
    /// The family whose frame shapes this run reads (`acp`, `claude`, ...).
    pub family: String,
    /// The thread and engine this run is for, so the pool can name its live session.
    pub thread: String,
    pub engine: String,
    pub mapper: DeltaMapper,
    pub events: mpsc::Sender<EngineEvent>,
    pub cancel: CancellationToken,
    pub steering: mpsc::UnboundedReceiver<SteerMessage>,
    pub controls: mpsc::UnboundedReceiver<RunControl>,
    pub cancel_grace: std::time::Duration,
    /// What the `send` response already said.
    pub accepted: Value,
    /// The id the prompt went in under; the engine's frames name it when its turn is queued and starts.
    pub message_id: String,
    /// ACP names no turn ids. Until the host accepts the prompt, a delta is not this
    /// prompt's answer; it is shown as a leftover, never counted.
    pub prompt_accepted: bool,
}

impl Drop for Run {
    fn drop(&mut self) {
        self.pool
            .forget_session(&self.engine, &self.thread, self.session_id());
    }
}

/// Where the prompt's turn is, from the engine's own frames about this run's `messageId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Turn {
    /// The engine said nothing about the prompt, so everything it says counts.
    Unknown,
    /// The prompt is queued behind another turn: what arrives now is that turn's.
    Queued,
    /// The prompt's turn began.
    Own,
}

impl Turn {
    /// Reads a `delta` for what it says about the prompt. Returns whether it was only bookkeeping.
    fn note(&mut self, update: &Value, message_id: &str) -> bool {
        let named = |key: &str| update.get(key).and_then(Value::as_str) == Some(message_id);
        match update.get("type").and_then(Value::as_str) {
            Some("command_lifecycle") => {
                if named("command_uuid") {
                    match update.get("state").and_then(Value::as_str) {
                        Some("queued") if *self == Self::Unknown => *self = Self::Queued,
                        Some("started") => *self = Self::Own,
                        _ => {}
                    }
                }
                true
            }
            // The engine taking the prompt up echoes it.
            Some("user") if named("uuid") => {
                *self = Self::Own;
                false
            }
            _ => false,
        }
    }
}

async fn sleep_until_opt(due: Option<tokio::time::Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}

/// What the engine is told for a verdict: the behaviour and, for a denial, the gate's own words.
/// An allow carries nothing else, so the host can only grant it once.
fn decision(verdict: Verdict) -> Value {
    match verdict {
        Verdict::Allow => json!({ "behavior": "allow" }),
        Verdict::Deny(message) => json!({ "behavior": "deny", "message": message }),
    }
}

/// The person's answers, keyed by question text with several selections joined by a comma;
/// a skipped question is a denial.
fn answered(answers: Option<Answers>) -> Value {
    match answers {
        Some(answers) => {
            let joined: serde_json::Map<String, Value> = answers
                .into_iter()
                .map(|(question, values)| (question, json!(values.join(", "))))
                .collect();
            json!({ "answers": joined })
        }
        None => json!({ "behavior": "deny" }),
    }
}

impl Run {
    /// The resumable session the host named, from the `send` reply first and the `open`
    /// reply after it. ACP names its session at setup, before the prompt is sent.
    fn session_id(&self) -> Option<&str> {
        self.accepted
            .get("engineSessionId")
            .and_then(Value::as_str)
            .or(self.open_session.as_deref())
    }

    /// The resumable session and turn the host named when it accepted the turn. ACP
    /// names its session in the `open` reply, before the prompt is sent.
    fn accepted_events(&self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        let mut session = self.open_session.as_deref();
        if let Some(id) = self.accepted.get("engineSessionId").and_then(Value::as_str) {
            session = Some(id);
        }
        if let Some(id) = session {
            events.push(EngineEvent::Session(id.to_owned()));
        }
        if self.family == "claude" {
            // The Claude host's turn id is its own counter (`turn-<sendId>`), not an id in
            // the session file. The prompt's message id is what the file records, so that
            // is the point a native rewind names and stores.
            events.push(EngineEvent::Turn(self.message_id.clone()));
        } else if let Some(id) = self.accepted.get("turnId").and_then(Value::as_str) {
            events.push(EngineEvent::Turn(id.to_owned()));
        }
        events
    }

    fn steer(&self, steer: SteerMessage) {
        let connection = self.connection.clone();
        let events = self.events.clone();
        let send_id = self.send_id;
        tokio::spawn(async move {
            let answer = connection
                .request(
                    "steer",
                    json!({ "sendId": send_id, "messageId": steer.message_id, "text": steer.prompt }),
                )
                .await;
            let accepted = answer
                .ok()
                .and_then(|value| {
                    value
                        .get("outcome")
                        .and_then(Value::as_str)
                        .map(|outcome| outcome != "rejected")
                })
                .unwrap_or(false);
            if !accepted {
                let _ = events.send(EngineEvent::SteerFailed(steer)).await;
            }
        });
    }

    /// Answers a frame at once, without asking anyone.
    fn respond(&self, request_id: &str, decision: Value) {
        self.connection.fire(
            "respond",
            json!({ "requestId": request_id, "decision": decision }),
        );
    }

    /// Hands a `permission` to the approval gate. Orbyn's own tools govern themselves and are
    /// allowed as they are on the CLI path. Returns false when nobody reads this run any more.
    async fn permission(&self, ask: AskFrame, waiting: &mut Waiting) -> bool {
        if ask.tool.starts_with(OWN_TOOL_PREFIX) {
            self.respond(&ask.request_id, decision(Verdict::Allow));
            return true;
        }
        let action = if self.family == "acp" {
            crate::agents::driver::acp::permission_action(&ask.input)
        } else {
            classify_tool(&ask.tool, &ask.input)
        };
        let mut detail = action.detail;
        // The engine's choices are part of what an approval binds (option kinds), so they travel.
        if let (Some(options), Some(detail)) = (ask.options, detail.as_object_mut()) {
            detail.insert("options".into(), options);
        }
        let (reply, verdict) = oneshot::channel();
        let request = PermissionRequest {
            kind: action.kind,
            target: action.target,
            summary: action.summary,
            detail,
            paths: action.paths,
            reply,
        };
        if self
            .events
            .send(EngineEvent::Permission(request))
            .await
            .is_err()
        {
            return false;
        }
        let id = ask.request_id;
        waiting.push(Box::pin(async move {
            let verdict = verdict
                .await
                .unwrap_or_else(|_| Verdict::Deny("The run has ended.".to_owned()));
            (id, decision(verdict))
        }));
        true
    }

    /// Hands a `question` to the desk. One with nothing to show is refused, never asked.
    async fn question(&self, ask: AskFrame, waiting: &mut Waiting) -> bool {
        let questions = parse_questions(&ask.input);
        if questions.is_empty() {
            self.respond(&ask.request_id, json!({ "behavior": "deny" }));
            return true;
        }
        let (reply, answers) = oneshot::channel();
        if self
            .events
            .send(EngineEvent::Question(QuestionAsk { questions, reply }))
            .await
            .is_err()
        {
            return false;
        }
        let id = ask.request_id;
        waiting.push(Box::pin(async move {
            (id, answered(answers.await.ok().flatten()))
        }));
        true
    }

    pub(super) async fn drive(mut self) {
        let mut waiting = Waiting::new();
        let mut turn = Turn::Unknown;
        let mut tasks: HashMap<String, String> = HashMap::new();
        let mut cancel_sent = false;
        let mut grace: Option<tokio::time::Instant> = None;
        for event in self.accepted_events() {
            if self.events.send(event).await.is_err() {
                self.abandon();
                return;
            }
        }
        let outcome = loop {
            tokio::select! {
                biased;
                () = self.cancel.cancelled(), if !cancel_sent => {
                    cancel_sent = true;
                    // The host refuses what it holds itself; an answer after this is late.
                    waiting.clear();
                    self.connection.fire("cancel", json!({ "sendId": self.send_id }));
                    grace = Some(tokio::time::Instant::now() + self.cancel_grace);
                }
                () = sleep_until_opt(grace), if grace.is_some() => break EngineOutcome::Cancelled,
                Some(steer) = self.steering.recv() => self.steer(steer),
                Some((request_id, decision)) = waiting.next(), if !waiting.is_empty() => {
                    self.respond(&request_id, decision);
                }
                Some(RunControl::StopWorkflow { tool_call_id }) = self.controls.recv() => {
                    if let Some(task) = tasks.get(&tool_call_id) {
                        self.connection.fire(
                            "stopTask",
                            json!({ "sessionId": self.session, "taskId": task }),
                        );
                    }
                }
                frame = self.route.frames.recv() => match frame {
                    None => break self.after_close().await,
                    Some(RunFrame::Delta(update)) => {
                        if self.family != "acp" && turn.note(&update, &self.message_id) {
                            // Lifecycle bookkeeping makes no event.
                            continue;
                        }
                        // Output between the prompt being queued and its turn starting is a leftover turn's:
                        // shown, never counted as the answer (docs/engine-run-contract.md, rule 3).
                        // ACP names no turn ids, so nothing counts until the prompt is accepted.
                        let leftover = turn == Turn::Queued
                            || (self.family == "acp" && !self.prompt_accepted);
                        for event in self.mapper.map(&update) {
                            let event = if leftover {
                                EngineEvent::Leftover(Box::new(event))
                            } else {
                                event
                            };
                            if self.events.send(event).await.is_err() {
                                self.abandon();
                                return;
                            }
                        }
                    }
                    Some(RunFrame::Permission(ask)) => {
                        if !self.permission(ask, &mut waiting).await {
                            self.abandon();
                            return;
                        }
                    }
                    Some(RunFrame::Question(ask)) => {
                        if !self.question(ask, &mut waiting).await {
                            self.abandon();
                            return;
                        }
                    }
                    Some(RunFrame::Accepted) => {
                        self.prompt_accepted = true;
                    }
                    Some(RunFrame::Session { engine_session_id, turn_id }) => {
                        // A changed session clears the run's stored turn; for Claude the
                        // prompt's message id is still the point inside the (new) session,
                        // and the host validates it against that file before resuming.
                        let turn_id = if self.family == "claude" {
                            Some(self.message_id.clone())
                        } else {
                            turn_id
                        };
                        let events = engine_session_id
                            .map(EngineEvent::Session)
                            .into_iter()
                            .chain(turn_id.map(EngineEvent::Turn));
                        for event in events {
                            if self.events.send(event).await.is_err() {
                                self.abandon();
                                return;
                            }
                        }
                    }
                    Some(RunFrame::Background(event)) => {
                        if event.get("type").and_then(Value::as_str) == Some("started")
                            && let (Some(call), Some(id)) = (
                                event.get("toolCallId").and_then(Value::as_str),
                                event.get("id").and_then(Value::as_str),
                            )
                        {
                            tasks.insert(call.to_owned(), id.to_owned());
                        }
                    }
                    Some(RunFrame::Done(done)) => {
                        // A run the person cancelled is not a failure to show, whatever the engine called its end.
                        let failed = !cancel_sent
                            && (!done.ok
                                || matches!(done.reason, Some(Reason::Failed | Reason::MaxTurns | Reason::Refused)));
                        if failed {
                            let kind = done
                                .error
                                .as_ref()
                                .map(super::frames::HostError::effective_kind)
                                .filter(|kind| *kind != ErrorKind::Unknown);
                            self.pool.note_run(kind);
                        } else if !cancel_sent && done.reason == Some(Reason::Completed) {
                            self.pool.note_run(None);
                        }
                        let (outcome, extra) = map_family(&self.family, &done);
                        for event in extra {
                            let _ = self.events.send(event).await;
                        }
                        break outcome;
                    }
                },
            }
        };
        // Whatever is still open is unanswerable now: the host's callbacks end with the run or its crash,
        // and a verdict that comes later is dropped, never sent.
        drop(waiting);
        // A run the person cancelled ends as cancelled, whatever the host's last word was.
        let outcome = if cancel_sent && !matches!(outcome, EngineOutcome::SessionLost) {
            EngineOutcome::Cancelled
        } else {
            outcome
        };
        for event in self.mapper.finish() {
            let _ = self.events.send(event).await;
        }
        self.connection
            .fire("closeSession", json!({ "sessionId": self.session }));
        let _ = self.events.send(EngineEvent::Finished(outcome)).await;
    }

    /// The receiver of events is gone: nobody reads this run, so the host stops it.
    fn abandon(&self) {
        self.connection
            .fire("cancel", json!({ "sendId": self.send_id }));
        self.connection
            .fire("closeSession", json!({ "sessionId": self.session }));
    }

    /// The route ended without a `runDone`: the host is gone.
    async fn after_close(&self) -> EngineOutcome {
        match self.connection.closed().await {
            // The app is exiting or the person stopped the host: nothing here is a failure to retry.
            ClosedReason::Shutdown | ClosedReason::Stopped => EngineOutcome::Cancelled,
            ClosedReason::Crash(_) | ClosedReason::Idle => host_stopped(),
        }
    }
}
