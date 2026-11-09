use orbyn_contracts::ActionKind;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::METHOD_NOT_FOUND;
use crate::agents::driver::rpc::RpcClient;
use crate::agents::driver::{EngineEvent, EngineOutcome, ask_paths};
use crate::governance::approvals::Verdict;

pub(super) fn stop_outcome(reply: &Value) -> EngineOutcome {
    match reply.get("stopReason").and_then(Value::as_str) {
        Some("end_turn") => EngineOutcome::Completed,
        Some("cancelled") => EngineOutcome::Cancelled,
        Some(reason) => EngineOutcome::Failed(format!("The agent stopped: {reason}.")),
        None => EngineOutcome::Completed,
    }
}

pub(super) fn answer_request(
    rpc: &RpcClient,
    events: &mpsc::Sender<EngineEvent>,
    cancel: &CancellationToken,
    id: Value,
    method: String,
    params: Value,
) {
    let rpc = rpc.clone();
    if method != "session/request_permission" {
        tokio::spawn(async move {
            let text = format!("Orbyn does not serve {method}.");
            let _ = rpc.respond_error(id, METHOD_NOT_FOUND, &text).await;
        });
        return;
    }
    let events = events.clone();
    let cancel = cancel.clone();
    tokio::spawn(async move {
        let (kind, target, summary) = classify(&params);
        let paths = if kind == ActionKind::FileChange {
            vec![target.clone()]
        } else {
            Vec::new()
        };
        let verdict = ask_paths(&events, kind, target, summary, params.clone(), paths).await;
        let outcome = permission_outcome(&params, &verdict, cancel.is_cancelled());
        let _ = rpc.respond(id, json!({ "outcome": outcome })).await;
    });
}

/// Picks the option that matches the verdict. Fails closed: with no matching
/// option the request is reported cancelled, never allowed.
pub(super) fn permission_outcome(params: &Value, verdict: &Verdict, cancelled: bool) -> Value {
    if cancelled {
        return json!({ "outcome": "cancelled" });
    }
    let wanted: &[&str] = if verdict.is_allowed() {
        &["allow_once", "allow_always"]
    } else {
        &["reject_once", "reject_always"]
    };
    let options = params
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    wanted
        .iter()
        .find_map(|kind| {
            options
                .iter()
                .find(|option| option.get("kind").and_then(Value::as_str) == Some(kind))
        })
        .and_then(|option| option.get("optionId").and_then(Value::as_str))
        .map_or_else(
            || json!({ "outcome": "cancelled" }),
            |id| json!({ "outcome": "selected", "optionId": id }),
        )
}

pub(super) fn classify(params: &Value) -> (ActionKind, String, String) {
    let call = params.get("toolCall").cloned().unwrap_or(Value::Null);
    let title = call
        .get("title")
        .or_else(|| params.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("Agent action")
        .to_owned();
    let path = call
        .pointer("/locations/0/path")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let command = call
        .pointer("/rawInput/command")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match call.get("kind").and_then(Value::as_str) {
        Some("execute") => (
            ActionKind::Command,
            command.unwrap_or_else(|| title.clone()),
            title,
        ),
        Some("edit" | "delete" | "move") => (
            ActionKind::FileChange,
            path.unwrap_or_else(|| title.clone()),
            title,
        ),
        Some("fetch" | "search") => (ActionKind::Network, title.clone(), title),
        _ => (ActionKind::Tool, title.clone(), title),
    }
}
