//! One run over a connection: it starts the turn, turns the host's frames into engine events,
//! carries cancel, steer and stop-task to the host, and ends with exactly one `Finished`.
//! Slice 1 answers every `permission` and `question` with a refusal; the seam for the approval
//! gate and the question desk is `refuse`.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::connection::{ClosedReason, Connection, RunFrame, RunRoute};
use super::delta::DeltaMapper;
use super::frames::AskFrame;
use super::outcome::{host_stopped, map};
use crate::agents::driver::{EngineEvent, EngineOutcome, RunControl, SteerMessage};

/// What the host is told when a permission or question has no handler yet.
const REFUSED: &str = "Orbyn has not enabled approvals for this engine path yet.";

pub(super) struct Run {
    pub connection: Arc<Connection>,
    pub route: RunRoute,
    pub send_id: u64,
    pub session: String,
    pub mapper: DeltaMapper,
    pub events: mpsc::Sender<EngineEvent>,
    pub cancel: CancellationToken,
    pub steering: mpsc::UnboundedReceiver<SteerMessage>,
    pub controls: mpsc::UnboundedReceiver<RunControl>,
    pub cancel_grace: std::time::Duration,
    /// What the `send` response already said.
    pub accepted: Value,
}

async fn sleep_until_opt(due: Option<tokio::time::Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}

/// Answers a frame that asks for something with a refusal. A late or repeated answer is
/// ignored by the host.
fn refuse(connection: &Connection, ask: &AskFrame) {
    connection.fire(
        "respond",
        json!({ "requestId": ask.request_id, "decision": { "behavior": "deny", "message": REFUSED } }),
    );
}

impl Run {
    /// The resumable session and turn the host named when it accepted the turn.
    fn accepted_events(&self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        if let Some(id) = self.accepted.get("engineSessionId").and_then(Value::as_str) {
            events.push(EngineEvent::Session(id.to_owned()));
        }
        if let Some(id) = self.accepted.get("turnId").and_then(Value::as_str) {
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

    pub(super) async fn drive(mut self) {
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
                    self.connection.fire("cancel", json!({ "sendId": self.send_id }));
                    grace = Some(tokio::time::Instant::now() + self.cancel_grace);
                }
                () = sleep_until_opt(grace), if grace.is_some() => break EngineOutcome::Cancelled,
                Some(steer) = self.steering.recv() => self.steer(steer),
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
                        for event in self.mapper.map(&update) {
                            if self.events.send(event).await.is_err() {
                                self.abandon();
                                return;
                            }
                        }
                    }
                    Some(RunFrame::Permission(ask) | RunFrame::Question(ask)) => {
                        refuse(&self.connection, &ask);
                    }
                    Some(RunFrame::Session { engine_session_id, turn_id }) => {
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
                        let (outcome, extra) = map(&done);
                        for event in extra {
                            let _ = self.events.send(event).await;
                        }
                        break outcome;
                    }
                },
            }
        };
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
            ClosedReason::Shutdown => EngineOutcome::Cancelled,
            ClosedReason::Crash(_) | ClosedReason::Idle => host_stopped(),
        }
    }
}
