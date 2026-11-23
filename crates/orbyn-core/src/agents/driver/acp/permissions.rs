use orbyn_contracts::ActionKind;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::METHOD_NOT_FOUND;
use crate::agents::driver::rpc::RpcClient;
use crate::agents::driver::{EngineEvent, EngineOutcome, ask_paths};
use crate::governance::approvals::Verdict;

/// How the turn that answered our prompt ended. `stopReason` is required by the
/// protocol, so a reply without a known one is not proof of success: the run fails
/// plainly instead of looking finished. A reply with no output at all is
/// the run pump's to report ("No reply"), never hidden here, unless it also used
/// no tokens: see `stop_outcome_after`.
pub(super) fn stop_outcome(reply: &Value) -> EngineOutcome {
    match reply.get("stopReason").and_then(Value::as_str) {
        Some("end_turn") => EngineOutcome::Completed,
        Some("cancelled") => EngineOutcome::Cancelled,
        Some("max_tokens") => stopped("it ran out of tokens"),
        Some("max_turn_requests") => stopped("it reached its limit of model requests for one turn"),
        Some("refusal") => stopped("it refused to continue"),
        Some(reason) => stopped(&format!("it gave the reason \"{reason}\"")),
        None => stopped("it did not say why"),
    }
}

/// `stop_outcome`, told how many events the agent sent during the turn and which program it
/// is. An `end_turn` that sent nothing and reports no `usage` never ran: OpenCode 1.15.10
/// answers that way when a newer OpenCode already migrated its database and its own prompt
/// handler fails. Asking the person to send again cannot help, so name the program and
/// version. A turn that ran reports its tokens even when it said nothing, and stays the run
/// pump's "No reply".
pub(super) fn stop_outcome_after(reply: &Value, sent: usize, agent: &str) -> EngineOutcome {
    match stop_outcome(reply) {
        EngineOutcome::Completed if sent == 0 && reply.get("usage").is_none() => {
            EngineOutcome::Failed(format!(
                "{agent} ended the turn without running it. Check its log or update it."
            ))
        }
        outcome => outcome,
    }
}

fn stopped(why: &str) -> EngineOutcome {
    EngineOutcome::Failed(format!("The agent stopped before finishing: {why}."))
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
        let paths = gate_paths(&params, kind);
        let verdict = if kind == ActionKind::FileChange && paths.is_empty() {
            Verdict::Deny(
                "Orbyn declined this change: the agent did not say which file it touches."
                    .to_owned(),
            )
        } else {
            ask_paths(&events, kind, target, summary, params.clone(), paths).await
        };
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

/// Every file a file-changing request names, so the gate scope-checks all of them.
/// Empty for any other kind, and for a file change that names none: that one is
/// declined by the caller, never gated as a path derived from its title.
pub(super) fn gate_paths(params: &Value, kind: ActionKind) -> Vec<String> {
    if kind != ActionKind::FileChange {
        return Vec::new();
    }
    params
        .pointer("/toolCall/locations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|location| location.get("path").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
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
