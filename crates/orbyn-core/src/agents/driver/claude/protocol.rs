use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::Stream;
use serde_json::{Value, json};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_util::codec::LinesCodecError;
use tokio_util::sync::CancellationToken;

use super::events::StreamMapper;
use super::workflows::Workflows;
use super::{EXIT_GRACE, HOLD_GRACE, HOLD_IDLE, HOLD_LIMIT};
use crate::agents::driver::pipes::{EngineProcess, exit_message, lines};
use crate::agents::driver::{
    EngineEvent, EngineOutcome, RunControl, RunKind, RunSpec, SteerMessage,
};

/// What a run takes while it works: steering messages and controls.
pub(super) struct Inputs {
    pub(super) steering: mpsc::UnboundedReceiver<SteerMessage>,
    pub(super) controls: mpsc::UnboundedReceiver<RunControl>,
}

async fn sleep_until(due: Option<tokio::time::Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}

/// The caps on a hold: the CLI keeps a completed turn open for background work that says when
/// it ends. Work that never notifies must not keep the run open: a quiet spell ends it after
/// the idle cap, and the total cap bounds it whatever the traffic.
#[derive(Default)]
struct Held {
    /// When the run's total cap runs out. Set by the run's first hold and never moved: background
    /// work that keeps alternating between holding and not holding must not win a fresh cap each
    /// time, or the run would stay open past it.
    limit_at: Option<tokio::time::Instant>,
    /// When this hold's quiet spell runs out. Dropped while the turn is not held, so a run between
    /// holds is not ended by a stale cap, and moved on by every line that says the work is still
    /// there, so a hold that keeps talking is not ended by it.
    idle_at: Option<tokio::time::Instant>,
}

impl Held {
    /// The turn is held for background work: the run's total cap starts at its first hold, and the
    /// quiet cap starts again from now.
    fn hold(&mut self, limit: Duration, idle: Duration) {
        let now = tokio::time::Instant::now();
        self.limit_at.get_or_insert(now + limit);
        self.idle_at = Some(now + idle);
    }

    /// The turn is no longer held: the quiet cap goes, and the run's total cap stays.
    fn release(&mut self) {
        self.idle_at = None;
    }

    /// The instants the run loop watches: the run's total cap and this hold's quiet cap. The total
    /// cap is watched only while a hold is on, because it ends a hold, and a run that is not waiting
    /// on background work is not ended by it.
    fn due(&self) -> (Option<tokio::time::Instant>, Option<tokio::time::Instant>) {
        (self.idle_at.and(self.limit_at), self.idle_at)
    }
}

/// How the run's frame loop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// A turn reported its result: the prompt's, or the one that answered it.
    Finished,
    /// The engine's stream closed.
    StreamEnded,
    /// The run was cancelled.
    Cancelled,
    /// The hold's total cap ran out.
    Limit,
    /// The hold's quiet cap ran out.
    Idle,
    /// The hold ended and the follow-up turn the CLI starts for it never came.
    Grace,
    /// The events receiver is gone: nothing is listening, so the run stops where it is.
    Abandoned,
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
    let stream = lines(process.stdout);
    let end = read_frames(
        stream,
        &mut stdin,
        &mut mapper,
        &events,
        &mut steering,
        &mut controls,
        &cancel,
    )
    .await;
    if end == End::Abandoned {
        return;
    }
    let stream_ended = end == End::StreamEnded;
    // Work still running now dies with the CLI; say why it is being stopped. A cap says which one
    // ended the hold, so a run that waited on work that never answered is not reported as finished work.
    let held_reason = match end {
        End::Limit => Some("limit"),
        End::Idle => Some("idle"),
        _ => None,
    };
    // A finished turn's outcome is held while the work it started still runs; a cap or the grace
    // that ends the wait takes it, so the run reports the turn it was holding, not a failure.
    let outcome = mapper.outcome.take().or_else(|| mapper.held.take());
    // Closing stdin is the CLI's signal to exit.
    drop(stdin);
    for event in mapper.close_open_blocks() {
        let _ = events.send(event).await;
    }
    for event in mapper.workflows.end_all() {
        let _ = events.send(event).await;
    }
    let cancelled = cancel.is_cancelled();
    let why = if cancelled {
        "cancelled"
    } else if let Some(reason) = held_reason {
        reason
    } else if stream_ended {
        "engine exited"
    } else {
        "turn ended"
    };
    for event in mapper.end_background(why) {
        let _ = events.send(event).await;
    }
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

/// Reads the engine's frames until the run ends: a turn reports its result, the stream closes, the
/// hold's caps run out, the run is cancelled, or nothing is listening any more. `stdin` takes the
/// steering messages the CLI folds into the running turn.
async fn read_frames<S, W>(
    mut stream: S,
    stdin: &mut W,
    mapper: &mut StreamMapper,
    events: &mpsc::Sender<EngineEvent>,
    steering: &mut mpsc::UnboundedReceiver<SteerMessage>,
    controls: &mut mpsc::UnboundedReceiver<RunControl>,
    cancel: &CancellationToken,
) -> End
where
    S: Stream<Item = Result<String, LinesCodecError>> + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut grace = None;
    let mut hold = Held::default();
    loop {
        let (limit_due, idle_due) = hold.due();
        tokio::select! {
            biased;
            () = cancel.cancelled() => return End::Cancelled,
            () = sleep_until(grace) => return End::Grace,
            () = sleep_until(limit_due), if limit_due.is_some() => return End::Limit,
            () = sleep_until(idle_due), if idle_due.is_some() => return End::Idle,
            Some(control) = controls.recv() => {
                let RunControl::StopWorkflow { tool_call_id } = control;
                if let Some(frame) = mapper.workflows.stop_request(&tool_call_id) {
                    if let Err(error) = stdin.write_all(format!("{frame}\n").as_bytes()).await {
                        tracing::debug!(%error, "could not ask Claude to stop a workflow");
                    }
                    let _ = stdin.flush().await;
                } else if events.send(Workflows::already_ended(&tool_call_id)).await.is_err() {
                    return End::Abandoned;
                }
            }
            Some(steer) = steering.recv() => {
                // The CLI folds a message sent mid-turn into the running turn.
                let frame = format!("{}\n", user_frame(&steer.prompt));
                if let Err(error) = stdin.write_all(frame.as_bytes()).await {
                    tracing::debug!(%error, "could not steer the run");
                }
                let _ = stdin.flush().await;
            }
            line = stream.next() => {
                let Some(line) = line else { return End::StreamEnded };
                let Ok(line) = line else { continue };
                let Ok(frame) = serde_json::from_str::<Value>(&line) else {
                    tracing::debug!("skipping a non-JSON Claude line");
                    continue;
                };
                for event in mapper.on_frame(&frame) {
                    if events.send(event).await.is_err() {
                        return End::Abandoned;
                    }
                }
                if mapper.outcome.is_some() {
                    return End::Finished;
                }
                if mapper.waiting_on_hold() {
                    // The background work ended; the follow-up turn has a short grace, not a cap.
                    hold.release();
                    grace.get_or_insert_with(|| tokio::time::Instant::now() + HOLD_GRACE);
                } else if mapper.held.is_some() && mapper.holding() {
                    // Held for work that is still running: each line is a sign of life, and the
                    // caps end the hold if the notifications never come.
                    grace = None;
                    hold.hold(HOLD_LIMIT, HOLD_IDLE);
                } else {
                    grace = None;
                    hold.release();
                }
            }
        }
    }
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
pub(crate) fn prompt_uuid() -> String {
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::pin::Pin;

    use super::*;

    const STEP: Duration = Duration::from_millis(1);

    fn started(id: &str) -> Value {
        json!({ "type": "system", "subtype": "task_started", "task_id": id,
            "task_type": "local_agent", "description": "probe" })
    }

    fn ended(id: &str) -> Value {
        json!({ "type": "system", "subtype": "task_notification", "task_id": id,
            "status": "completed" })
    }

    fn finished() -> Value {
        json!({ "type": "result", "subtype": "success", "is_error": false })
    }

    fn said(text: &str) -> Value {
        json!({ "type": "assistant", "message": { "content": [{ "type": "text", "text": text }] } })
    }

    /// A scripted engine stream: each step's frame arrives after its delay of paused time, and a step
    /// with no frame closes the stream after its delay.
    fn script(
        steps: Vec<(Duration, Option<Value>)>,
    ) -> Pin<Box<dyn Stream<Item = Result<String, LinesCodecError>>>> {
        Box::pin(futures_util::stream::unfold(
            VecDeque::from(steps),
            |mut steps| async move {
                let (delay, frame) = steps.pop_front()?;
                tokio::time::sleep(delay).await;
                let frame = frame?;
                Some((Ok(frame.to_string()), steps))
            },
        ))
    }

    /// Runs the frame loop over a scripted stream and reports how it ended, how long the run took in
    /// paused time, and the mapper it left behind.
    async fn driven(steps: Vec<(Duration, Option<Value>)>) -> (End, Duration, StreamMapper) {
        // Held for the whole call: a closed events channel would stop the run instead.
        let (events, _listening) = mpsc::channel(1024);
        let (_steering, mut steering) = mpsc::unbounded_channel();
        let (_controls, mut controls) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        let mut stdin = tokio::io::sink();
        let mut mapper = StreamMapper::default();
        let began = tokio::time::Instant::now();
        let end = read_frames(
            script(steps),
            &mut stdin,
            &mut mapper,
            &events,
            &mut steering,
            &mut controls,
            &cancel,
        )
        .await;
        (end, began.elapsed(), mapper)
    }

    /// The quiet cap's arm: a hold whose work goes quiet is ended by it, and every line that says the
    /// work is still there starts the quiet spell again, so it counts from the last line, not from the
    /// start of the hold.
    #[tokio::test(start_paused = true)]
    async fn the_quiet_cap_ends_a_silent_hold_and_restarts_on_every_sign_of_life() {
        let mut steps = vec![
            (STEP, Some(started("a1"))),
            (STEP, Some(finished())),
            (STEP, Some(said("the follow-up turn starts"))),
        ];
        // Twelve minutes of signs of life, then silence. The quiet cap runs from the last of them.
        let talk = Duration::from_secs(60) * 12;
        for _ in 0..12 {
            steps.push((Duration::from_secs(60), Some(said("still working"))));
        }
        steps.push((HOLD_LIMIT, None));

        let (end, elapsed, mapper) = driven(steps).await;
        assert_eq!(end, End::Idle);
        assert!(
            elapsed >= talk + HOLD_IDLE,
            "the quiet cap must count from the last line, not from the start of the hold: {elapsed:?}"
        );
        assert!(
            elapsed < HOLD_LIMIT,
            "the total cap must not be what ended this hold: {elapsed:?}"
        );
        // The held turn's outcome is still there for the caller's teardown to report.
        assert_eq!(mapper.held, Some(EngineOutcome::Completed));
    }

    /// The total cap's arm: background work that starts and ends over and over leaves the mapper
    /// un-held for a moment each time. The run's total cap counts from its first hold, so that
    /// alternating traffic cannot keep the run open past it.
    #[tokio::test(start_paused = true)]
    async fn the_total_cap_counts_across_holds_so_alternating_work_cannot_keep_the_run_open() {
        let mut steps = vec![
            (STEP, Some(started("a1"))),
            (STEP, Some(finished())),
            (STEP, Some(said("the follow-up turn starts"))),
        ];
        // An agent ends and the next starts every 50 s. The last start lands just before the total
        // cap, so the run is holding when it runs out.
        let mut live = "a1".to_owned();
        for step in 1..=36 {
            let at = Duration::from_secs(50);
            if step % 2 == 1 {
                steps.push((at, Some(ended(&live))));
            } else {
                let id = format!("a{}", step / 2 + 1);
                steps.push((at, Some(started(&id))));
                live = id;
            }
        }
        // Two minutes of silence, shorter than the quiet cap, so only the total cap can end the run:
        // a hold that restarted its total cap would let the stream close instead.
        steps.push((Duration::from_secs(120), None));

        let (end, elapsed, mapper) = driven(steps).await;
        assert_eq!(end, End::Limit);
        assert!(
            elapsed >= HOLD_LIMIT,
            "the total cap must count from the run's first hold: {elapsed:?}"
        );
        assert!(
            elapsed < HOLD_LIMIT + Duration::from_secs(50),
            "the total cap must not be restarted by the holds after it: {elapsed:?}"
        );
        assert_eq!(mapper.held, Some(EngineOutcome::Completed));
    }

    /// The same caps one level down: the total cap belongs to the run, the quiet cap to the hold.
    #[tokio::test(start_paused = true)]
    async fn the_total_cap_spans_the_runs_holds_while_the_quiet_cap_restarts() {
        let began = tokio::time::Instant::now();
        let mut hold = Held::default();
        assert_eq!(
            hold.due(),
            (None, None),
            "a run that is not holding watches nothing"
        );

        hold.hold(HOLD_LIMIT, HOLD_IDLE);
        assert_eq!(
            hold.due(),
            (Some(began + HOLD_LIMIT), Some(began + HOLD_IDLE))
        );

        // The work ends for a moment: the quiet cap goes, and the total cap stays where the run's
        // first hold put it.
        tokio::time::sleep(Duration::from_secs(60)).await;
        hold.release();
        assert_eq!(hold.due(), (None, None));
        assert_eq!(
            hold.limit_at,
            Some(began + HOLD_LIMIT),
            "the total cap belongs to the run"
        );

        // The work comes back: the quiet cap counts from here, the total cap does not move.
        hold.hold(HOLD_LIMIT, HOLD_IDLE);
        let now = tokio::time::Instant::now();
        assert_eq!(
            hold.due(),
            (Some(began + HOLD_LIMIT), Some(now + HOLD_IDLE))
        );
    }
}
