use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::events::StreamMapper;
use super::workflows::Workflows;
use super::{EXIT_GRACE, HOLD_GRACE};
use crate::agents::driver::pipes::{EngineProcess, exit_message, lines};
use crate::agents::driver::{EngineEvent, EngineOutcome, RunControl, RunKind, RunSpec};

/// What a run takes while it works: steering text and controls.
pub(super) struct Inputs {
    pub(super) steering: mpsc::UnboundedReceiver<String>,
    pub(super) controls: mpsc::UnboundedReceiver<RunControl>,
}

async fn sleep_until(due: Option<tokio::time::Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}

pub(super) async fn run_protocol(
    mut process: EngineProcess,
    spec: RunSpec,
    events: mpsc::Sender<EngineEvent>,
    inputs: Inputs,
    cancel: CancellationToken,
) {
    let Inputs {
        mut steering,
        mut controls,
    } = inputs;
    // The prompt goes over stdin, never argv: it can exceed ARG_MAX and argv is
    // visible to other processes.
    let uuid = prompt_uuid();
    let prompt = user_message(&spec, &uuid);
    let wrote = async {
        process.stdin.write_all(prompt.as_bytes()).await?;
        process.stdin.write_all(b"\n").await?;
        process.stdin.flush().await
    }
    .await;
    if let Err(error) = wrote {
        tracing::debug!(%error, "could not write the prompt");
    }
    let mut stdin = process.stdin;
    // A slash command or skill expands into messages with uuids of their own, so no echo of ours
    // is promised; only a plain prompt is told apart from leftovers this way.
    let plain = matches!(spec.kind, RunKind::Turn) && !spec.prompt.trim_start().starts_with('/');
    let mut mapper = if plain {
        StreamMapper::answering(&uuid)
    } else {
        StreamMapper::default()
    };
    let mut stream = lines(process.stdout);
    let mut outcome = None;
    let mut grace = None;
    let stream_ended = loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => break false,
            () = sleep_until(grace) => {
                outcome = mapper.held.take();
                break false;
            }
            Some(control) = controls.recv() => {
                let RunControl::StopWorkflow { tool_call_id } = control;
                if let Some(frame) = mapper.workflows.stop_request(&tool_call_id) {
                    if let Err(error) = stdin.write_all(format!("{frame}\n").as_bytes()).await {
                        tracing::debug!(%error, "could not ask Claude to stop a workflow");
                    }
                    let _ = stdin.flush().await;
                } else if events.send(Workflows::already_ended(&tool_call_id)).await.is_err() {
                    return;
                }
            }
            Some(text) = steering.recv() => {
                // The CLI folds a message sent mid-turn into the running turn.
                let frame = format!("{}\n", user_frame(&text));
                if let Err(error) = stdin.write_all(frame.as_bytes()).await {
                    tracing::debug!(%error, "could not steer the run");
                }
                let _ = stdin.flush().await;
            }
            line = stream.next() => {
                let Some(line) = line else { break true };
                let Ok(line) = line else { continue };
                let Ok(frame) = serde_json::from_str::<Value>(&line) else {
                    tracing::debug!("skipping a non-JSON Claude line");
                    continue;
                };
                for event in mapper.on_frame(&frame) {
                    if events.send(event).await.is_err() {
                        return;
                    }
                }
                if let Some(result) = mapper.outcome.take() {
                    outcome = Some(result);
                    break false;
                }
                if mapper.waiting_on_hold() {
                    grace.get_or_insert_with(|| tokio::time::Instant::now() + HOLD_GRACE);
                } else {
                    grace = None;
                }
            }
        }
    };
    // The turn finished before the CLI went away; a workflow it held for is gone.
    let outcome = outcome.or_else(|| mapper.held.take());
    // Closing stdin is the CLI's signal to exit.
    drop(stdin);
    for event in mapper.close_open_blocks() {
        let _ = events.send(event).await;
    }
    for event in mapper.workflows.end_all() {
        let _ = events.send(event).await;
    }
    for event in mapper.end_background() {
        let _ = events.send(event).await;
    }
    let cancelled = cancel.is_cancelled();
    let status = match tokio::time::timeout(EXIT_GRACE, process.child.wait()).await {
        Ok(status) => status.ok(),
        Err(_) => {
            process.child.terminate();
            None
        }
    };
    let outcome = match (outcome, cancelled, stream_ended) {
        (_, true, _) => EngineOutcome::Cancelled,
        (Some(result), _, _) => result,
        _ => {
            let message = exit_message(status, &process.stderr).await;
            if message.contains(NO_SESSION) {
                EngineOutcome::SessionLost
            } else {
                EngineOutcome::Failed(message)
            }
        }
    };
    let _ = events.send(EngineEvent::Finished(outcome)).await;
}

/// How the CLI reports a `--resume` id it does not have.
pub(super) const NO_SESSION: &str = "No conversation found";

/// Slash commands reach the CLI bare, or it would read them as ordinary text.
fn user_message(spec: &RunSpec, uuid: &str) -> String {
    let text = match spec.kind {
        RunKind::Turn => &spec.prompt,
        _ => &spec.user_prompt,
    };
    json!({ "type": "user", "uuid": uuid, "message": { "role": "user", "content": text } })
        .to_string()
}

/// The CLI asks for a UUID, and echoes it back with `--replay-user-messages` when it takes the
/// prompt up.
fn prompt_uuid() -> String {
    let n = ulid::Ulid::generate().0;
    // Version 4, variant 1: the shape the CLI validates.
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (n >> 96) as u32,
        (n >> 80) as u16,
        (n >> 68) as u16 & 0x0fff,
        0x8000 | ((n >> 52) as u16 & 0x3fff),
        n & 0xffff_ffff_ffff
    )
}

fn user_frame(text: &str) -> String {
    json!({ "type": "user", "message": { "role": "user", "content": text } }).to_string()
}
