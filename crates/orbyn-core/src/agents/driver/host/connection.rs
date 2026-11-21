//! One live host process: requests out, frames routed to the run they belong to, and the
//! rules that end it (exit, closed stdout, stalls, malformed frames, an oversized frame,
//! idle). It never restarts anything; it says why it ended and the pool decides.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

use super::frames::{AskFrame, HostError, Incoming, RunDone, decode, is_gated, request};
use super::process::{Read, Ready, Transport};
use super::state::{CrashInfo, CrashKind, ExitRecord, MalformedWindow, Timings};

/// What a run hears from its host.
#[derive(Debug)]
pub enum RunFrame {
    Delta(Value),
    Permission(AskFrame),
    Question(AskFrame),
    Session {
        engine_session_id: Option<String>,
        turn_id: Option<String>,
    },
    Done(RunDone),
    Background(Value),
}

/// Why a connection ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosedReason {
    /// The host died, stalled or broke the protocol. Open runs failed with it.
    Crash(CrashInfo),
    /// No run and no background work for the idle timeout.
    Idle,
    /// The core closed it: the app is exiting.
    Shutdown,
    /// The person stopped it.
    Stopped,
}

/// Why a request got no answer.
#[derive(Debug)]
pub enum RequestError {
    /// The host never announced this request in `hello.capabilities`; nothing was sent.
    Unsupported(String),
    /// The host answered `ok: false`.
    Failed(HostError),
    /// The host ended before it answered.
    Closed,
}

/// The connection is closing and takes no new run.
#[derive(Debug)]
pub struct Closing;

struct Route {
    session: String,
    thread: String,
    tx: mpsc::UnboundedSender<RunFrame>,
}

#[derive(Default)]
struct Book {
    routes: HashMap<u64, Route>,
    /// Background tasks the host reported started and not yet ended.
    background: HashSet<String>,
    closing: bool,
}

type Pending = oneshot::Sender<Result<Value, HostError>>;

struct Shared {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Pending>>,
    book: Mutex<Book>,
    malformed: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A handle on one live host. Cheap to share.
pub struct Connection {
    pub id: u64,
    pub capabilities: Vec<String>,
    pub host_version: String,
    started: Instant,
    shared: Arc<Shared>,
    outbox: mpsc::UnboundedSender<String>,
    closed: watch::Receiver<Option<ClosedReason>>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// A run's frames. Dropping it removes the route.
pub struct RunRoute {
    pub frames: mpsc::UnboundedReceiver<RunFrame>,
    send_id: u64,
    shared: Arc<Shared>,
}

impl Drop for RunRoute {
    fn drop(&mut self) {
        lock(&self.shared.book).routes.remove(&self.send_id);
    }
}

impl Connection {
    /// Takes over a transport that finished its handshake and starts watching it.
    pub(crate) fn start(
        transport: Transport,
        ready: Ready,
        id: u64,
        timings: Timings,
        shutdown: CancellationToken,
        stop: CancellationToken,
    ) -> Arc<Self> {
        let shared = Arc::new(Shared {
            // 1 was `init`.
            next_id: AtomicU64::new(2),
            pending: Mutex::default(),
            book: Mutex::default(),
            malformed: AtomicU64::new(0),
        });
        let (closed_tx, closed) = watch::channel(None);
        let connection = Arc::new(Self {
            id,
            capabilities: ready.hello.capabilities,
            host_version: ready.hello.host,
            started: Instant::now(),
            shared: shared.clone(),
            outbox: transport.outbox.clone(),
            closed,
        });
        tokio::spawn(watch_host(
            transport, shared, timings, shutdown, stop, closed_tx,
        ));
        connection
    }

    pub fn uptime(&self) -> std::time::Duration {
        self.started.elapsed()
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.iter().any(|have| have == name)
    }

    /// An id for a `send`, taken before the request so its route exists before any frame can.
    pub fn next_id(&self) -> u64 {
        self.shared.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Sends `kind` and waits for its response.
    pub async fn request(&self, kind: &str, fields: Value) -> Result<Value, RequestError> {
        self.request_as(self.next_id(), kind, fields).await
    }

    /// `request` under an id taken with `next_id`.
    pub async fn request_as(
        &self,
        id: u64,
        kind: &str,
        fields: Value,
    ) -> Result<Value, RequestError> {
        if is_gated(kind) && !self.has_capability(kind) {
            return Err(RequestError::Unsupported(kind.to_owned()));
        }
        let (tx, rx) = oneshot::channel();
        lock(&self.shared.pending).insert(id, tx);
        if self.outbox.send(request(id, kind, fields)).is_err() {
            lock(&self.shared.pending).remove(&id);
            return Err(RequestError::Closed);
        }
        match rx.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(RequestError::Failed(error)),
            Err(_) => Err(RequestError::Closed),
        }
    }

    /// Sends `kind` and does not wait; the answer is ignored.
    pub fn fire(&self, kind: &str, fields: Value) {
        if is_gated(kind) && !self.has_capability(kind) {
            return;
        }
        let _ = self.outbox.send(request(self.next_id(), kind, fields));
    }

    /// Where the frames of `send_id` (and the background frames of `session`) go.
    pub fn register(&self, send_id: u64, session: &str, thread: &str) -> Result<RunRoute, Closing> {
        let mut book = lock(&self.shared.book);
        if book.closing {
            return Err(Closing);
        }
        let (tx, frames) = mpsc::unbounded_channel();
        book.routes.insert(
            send_id,
            Route {
                session: session.to_owned(),
                thread: thread.to_owned(),
                tx,
            },
        );
        Ok(RunRoute {
            frames,
            send_id,
            shared: self.shared.clone(),
        })
    }

    /// Resolves when the connection has ended, with why.
    pub async fn closed(&self) -> ClosedReason {
        let mut closed = self.closed.clone();
        loop {
            if let Some(reason) = closed.borrow_and_update().clone() {
                return reason;
            }
            if closed.changed().await.is_err() {
                return ClosedReason::Crash(CrashInfo::new(
                    CrashKind::Exited,
                    "The engine host connection was dropped.",
                ));
            }
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.borrow().is_some()
    }

    /// Background tasks reported started and not yet ended.
    pub fn open_background(&self) -> usize {
        lock(&self.shared.book).background.len()
    }

    /// The threads that have a run open, each once, in order.
    pub fn open_threads(&self) -> Vec<String> {
        let mut threads: Vec<String> = lock(&self.shared.book)
            .routes
            .values()
            .map(|route| route.thread.clone())
            .collect();
        threads.sort();
        threads.dedup();
        threads
    }

    /// Runs with an open route.
    pub fn open_runs(&self) -> usize {
        lock(&self.shared.book).routes.len()
    }

    /// Frames dropped as malformed so far.
    pub fn malformed_count(&self) -> u64 {
        self.shared.malformed.load(Ordering::Relaxed)
    }
}

fn dispatch(shared: &Shared, frame: Incoming, pings: &mut HashSet<u64>) -> bool {
    let route = |send_id: u64, frame: RunFrame| {
        let book = lock(&shared.book);
        if let Some(route) = book.routes.get(&send_id) {
            let _ = route.tx.send(frame);
        }
    };
    match frame {
        Incoming::Hello(_) => {}
        Incoming::Response(response) => {
            if pings.remove(&response.id) {
                pings.clear();
                return true;
            }
            if let Some(reply) = lock(&shared.pending).remove(&response.id) {
                let _ = reply.send(response.result);
            }
        }
        Incoming::Delta { send_id, update } => route(send_id, RunFrame::Delta(update)),
        Incoming::Permission(ask) => route(ask.send_id, RunFrame::Permission(ask)),
        Incoming::Question(ask) => route(ask.send_id, RunFrame::Question(ask)),
        Incoming::Session {
            send_id,
            engine_session_id,
            turn_id,
        } => route(
            send_id,
            RunFrame::Session {
                engine_session_id,
                turn_id,
            },
        ),
        Incoming::RunDone(done) => {
            // Exactly one `runDone` reaches a run: the route goes with it, so a second is dropped.
            let removed = lock(&shared.book).routes.remove(&done.send_id);
            if let Some(route) = removed {
                let _ = route.tx.send(RunFrame::Done(done));
            }
        }
        Incoming::Log { level } => {
            tracing::debug!(level, "engine host log");
        }
        Incoming::Background { session_id, event } => {
            let mut book = lock(&shared.book);
            let id = event.get("id").and_then(Value::as_str).map(str::to_owned);
            match (event.get("type").and_then(Value::as_str), id) {
                (Some("started"), Some(id)) => {
                    book.background.insert(id);
                }
                (Some("ended"), Some(id)) => {
                    book.background.remove(&id);
                }
                _ => {}
            }
            if let Some(route) = book
                .routes
                .values()
                .find(|route| route.session == session_id)
            {
                let _ = route.tx.send(RunFrame::Background(event));
            }
        }
    }
    false
}

async fn sleep_until_opt(due: Option<tokio::time::Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}

async fn crash(
    transport: &mut Transport,
    kind: CrashKind,
    what: &str,
    exit: Option<ExitRecord>,
) -> CrashInfo {
    let mut exit = exit;
    if exit.is_none() {
        // Closed output usually means the process is gone; give its exit status a moment to arrive.
        let waited =
            tokio::time::timeout(std::time::Duration::from_secs(1), transport.child.wait()).await;
        exit = waited
            .ok()
            .map(|status| status.map(ExitRecord::of).unwrap_or_default());
    }
    transport.stderr.drained().await;
    let mut info = CrashInfo::new(kind, what).with_stderr(&transport.stderr.text());
    info.exit = exit;
    if let Some(exit) = exit {
        info.message = format!("{what} ({})", exit.describe());
    }
    info
}

async fn watch_host(
    mut transport: Transport,
    shared: Arc<Shared>,
    timings: Timings,
    shutdown: CancellationToken,
    stop: CancellationToken,
    closed: watch::Sender<Option<ClosedReason>>,
) {
    let mut malformed = MalformedWindow::default();
    let mut pings: HashSet<u64> = HashSet::new();
    let mut misses = 0u32;
    let mut idle_since: Option<Instant> = None;
    let mut exit: Option<ExitRecord> = None;
    let mut ticker = tokio::time::interval_at(
        tokio::time::Instant::now() + timings.ping_every,
        timings.ping_every,
    );
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // After the process exits its last frames may still be in the pipe; they are read first.
    let mut drain_until: Option<tokio::time::Instant> = None;
    let reason = loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => {
                lock(&shared.book).closing = true;
                break ClosedReason::Shutdown;
            }
            () = stop.cancelled() => {
                lock(&shared.book).closing = true;
                break ClosedReason::Stopped;
            }
            item = transport.lines.recv() => match item {
                None | Some(Read::Eof) => {
                    break ClosedReason::Crash(crash(&mut transport, CrashKind::ClosedOutput, "The engine host closed its output", exit).await);
                }
                Some(Read::TooLong) => {
                    break ClosedReason::Crash(crash(&mut transport, CrashKind::Oversize, "The engine host sent a frame over 8 MiB.", exit).await);
                }
                Some(Read::Failed(error)) => {
                    break ClosedReason::Crash(crash(&mut transport, CrashKind::ClosedOutput, &format!("The engine host pipe failed: {error}"), exit).await);
                }
                Some(Read::Malformed) => {
                    shared.malformed.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!("dropped an engine host line that is not UTF-8");
                    if malformed.record(Instant::now(), &timings) {
                        break ClosedReason::Crash(crash(&mut transport, CrashKind::Malformed, "The engine host sent too many malformed frames.", exit).await);
                    }
                }
                Some(Read::Line(line)) => match decode(&line) {
                    Ok(frame) => {
                        if dispatch(&shared, frame, &mut pings) {
                            misses = 0;
                        }
                    }
                    Err(error) => {
                        shared.malformed.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!(%error, "dropped a malformed engine host frame");
                        if malformed.record(Instant::now(), &timings) {
                            break ClosedReason::Crash(crash(&mut transport, CrashKind::Malformed, "The engine host sent too many malformed frames.", exit).await);
                        }
                    }
                },
            },
            status = transport.child.wait(), if exit.is_none() => {
                exit = Some(status.map(ExitRecord::of).unwrap_or_default());
                drain_until = Some(tokio::time::Instant::now() + std::time::Duration::from_millis(500));
            }
            () = sleep_until_opt(drain_until), if drain_until.is_some() => {
                break ClosedReason::Crash(crash(&mut transport, CrashKind::Exited, "The engine host exited", exit).await);
            }
            _ = ticker.tick() => {
                let (runs, background) = {
                    let book = lock(&shared.book);
                    (book.routes.len(), book.background.len())
                };
                if runs > 0 {
                    idle_since = None;
                    if !pings.is_empty() {
                        misses += 1;
                        if misses >= timings.ping_misses {
                            break ClosedReason::Crash(crash(&mut transport, CrashKind::Stalled, "The engine host stopped answering.", exit).await);
                        }
                    }
                    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
                    pings.insert(id);
                    let _ = transport.outbox.send(request(id, "ping", Value::Null));
                } else {
                    pings.clear();
                    misses = 0;
                    if background > 0 {
                        idle_since = None;
                    } else if let Some(limit) = timings.idle_stop {
                        let since = *idle_since.get_or_insert_with(Instant::now);
                        if since.elapsed() >= limit {
                            // Decided under the lock, so no run can slip in as it stops.
                            let mut book = lock(&shared.book);
                            if book.routes.is_empty() && book.background.is_empty() {
                                book.closing = true;
                                break ClosedReason::Idle;
                            }
                        }
                    }
                }
            }
        }
    };
    if let ClosedReason::Crash(text) = &reason {
        tracing::warn!(kind = ?text.kind, exit = ?exit.map(|exit| exit.describe()), "{}", text.message);
    }
    lock(&shared.book).closing = true;
    // Runs read the reason first, then find their routes gone.
    let _ = closed.send(Some(reason.clone()));
    lock(&shared.pending).clear();
    lock(&shared.book).routes.clear();
    transport.close_stdin.cancel();
    if matches!(reason, ClosedReason::Crash(_)) && exit.is_none() {
        transport.child.terminate();
    }
    if tokio::time::timeout(timings.exit_grace, transport.child.wait())
        .await
        .is_err()
    {
        transport.child.terminate();
        let _ = tokio::time::timeout(timings.exit_grace, transport.child.wait()).await;
    }
    // Dropping the transport kills whatever is left of the tree.
}
