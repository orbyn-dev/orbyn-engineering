use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::events::{TurnMapper, parse_goal};
use super::protocol::{answer_request, is_missing_thread, read_app};
use super::sandbox::{
    CONNECTOR_NOTE, approval_policy, collaboration_mode, sandbox_policy, thread_config, tier_param,
};
use super::{CALL_TIMEOUT, INTERRUPT_GRACE, TOOLS_LIMIT};
use crate::agents::driver::pipes::{EngineProcess, exit_message};
use crate::agents::driver::rpc::{Inbound, RpcClient};
use crate::agents::driver::{EngineEvent, EngineOutcome, GoalAction, RunKind, RunSpec};
use crate::error::{CoreError, Result};

pub(super) async fn run_protocol(
    mut process: EngineProcess,
    spec: RunSpec,
    sandbox: String,
    events: mpsc::Sender<EngineEvent>,
    mut steering: mpsc::UnboundedReceiver<String>,
    cancel: CancellationToken,
) {
    let (rpc, mut inbound) = RpcClient::start(process.stdin, process.stdout, false);
    let opened = tokio::select! {
        biased;
        () = cancel.cancelled() => Err(CoreError::RunClosed),
        result = open_turn(&rpc, &spec, &sandbox, &events) => result,
    };
    let (session, mut turn_id) = match opened {
        Ok(ids) => ids,
        Err(error) => {
            let outcome = if cancel.is_cancelled() {
                EngineOutcome::Cancelled
            } else if matches!(error, CoreError::SessionLost) {
                EngineOutcome::SessionLost
            } else {
                let status = tokio::time::timeout(EXIT_WAIT, process.child.wait()).await;
                let exited = status.ok().and_then(Result::ok);
                match exited {
                    Some(status) => {
                        EngineOutcome::Failed(exit_message(Some(status), &process.stderr).await)
                    }
                    None => EngineOutcome::Failed(error.to_string()),
                }
            };
            let _ = events.send(EngineEvent::Finished(outcome)).await;
            return;
        }
    };
    if let Some(turn) = &turn_id {
        let _ = events.send(EngineEvent::Turn(turn.clone())).await;
    }
    let mut mapper = TurnMapper::new(session.clone());
    let mut interrupted_at: Option<tokio::time::Instant> = None;
    let outcome = loop {
        let deadline = interrupted_at.map(|at| at + INTERRUPT_GRACE);
        tokio::select! {
            biased;
            () = cancel.cancelled(), if interrupted_at.is_none() => {
                interrupted_at = Some(tokio::time::Instant::now());
                if let Some(turn) = &turn_id {
                    let rpc = rpc.clone();
                    let params = json!({ "threadId": session, "turnId": turn });
                    tokio::spawn(async move {
                        let _ = rpc.call("turn/interrupt", params, INTERRUPT_GRACE).await;
                    });
                }
            }
            () = sleep_until(deadline), if deadline.is_some() => break EngineOutcome::Cancelled,
            Some(text) = steering.recv() => {
                if let Some(turn) = &turn_id {
                    let rpc = rpc.clone();
                    let params = json!({
                        "threadId": session,
                        "expectedTurnId": turn,
                        "input": [{ "type": "text", "text": text, "text_elements": [] }],
                    });
                    let refused = events.clone();
                    tokio::spawn(async move {
                        if let Err(error) = rpc.call("turn/steer", params, CALL_TIMEOUT).await {
                            tracing::debug!(%error, "could not steer the run");
                            let _ = refused.send(EngineEvent::SteerFailed(text)).await;
                        }
                    });
                }
            }
            message = inbound.recv() => {
                let Some(message) = message else {
                    if interrupted_at.is_some() {
                        break EngineOutcome::Cancelled;
                    }
                    let status = tokio::time::timeout(EXIT_WAIT, process.child.wait()).await;
                    let status = status.ok().and_then(Result::ok);
                    break EngineOutcome::Failed(exit_message(status, &process.stderr).await);
                };
                match message {
                    Inbound::Notification { method, params } => {
                        if method == "turn/started"
                            && turn_id.is_none()
                            && let Some(id) = params.pointer("/turn/id").and_then(Value::as_str)
                        {
                            turn_id = Some(id.to_owned());
                            if events.send(EngineEvent::Turn(id.to_owned())).await.is_err() {
                                return;
                            }
                        }
                        let step = mapper.on_notification(&method, &params);
                        for event in step.events {
                            if events.send(event).await.is_err() {
                                return;
                            }
                        }
                        if method == "item/completed"
                            && let Some(app) = read_app(&rpc, &session, &params).await
                            && events.send(EngineEvent::McpApp(app)).await.is_err()
                        {
                            return;
                        }
                        if let Some(outcome) = step.outcome {
                            break outcome;
                        }
                    }
                    Inbound::Request { id, method, params } => {
                        answer_request(&rpc, &events, &mapper, id, method, params);
                    }
                }
            }
        }
    };
    for event in mapper.close_open() {
        let _ = events.send(event).await;
    }
    let outcome = if cancel.is_cancelled() {
        EngineOutcome::Cancelled
    } else {
        outcome
    };
    let _ = events.send(EngineEvent::Finished(outcome)).await;
}

const EXIT_WAIT: Duration = Duration::from_secs(2);

async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Runs the handshake and starts the turn. Returns the Codex thread id and the
/// turn id when the response has already carried it.
async fn open_turn(
    rpc: &RpcClient,
    spec: &RunSpec,
    sandbox: &str,
    events: &mpsc::Sender<EngineEvent>,
) -> Result<(String, Option<String>)> {
    rpc.call(
        "initialize",
        json!({
            "clientInfo": {
                "name": "orbyn",
                "title": "Orbyn Engineering",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": { "experimentalApi": true },
        }),
        CALL_TIMEOUT,
    )
    .await?;
    rpc.notify("initialized", None).await?;
    let config = thread_config(spec);
    // Said on every resume too: a saved session keeps the policy it last had, so
    // leaving full access has to be stated to take effect.
    // At the library, the model often answers that it has no file tools. Starting
    // here loads the checkout's `.codex` config, which folder trust already gates.
    let mut params = json!({
        "cwd": spec.checkout,
        "approvalPolicy": approval_policy(spec.mode),
        "sandbox": if spec.plan { "read-only" } else { sandbox },
        "config": config,
    });
    // The note stands without a context too: the connectors are there either way.
    params["developerInstructions"] = json!(if spec.context.is_empty() {
        CONNECTOR_NOTE.to_owned()
    } else {
        format!("{}\n{CONNECTOR_NOTE}", spec.context)
    });
    if let Some(model) = &spec.model {
        params["model"] = json!(model);
    }
    if let Some(tier) = tier_param(spec) {
        params["serviceTier"] = tier;
    }
    // The same thread takes a new model: a resume carries it, and the history stays.
    let thread = match &spec.resume {
        Some(session) => {
            params["threadId"] = json!(session);
            match rpc.call("thread/resume", params, CALL_TIMEOUT).await {
                Ok(thread) => thread,
                Err(error) if is_missing_thread(&error.to_string()) => {
                    return Err(CoreError::SessionLost);
                }
                Err(error) => return Err(error),
            }
        }
        None => rpc.call("thread/start", params, CALL_TIMEOUT).await?,
    };
    let session = thread
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or_else(|| CoreError::EngineUnavailable("Codex returned no thread id.".into()))?
        .to_owned();
    let _ = events.send(EngineEvent::Session(session.clone())).await;
    match &spec.kind {
        RunKind::Compact => {
            rpc.call(
                "thread/compact/start",
                json!({ "threadId": session }),
                CALL_TIMEOUT,
            )
            .await?;
            return Ok((session, None));
        }
        RunKind::Goal(action) => {
            let (method, params) = match action {
                GoalAction::Set(objective) => (
                    "thread/goal/set",
                    json!({ "threadId": session, "objective": objective, "status": "active" }),
                ),
                GoalAction::Clear => ("thread/goal/clear", json!({ "threadId": session })),
                GoalAction::Pause => (
                    "thread/goal/set",
                    json!({ "threadId": session, "status": "paused" }),
                ),
                GoalAction::Resume => (
                    "thread/goal/set",
                    json!({ "threadId": session, "status": "active" }),
                ),
            };
            let reply = rpc.call(method, params, CALL_TIMEOUT).await?;
            // The reply carries the goal; a notification could arrive after the
            // run has already stopped listening.
            let goal = reply.get("goal").filter(|goal| !goal.is_null());
            let _ = events.send(EngineEvent::Goal(goal.map(parse_goal))).await;
            if matches!(action, GoalAction::Clear | GoalAction::Pause) {
                let _ = events
                    .send(EngineEvent::Finished(EngineOutcome::Completed))
                    .await;
                return Ok((session, None));
            }
        }
        RunKind::Turn => {}
    }
    // Older versions lack this method; a run goes ahead either way.
    let _ = rpc
        .call("mcpServerStatus/list", json!({}), TOOLS_LIMIT)
        .await;
    let text = match &spec.kind {
        RunKind::Goal(GoalAction::Resume) => "Continue working toward the goal.",
        // In the user's message, the context often led the model to answer that it
        // had no tools. It goes in the thread's instructions instead.
        _ => spec.user_prompt.as_str(),
    };
    let policy = if spec.plan { "read-only" } else { sandbox };
    let mut params = json!({
        "threadId": session,
        "input": [{ "type": "text", "text": text, "text_elements": [] }],
        "cwd": spec.checkout,
        "sandboxPolicy": sandbox_policy(policy, &spec.checkout, spec.mode),
        "approvalPolicy": approval_policy(spec.mode),
        "model": spec.model,
        "effort": spec.effort,
        "collaborationMode": collaboration_mode(spec, &thread)?,
    });
    if let Some(tier) = tier_param(spec) {
        params["serviceTier"] = tier;
    }
    let turn = rpc.call("turn/start", params, CALL_TIMEOUT).await?;
    let turn_id = turn
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok((session, turn_id))
}
