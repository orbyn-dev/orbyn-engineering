//! `GET /ws/terminal/:thread_id`: binary in and out; a JSON text frame
//! `{cols,rows}` resizes. Trust is checked before the socket upgrades. The same
//! with `/:terminal` attaches to a named terminal, and `?observe=true` watches
//! one without being able to type or resize. Both routes need the person key (`routes/classes.rs`): a browser cannot set a
//! header on a socket and the key must not be in the URL, so the page offers it as the subprotocol `orbyn.person.<key>`
//! beside `orbyn.v1`, and the upgrade answers `orbyn.v1` only (never the key).

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::{SinkExt, Stream, StreamExt, stream};
use orbyn_contracts::{
    CommandOutputEvent, OpenConsole, OpenTerminal, RenameTerminal, RunScript,
    TerminalAttentionOpen, TerminalScrollback,
};
use orbyn_core::CoreError;
use orbyn_core::host::terminal::{
    MAIN_TERMINAL, PtyOutputState, PtySession, PtySpec, TerminalLabel, default_shell,
};
use orbyn_core::util::new_id;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};

use crate::http::access::SOCKET_PROTOCOL;
use crate::http::{ApiFailure, ApiJson, ApiResult, AppState, LOCAL_USER};
use axum::Router;
use axum::routing::{get, patch, post};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/ws/terminal/{thread_id}", get(terminal))
        .route("/ws/terminal/{thread_id}/{terminal}", get(attach))
        .route("/api/threads/{id}/terminals", get(list).post(open))
        .route("/api/threads/{id}/terminal-attention", get(attention))
        .route("/api/threads/{id}/console", post(console))
        .route(
            "/api/threads/{id}/terminals/{terminal}",
            patch(rename).delete(close),
        )
        .route(
            "/api/threads/{id}/terminals/{terminal}/scrollback",
            get(scrollback).delete(clear),
        )
        .route(
            "/api/threads/{id}/terminals/{terminal}/output/events",
            get(output_events),
        )
        .route("/api/threads/{id}/scripts", get(scripts).post(run_script))
}

/// A shell with no client attached survives this long, for reconnects.
const IDLE_SHELL: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
struct Resize {
    cols: u16,
    rows: u16,
}

pub async fn terminal(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    upgrade: WebSocketUpgrade,
) -> ApiResult<Response> {
    let thread = state.core.store.thread(&thread_id)?;
    let project = state.core.store.project(&thread.project_id)?;
    if !project.trusted {
        return Err(CoreError::FolderUntrusted.into());
    }
    let session = match state.core.terminals.session(&thread_id) {
        Some(session) => session,
        None => {
            state.core.record_user_audit(
                LOCAL_USER,
                "terminal.open",
                &thread.cwd,
                &thread_id,
                json!({ "managed": false }),
            )?;
            state.core.terminals.open(
                &thread_id,
                PtySpec {
                    program: default_shell(),
                    args: Vec::new(),
                    cwd: thread.cwd.clone().into(),
                    cols: 120,
                    rows: 32,
                    engine: false,
                },
            )?
        }
    };
    Ok(upgrade
        .protocols([SOCKET_PROTOCOL])
        .on_upgrade(move |socket| {
            serve(
                state,
                thread_id,
                MAIN_TERMINAL.to_owned(),
                session,
                socket,
                false,
            )
        }))
}

fn trusted_thread(state: &AppState, id: &str) -> ApiResult<orbyn_contracts::Thread> {
    let thread = state.core.store.thread(id)?;
    if !state.core.store.project(&thread.project_id)?.trusted {
        return Err(CoreError::FolderUntrusted.into());
    }
    Ok(thread)
}

fn not_found() -> ApiFailure {
    CoreError::NotFound("terminal".into()).into()
}

pub async fn attach(
    State(state): State<AppState>,
    Path((thread_id, terminal)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
    upgrade: WebSocketUpgrade,
) -> ApiResult<Response> {
    trusted_thread(&state, &thread_id)?;
    let session = state
        .core
        .terminals
        .named(&thread_id, &terminal)
        .ok_or_else(not_found)?;
    let observe = query.get("observe").is_some_and(|value| value == "true");
    Ok(upgrade
        .protocols([SOCKET_PROTOCOL])
        .on_upgrade(move |socket| serve(state, thread_id, terminal, session, socket, observe)))
}

pub async fn list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    state.core.store.thread(&id)?;
    Ok(Json(state.core.terminals.list(&id)))
}

/// `GET /api/threads/:id/terminal-attention`: the engine consoles waiting for the
/// person now, from the in-memory registry. Local: the same redacted values the
/// thread's log already carries.
pub async fn attention(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let items = state.core.terminal_attention(&id)?;
    Ok(Json(TerminalAttentionOpen { items }))
}

pub async fn open(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<OpenTerminal>,
) -> ApiResult<impl IntoResponse> {
    let thread = trusted_thread(&state, &id)?;
    let group = match &body.split_of {
        Some(other) => Some(
            state
                .core
                .terminals
                .group_of(&id, other)
                .ok_or_else(not_found)?,
        ),
        None => None,
    };
    let terminal = new_id();
    state.core.record_user_audit(
        LOCAL_USER,
        "terminal.open",
        &thread.cwd,
        &id,
        json!({ "managed": false, "terminal": terminal }),
    )?;
    let info = state.core.terminals.open_named(
        &id,
        &terminal,
        TerminalLabel {
            title: body.title,
            group,
            direction: body.direction,
            script: None,
        },
        PtySpec {
            program: default_shell(),
            args: Vec::new(),
            cwd: thread.cwd.into(),
            cols: body.cols.unwrap_or(120),
            rows: body.rows.unwrap_or(32),
            engine: false,
        },
    )?;
    Ok((StatusCode::CREATED, Json(info)))
}

/// The engine's own CLI resumed on this thread's session, in a terminal of its own.
pub async fn console(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<OpenConsole>,
) -> ApiResult<impl IntoResponse> {
    trusted_thread(&state, &id)?;
    let info = state
        .core
        .open_engine_console(
            &id,
            (body.cols.unwrap_or(120), body.rows.unwrap_or(32)),
            LOCAL_USER,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(info)))
}

pub async fn rename(
    State(state): State<AppState>,
    Path((id, terminal)): Path<(String, String)>,
    ApiJson(body): ApiJson<RenameTerminal>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.terminals.rename(
        &id,
        &terminal,
        &body.title,
    )?))
}

pub async fn close(
    State(state): State<AppState>,
    Path((id, terminal)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    state.core.terminals.close_one(&id, &terminal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn scrollback(
    State(state): State<AppState>,
    Path((id, terminal)): Path<(String, String)>,
) -> ApiResult<impl IntoResponse> {
    let session = state
        .core
        .terminals
        .named(&id, &terminal)
        .ok_or_else(not_found)?;
    let text = String::from_utf8_lossy(&session.scrollback()).into_owned();
    Ok(Json(TerminalScrollback { text }))
}

/// How much retained output one snapshot frame carries.
const SNAPSHOT_CHUNK: usize = 64 * 1024;
const OUTPUT_KEEP_ALIVE: Duration = Duration::from_secs(15);

/// `GET /api/threads/:id/terminals/:terminal/output/events`: a read-only subscription
/// to one terminal's output, for a client that runs a command and reads what it prints.
/// It sends the retained output first, then each chunk as the PTY broadcasts it, and
/// ends with `ended` when the terminal's output ends. The terminal socket stays the
/// interactive transport; this one is one way, pushes on its own and ends by itself.
pub async fn output_events(
    State(state): State<AppState>,
    Path((thread_id, terminal)): Path<(String, String)>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    state.core.store.thread(&thread_id)?;
    let session = state
        .core
        .terminals
        .named(&thread_id, &terminal)
        .ok_or_else(not_found)?;
    // Taken under the scrollback lock with the receiver, so no output falls between the
    // retained frames and the live ones.
    let (scrollback, output) = session.attach();
    let frames = OutputFrames {
        output_state: session.output_state(),
        session,
        output,
        pending: snapshot_frames(&scrollback),
        text: TextCarry::default(),
        done: false,
    };
    let frames = stream::unfold(frames, |mut frames| async move {
        let event = frames.next_frame().await?;
        let frame = Event::default().json_data(&event).ok()?;
        Some((Ok::<_, Infallible>(frame), frames))
    });
    Ok(Sse::new(frames).keep_alive(KeepAlive::new().interval(OUTPUT_KEEP_ALIVE)))
}

/// The output subscription's state: what it has not sent yet, what it has decoded so far,
/// and whether the output has ended.
struct OutputFrames {
    session: Arc<PtySession>,
    output: tokio::sync::broadcast::Receiver<Vec<u8>>,
    output_state: tokio::sync::watch::Receiver<PtyOutputState>,
    pending: VecDeque<CommandOutputEvent>,
    text: TextCarry,
    done: bool,
}

impl OutputFrames {
    /// The next frame. Output is pushed: this waits on the PTY broadcast, it never asks
    /// the terminal for a new read.
    async fn next_frame(&mut self) -> Option<CommandOutputEvent> {
        loop {
            if let Some(frame) = self.pending.pop_front() {
                return Some(frame);
            }
            if self.done {
                return None;
            }
            match next_output(&mut self.output, &mut self.output_state).await {
                Ok(Some(bytes)) => {
                    let text = self.text.push(&bytes);
                    if !text.is_empty() {
                        return Some(CommandOutputEvent::Chunk { text });
                    }
                }
                Ok(None) | Err(RecvError::Closed) => {
                    self.done = true;
                    // A character split at the end has no next bytes, so it reads lossily.
                    let tail = self.text.finish();
                    let ended = CommandOutputEvent::Ended {
                        exit_code: self.session.exit_code(),
                    };
                    if tail.is_empty() {
                        return Some(ended);
                    }
                    self.pending.push_back(ended);
                    return Some(CommandOutputEvent::Chunk { text: tail });
                }
                Err(RecvError::Lagged(dropped)) => {
                    // Chunks were dropped, so the order a client sees would have a hole;
                    // end the stream and let it connect again for the retained output.
                    self.done = true;
                    return Some(CommandOutputEvent::Overflow { dropped });
                }
            }
        }
    }
}

/// The retained output as snapshot frames, cut so one large scrollback is not one frame.
fn snapshot_frames(scrollback: &[u8]) -> VecDeque<CommandOutputEvent> {
    let mut frames = VecDeque::new();
    let mut text = TextCarry::default();
    for chunk in scrollback.chunks(SNAPSHOT_CHUNK) {
        let decoded = text.push(chunk);
        if !decoded.is_empty() {
            frames.push_back(CommandOutputEvent::Snapshot { text: decoded });
        }
    }
    let tail = text.finish();
    if !tail.is_empty() {
        frames.push_back(CommandOutputEvent::Snapshot { text: tail });
    }
    frames
}

/// PTY output read as UTF-8 across chunk boundaries: a character split between two
/// reads waits for its next bytes, and bytes that are not text read as U+FFFD, the same
/// as `TerminalScrollback.text`.
#[derive(Default)]
struct TextCarry(Vec<u8>);

impl TextCarry {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.0.extend_from_slice(bytes);
        let mut text = String::new();
        loop {
            let error = match std::str::from_utf8(&self.0) {
                Ok(valid) => {
                    text.push_str(valid);
                    self.0.clear();
                    break;
                }
                Err(error) => error,
            };
            let valid = error.valid_up_to();
            if let Ok(part) = std::str::from_utf8(&self.0[..valid]) {
                text.push_str(part);
            }
            match error.error_len() {
                Some(len) => {
                    text.push(char::REPLACEMENT_CHARACTER);
                    self.0.drain(..valid + len);
                }
                None => {
                    self.0.drain(..valid);
                    break;
                }
            }
        }
        text
    }

    /// What was left when the output ended, read the same lossy way.
    fn finish(&mut self) -> String {
        let rest = std::mem::take(&mut self.0);
        String::from_utf8_lossy(&rest).into_owned()
    }
}

pub async fn clear(
    State(state): State<AppState>,
    Path((id, terminal)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    state
        .core
        .terminals
        .named(&id, &terminal)
        .ok_or_else(not_found)?
        .clear();
    Ok(StatusCode::NO_CONTENT)
}

pub async fn scripts(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(state.core.project_file(&id)?))
}

pub async fn run_script(
    State(state): State<AppState>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<RunScript>,
) -> ApiResult<impl IntoResponse> {
    let terminal = match (body.name.as_deref(), body.id.as_deref()) {
        (Some(name), None) => state.core.run_script(&id, name, LOCAL_USER)?,
        (None, Some(action)) => state.core.run_script_by_id(&id, action, LOCAL_USER)?,
        _ => return Err(CoreError::Invalid("Choose one saved action ID or name.".into()).into()),
    };
    Ok((StatusCode::CREATED, Json(terminal)))
}

async fn serve(
    state: AppState,
    thread_id: String,
    terminal: String,
    session: Arc<PtySession>,
    socket: WebSocket,
    observe: bool,
) {
    let (mut sender, mut receiver) = socket.split();
    let (scrollback, mut output) = session.attach();
    let mut output_state = session.output_state();
    if !scrollback.is_empty()
        && sender
            .send(Message::Binary(scrollback.into()))
            .await
            .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            chunk = next_output(&mut output, &mut output_state) => match chunk {
                Ok(Some(bytes)) => {
                    if sender.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                Err(RecvError::Lagged(count)) => {
                    tracing::warn!(count, thread_id, terminal, "terminal viewer lagged behind PTY output");
                }
                Ok(None) | Err(RecvError::Closed) => {
                    let _ = sender.send(Message::Close(None)).await;
                    break;
                }
            },
            message = receiver.next() => match message {
                Some(Ok(Message::Binary(_) | Message::Text(_)))
                    if !input_allowed(observe, session.is_alive(), *output_state.borrow()) => {}
                Some(Ok(Message::Binary(bytes))) => {
                    if session.write(&bytes).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    if let Ok(size) = serde_json::from_str::<Resize>(&text)
                        && let Err(error) = session.resize(size.cols, size.rows)
                    {
                        tracing::debug!(%error, "resize failed");
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
    drop(output);
    // Named terminals stay until closed, or until their thread settles.
    if terminal == MAIN_TERMINAL && !observe {
        tokio::spawn(reap_if_idle(state, thread_id, session));
    }
}

/// Closes a plain shell nobody came back to. A terminal-tier run owns its PTY and
/// ends it through the run, so it is left alone.
async fn reap_if_idle(state: AppState, thread_id: String, session: Arc<PtySession>) {
    tokio::time::sleep(IDLE_SHELL).await;
    let attached = session.attached_clients();
    let running = state
        .core
        .store
        .thread(&thread_id)
        .is_ok_and(|thread| thread.active_run.is_some());
    if attached == 0 && !running {
        let _ = state.core.terminals.close_one(&thread_id, MAIN_TERMINAL);
    }
}

async fn next_output(
    output: &mut tokio::sync::broadcast::Receiver<Vec<u8>>,
    state: &mut tokio::sync::watch::Receiver<PtyOutputState>,
) -> std::result::Result<Option<Vec<u8>>, RecvError> {
    loop {
        let current = *state.borrow();
        match current {
            PtyOutputState::Closed => return Ok(None),
            PtyOutputState::Ended => return queued_output(output),
            PtyOutputState::Reading => {}
        }
        tokio::select! {
            chunk = output.recv() => return chunk.map(Some),
            changed = state.changed() => {
                if changed.is_err() { return queued_output(output); }
            }
        }
    }
}

fn queued_output(
    output: &mut tokio::sync::broadcast::Receiver<Vec<u8>>,
) -> std::result::Result<Option<Vec<u8>>, RecvError> {
    match output.try_recv() {
        Ok(bytes) => Ok(Some(bytes)),
        Err(TryRecvError::Lagged(count)) => Err(RecvError::Lagged(count)),
        Err(TryRecvError::Empty | TryRecvError::Closed) => Ok(None),
    }
}

fn input_allowed(observe: bool, child_alive: bool, output_state: PtyOutputState) -> bool {
    !observe && child_alive && output_state == PtyOutputState::Reading
}

#[cfg(test)]
mod tests {
    use super::next_output;
    use orbyn_core::host::terminal::PtyOutputState;

    #[test]
    fn draining_output_does_not_extend_input_or_resize_authority() {
        use super::input_allowed;
        assert!(input_allowed(false, true, PtyOutputState::Reading));
        assert!(!input_allowed(true, true, PtyOutputState::Reading));
        assert!(!input_allowed(false, false, PtyOutputState::Reading));
        assert!(!input_allowed(false, true, PtyOutputState::Ended));
        assert!(!input_allowed(false, true, PtyOutputState::Closed));
    }

    #[tokio::test]
    async fn child_exit_does_not_discard_pending_pty_output() {
        let (output, mut receiver) = tokio::sync::broadcast::channel(4);
        let (child, _exited) = tokio::sync::watch::channel(None);
        let (completion, mut state) = tokio::sync::watch::channel(PtyOutputState::Reading);
        {
            let next = next_output(&mut receiver, &mut state);
            tokio::pin!(next);
            assert!(futures_util::poll!(&mut next).is_pending());
            child.send_replace(Some(0));
            assert!(
                futures_util::poll!(&mut next).is_pending(),
                "child exit is not PTY output EOF"
            );
            output.send(b"final output".to_vec()).unwrap();
            completion.send_replace(PtyOutputState::Ended);
            assert_eq!(next.await.unwrap(), Some(b"final output".to_vec()));
        }
        assert_eq!(next_output(&mut receiver, &mut state).await.unwrap(), None);
    }

    #[tokio::test]
    async fn ended_output_drains_and_reports_overflow_before_closing() {
        let (output, mut receiver) = tokio::sync::broadcast::channel(2);
        let (_completion, mut state) = tokio::sync::watch::channel(PtyOutputState::Ended);
        for byte in 0..4 {
            output.send(vec![byte]).unwrap();
        }
        assert!(matches!(
            next_output(&mut receiver, &mut state).await,
            Err(super::RecvError::Lagged(2))
        ));
        assert_eq!(
            next_output(&mut receiver, &mut state).await.unwrap(),
            Some(vec![2])
        );
        assert_eq!(
            next_output(&mut receiver, &mut state).await.unwrap(),
            Some(vec![3])
        );
        assert_eq!(next_output(&mut receiver, &mut state).await.unwrap(), None);
    }

    #[tokio::test]
    async fn owner_close_does_not_wait_for_a_descendants_pty_eof() {
        let (output, mut receiver) = tokio::sync::broadcast::channel(4);
        let (completion, mut state) = tokio::sync::watch::channel(PtyOutputState::Reading);
        let next = next_output(&mut receiver, &mut state);
        tokio::pin!(next);
        assert!(futures_util::poll!(&mut next).is_pending());
        completion.send_replace(PtyOutputState::Closed);
        assert_eq!(next.await.unwrap(), None);
        output.send(b"final output".to_vec()).unwrap();
    }
}
