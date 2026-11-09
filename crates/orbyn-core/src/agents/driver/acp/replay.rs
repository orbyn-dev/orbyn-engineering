use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{CALL_TIMEOUT, METHOD_NOT_FOUND};
use crate::agents::driver::rpc::{Inbound, RpcClient};
use crate::error::{CoreError, Result};

/// Loading replays history before its response. Consume it while the call is
/// outstanding, then start a fresh mapper for the prompt; replay is not a turn.
pub(super) async fn resume_without_replay(
    rpc: &RpcClient,
    inbound: &mut mpsc::UnboundedReceiver<Inbound>,
    method: &str,
    params: Value,
    session: &str,
) -> Result<Value> {
    tokio::time::timeout(CALL_TIMEOUT, async {
        let resuming = method == "session/resume";
        let call = rpc.call_detailed(method, params, CALL_TIMEOUT);
        tokio::pin!(call);
        let mut bytes = 0usize;
        let mut count = 0usize;
        loop {
            tokio::select! {
                biased;
                message = inbound.recv() => {
                    let Some(message) = message else {
                        return finish_restore((&mut call).await.map_err(restore_error)?, bytes, count);
                    };
                    count += 1;
                    bytes = bytes.checked_add(replay_message_size(&message)?).ok_or_else(replay_limit)?;
                    if count > super::sessions::MAX_MESSAGES || bytes > super::sessions::MAX_BYTES {
                        return Err(replay_limit());
                    }
                    discard_replay_message(rpc, message, session, resuming).await?;
                }
                result = &mut call => return finish_restore(result.map_err(restore_error)?, bytes, count),
            }
        }
    })
    .await
    .map_err(|_| CoreError::EngineUnavailable("The ACP restore operation timed out.".into()))?
}

fn replay_limit() -> CoreError {
    CoreError::EngineUnavailable("The ACP replay exceeded its limit.".into())
}

/// Count the full retained envelope without allocating a second potentially
/// large JSON string. Legacy live RPC framing still admits one 64MiB line.
fn replay_message_size(message: &Inbound) -> Result<usize> {
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        jsonrpc: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<&'a Value>,
        method: &'a str,
        params: &'a Value,
    }
    let envelope = match message {
        Inbound::Request { id, method, params } => Envelope {
            jsonrpc: "2.0",
            id: Some(id),
            method,
            params,
        },
        Inbound::Notification { method, params } => Envelope {
            jsonrpc: "2.0",
            id: None,
            method,
            params,
        },
    };
    bounded_json_size(&envelope)
}

fn finish_restore(value: Value, bytes: usize, count: usize) -> Result<Value> {
    if count >= super::sessions::MAX_MESSAGES
        || bytes
            .checked_add(bounded_json_size(&value)?)
            .is_none_or(|size| size > super::sessions::MAX_BYTES)
    {
        return Err(replay_limit());
    }
    Ok(value)
}

fn bounded_json_size(value: &impl serde::Serialize) -> Result<usize> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|size| *size <= super::sessions::MAX_BYTES)
                .ok_or_else(|| std::io::Error::other("ACP replay limit"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, value).map_err(|_| replay_limit())?;
    Ok(count.0)
}

async fn discard_replay_message(
    rpc: &RpcClient,
    message: Inbound,
    session: &str,
    resuming: bool,
) -> Result<()> {
    match message {
        Inbound::Request { id, method, .. } => {
            if method == "session/request_permission" {
                rpc.respond(id, json!({"outcome":{"outcome":"cancelled"}}))
                    .await?;
            } else {
                rpc.respond_error(
                    id,
                    METHOD_NOT_FOUND,
                    "Client methods are unavailable while restoring history.",
                )
                .await?;
            }
        }
        Inbound::Notification { method, params } if method == "session/update" => {
            if params.get("sessionId").and_then(Value::as_str) != Some(session) {
                return Err(CoreError::EngineUnavailable(
                    "The agent replayed another session.".into(),
                ));
            }
            if resuming && method_is_transcript(params.get("update").unwrap_or(&Value::Null)) {
                return Err(CoreError::EngineUnavailable(
                    "The agent replayed history while resuming.".into(),
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn restore_error(error: crate::agents::driver::rpc::RpcCallError) -> CoreError {
    if error.code == Some(-32002) {
        CoreError::SessionLost
    } else {
        error.error
    }
}

fn method_is_transcript(update: &Value) -> bool {
    matches!(
        update.get("sessionUpdate").and_then(Value::as_str),
        Some(
            "user_message_chunk"
                | "agent_message_chunk"
                | "agent_thought_chunk"
                | "tool_call"
                | "tool_call_update"
                | "plan"
        )
    )
}
