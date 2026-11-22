//! One pool per family (per instance where an SDK needs it): the host process every run of
//! that family shares, its state, and the restart policy. The pool starts the host on the
//! first run, restarts it after a crash with backoff, and gives up after repeated failures
//! until the person asks again. The idle timeout lives in the connection; the pool only
//! notices that it ended.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;

use super::connection::{ClosedReason, Connection};
use super::frames::{ErrorKind, HostSdk};
use super::process::{ConnectError, HostSpec, Transport, handshake};
use super::state::{CrashInfo, HostState, Ledger, Restart, Timings};
use crate::credentials::SpawnSecret;

/// Hands out the engine's key for one spawn, when the person opted in. It is called for
/// every start, because a restart needs it again; the value goes into the child's
/// environment and nowhere else.
pub type SecretSource = Arc<dyn Fn() -> Option<SpawnSecret> + Send + Sync>;

/// No host can serve the run, with a sentence for the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable(pub String);

/// The next automatic restart, while the pool waits to try again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostRestart {
    /// The restart being waited for, counting from one inside the window.
    pub attempt: u32,
    /// The most restarts allowed inside the window.
    pub limit: u32,
    /// Whole seconds until the next try; `None` when the pool has no deadline to report.
    pub next_try_seconds: Option<u32>,
}

/// A family's host for the UI and the engine log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostStatus {
    pub family: String,
    pub state: HostState,
    pub host_version: Option<String>,
    /// Restarts after a crash since the app started.
    pub restart_count: u32,
    /// How the last host failed, and when (unix seconds).
    pub last_crash: Option<(CrashInfo, i64)>,
    /// The threads with a run open on the live host.
    pub threads: Vec<String>,
    /// What the family's latest finished run failed with; `None` after one that completed.
    pub last_error: Option<ErrorKind>,
    /// Unix seconds the live host became ready.
    pub since: Option<i64>,
    /// The SDK package and version the live host's hello named.
    pub sdk: Option<HostSdk>,
    /// The requests the live host's hello says it implements.
    pub capabilities: Option<Vec<String>>,
    /// One host process serves every thread of the family.
    pub pooled: bool,
    /// Present only while the pool waits to restart.
    pub restart: Option<HostRestart>,
    /// Present only for a family that gave up: its restarts inside the window.
    pub stops_in_window: Option<u32>,
    /// A Rust driver wraps the host for this family.
    pub has_driver: bool,
    /// The host answered that this build carries no SDK for the family.
    pub sdk_missing: bool,
}

/// Why the person's stop or restart did nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The host still has this many background tasks (shells, subagents, workflows) that its
    /// stopping would end. The person's stop must say so to go on.
    BackgroundOpen(usize),
    /// The host and the app speak different protocols; only an update fixes it.
    VersionMismatch,
    /// The family runs on its Rust driver.
    NotUsed,
}

struct Inner {
    current: Option<Arc<Connection>>,
    /// Ends `current` when the person stops it.
    stop: Option<CancellationToken>,
    ledger: Ledger,
    spawned_before: bool,
    host_version: Option<String>,
    restart_count: u32,
    last_crash: Option<(CrashInfo, i64)>,
    /// When the restart being waited for will try next. `None` when no restart waits.
    next_try_at: Option<Instant>,
}

pub struct Pool {
    pub family: String,
    spec: HostSpec,
    timings: Timings,
    secrets: Option<SecretSource>,
    state: watch::Sender<HostState>,
    inner: Mutex<Inner>,
    /// What the latest finished run failed with. Apart from `inner`, so a run that ends never
    /// waits on a start or a stop.
    last_error: std::sync::Mutex<Option<ErrorKind>>,
    /// Set once a Rust driver is wrapped, so the UI knows a fallback can run the family.
    wraps_driver: AtomicBool,
    /// Set when the host answered that this build carries no SDK for the family.
    sdk_missing: AtomicBool,
    /// The resumable session of each open run, by engine and thread, for a catalog read
    /// that can use the live session instead of starting a throwaway agent.
    sessions: std::sync::Mutex<HashMap<(String, String), String>>,
    shutdown: CancellationToken,
    connections: AtomicU64,
}

impl Pool {
    pub fn new(
        family: impl Into<String>,
        spec: HostSpec,
        timings: Timings,
        secrets: Option<SecretSource>,
    ) -> Arc<Self> {
        Arc::new(Self {
            family: family.into(),
            spec,
            timings,
            secrets,
            state: watch::channel(HostState::Idle).0,
            inner: Mutex::new(Inner {
                current: None,
                stop: None,
                ledger: Ledger::default(),
                spawned_before: false,
                host_version: None,
                restart_count: 0,
                last_crash: None,
                next_try_at: None,
            }),
            last_error: std::sync::Mutex::new(None),
            wraps_driver: AtomicBool::new(false),
            sdk_missing: AtomicBool::new(false),
            sessions: std::sync::Mutex::new(HashMap::new()),
            shutdown: CancellationToken::new(),
            connections: AtomicU64::new(0),
        })
    }

    pub fn timings(&self) -> &Timings {
        &self.timings
    }

    pub fn state(&self) -> HostState {
        self.state.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<HostState> {
        self.state.subscribe()
    }

    pub async fn status(&self) -> HostStatus {
        let inner = self.inner.lock().await;
        let state = self.state();
        let (since, sdk, capabilities) = match (&state, inner.current.as_ref()) {
            (HostState::Ready, Some(connection)) if !connection.is_closed() => (
                Some(connection.started_at()),
                connection.sdk.clone(),
                Some(connection.capabilities.clone()),
            ),
            _ => (None, None, None),
        };
        let restart = match &state {
            HostState::Restarting { attempt, .. } => Some(HostRestart {
                attempt: *attempt,
                limit: u32::try_from(self.timings.max_restarts).unwrap_or(u32::MAX),
                next_try_seconds: inner.next_try_at.map(|at| {
                    u32::try_from(
                        at.saturating_duration_since(Instant::now())
                            .as_secs()
                            .max(1),
                    )
                    .unwrap_or(u32::MAX)
                }),
            }),
            _ => None,
        };
        let stops_in_window = matches!(&state, HostState::Unavailable { .. })
            .then(|| u32::try_from(inner.ledger.restarts()).unwrap_or(u32::MAX));
        HostStatus {
            family: self.family.clone(),
            state,
            host_version: inner.host_version.clone(),
            restart_count: inner.restart_count,
            last_crash: inner.last_crash.clone(),
            threads: inner
                .current
                .as_ref()
                .map(|connection| connection.open_threads())
                .unwrap_or_default(),
            last_error: *self
                .last_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            since,
            sdk,
            capabilities,
            pooled: true,
            restart,
            stops_in_window,
            has_driver: self.wraps_driver.load(Ordering::Relaxed),
            sdk_missing: self.sdk_missing.load(Ordering::Relaxed),
        }
    }

    /// A Rust driver now wraps this pool, so a run has a fallback that is not the host.
    pub fn mark_wrapped(&self) {
        self.wraps_driver.store(true, Ordering::Relaxed);
    }

    /// The host answered that this build carries no SDK for the family.
    pub fn note_sdk_missing(&self) {
        self.sdk_missing.store(true, Ordering::Relaxed);
    }

    /// A run got past the host's SDK load, so this build does carry the SDK.
    pub fn note_sdk_present(&self) {
        self.sdk_missing.store(false, Ordering::Relaxed);
    }

    /// What a run on this family ended with, for the status: a failure's kind, or `None` for a
    /// run that completed.
    pub fn note_run(&self, error: Option<ErrorKind>) {
        *self
            .last_error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = error;
    }

    /// Records the session a run opened, so a catalog read can name it.
    pub fn note_session(&self, engine: &str, thread: &str, session: &str) {
        if session.is_empty() {
            return;
        }
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert((engine.to_owned(), thread.to_owned()), session.to_owned());
        }
    }

    /// A run ended: its session is no longer open to name. Only its own session is
    /// removed, so a newer run on the same thread keeps the one it just recorded.
    pub fn forget_session(&self, engine: &str, thread: &str, session: Option<&str>) {
        let Some(session) = session else {
            return;
        };
        if let Ok(mut sessions) = self.sessions.lock() {
            let key = (engine.to_owned(), thread.to_owned());
            if sessions.get(&key).is_some_and(|id| id.as_str() == session) {
                sessions.remove(&key);
            }
        }
    }

    /// One open session of `engine`, when a run on it is still live.
    pub fn open_session(&self, engine: &str) -> Option<String> {
        self.sessions
            .lock()
            .ok()?
            .iter()
            .find_map(|((session_engine, _), id)| {
                if session_engine.as_str() == engine {
                    Some(id.clone())
                } else {
                    None
                }
            })
    }

    fn set(&self, state: HostState) {
        self.state.send_replace(state);
    }

    /// How long a run waits for a host that is starting or restarting.
    fn wait_limit(&self) -> Duration {
        self.timings.first_hello + self.timings.backoff_cap + self.timings.hello
    }

    /// The live connection, starting the host if it is not running. A host that is restarting
    /// is waited for; one that is final (mismatch, unavailable) refuses at once.
    pub async fn acquire(self: &Arc<Self>) -> Result<Arc<Connection>, Unavailable> {
        let deadline = tokio::time::Instant::now() + self.wait_limit();
        loop {
            let mut changes = self.state.subscribe();
            let state = changes.borrow_and_update().clone();
            match state {
                HostState::Ready => {
                    let live = self.inner.lock().await.current.clone();
                    if let Some(connection) = live.filter(|connection| !connection.is_closed()) {
                        return Ok(connection);
                    }
                    // The connection ended and the pool has not caught up yet.
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                HostState::Idle | HostState::Stopped | HostState::Crashed => {
                    self.start_now().await?
                }
                HostState::Starting | HostState::Restarting { .. } => {
                    let waited = tokio::time::timeout_at(deadline, changes.changed()).await;
                    if waited.is_err() {
                        return Err(Unavailable(
                            "The engine host did not come up in time.".to_owned(),
                        ));
                    }
                }
                HostState::NotUsed(_) => {
                    return Err(Unavailable(
                        "This engine does not use the engine host.".to_owned(),
                    ));
                }
                other => {
                    return Err(Unavailable(
                        other
                            .problem()
                            .unwrap_or_else(|| "The engine host is unavailable.".to_owned()),
                    ));
                }
            }
        }
    }

    /// Starts the host for a run that found none.
    async fn start_now(self: &Arc<Self>) -> Result<(), Unavailable> {
        let mut inner = self.inner.lock().await;
        if !matches!(
            self.state(),
            HostState::Idle | HostState::Stopped | HostState::Crashed
        ) {
            return Ok(());
        }
        self.start_locked(&mut inner).await
    }

    /// Starts the host with the pool locked and its state one that may start.
    async fn start_locked(self: &Arc<Self>, inner: &mut Inner) -> Result<(), Unavailable> {
        self.set(HostState::Starting);
        let first = !std::mem::replace(&mut inner.spawned_before, true);
        match self.connect(first).await {
            Ok((connection, stop)) => {
                self.adopt(inner, connection, stop);
                Ok(())
            }
            Err(ConnectError::Mismatch { app, host }) => {
                inner.next_try_at = None;
                self.set(HostState::Mismatch { app, host });
                Err(Unavailable(
                    HostState::Mismatch { app, host }
                        .problem()
                        .unwrap_or_default(),
                ))
            }
            Err(ConnectError::Failed(error)) => {
                let message = format!("The engine host could not start: {}", error.message);
                if let Some(wait) = self.after_crash(inner, error, Duration::ZERO) {
                    self.spawn_restart(wait);
                }
                Err(Unavailable(message))
            }
        }
    }

    async fn connect(
        &self,
        first: bool,
    ) -> Result<(Arc<Connection>, CancellationToken), ConnectError> {
        let secret = self.secrets.as_ref().and_then(|source| source());
        let mut transport = Transport::spawn(&self.spec, secret.as_ref())?;
        // The key is in the child's environment now; this copy is wiped.
        drop(secret);
        let hello = if first {
            self.timings.first_hello
        } else {
            self.timings.hello
        };
        let ready = handshake(
            &mut transport,
            &self.spec,
            &self.family,
            hello,
            self.timings.init,
        )
        .await?;
        let id = self.connections.fetch_add(1, Ordering::Relaxed) + 1;
        let stop = self.shutdown.child_token();
        let connection = Connection::start(
            transport,
            ready,
            id,
            self.timings.clone(),
            self.shutdown.clone(),
            stop.clone(),
        );
        Ok((connection, stop))
    }

    fn adopt(
        self: &Arc<Self>,
        inner: &mut Inner,
        connection: Arc<Connection>,
        stop: CancellationToken,
    ) {
        inner.host_version = Some(connection.host_version.clone());
        inner.current = Some(connection.clone());
        inner.stop = Some(stop);
        inner.next_try_at = None;
        self.set(HostState::Ready);
        let pool = self.clone();
        tokio::spawn(async move {
            let reason = connection.closed().await;
            pool.ended(&connection, reason).await;
        });
    }

    /// A crash: records it and says how long to wait before the next try, or `None` when
    /// the family gave up.
    fn after_crash(
        &self,
        inner: &mut Inner,
        crash: CrashInfo,
        uptime: Duration,
    ) -> Option<Duration> {
        tracing::warn!(family = %self.family, kind = ?crash.kind, "engine host failed");
        let error = crash.message.clone();
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
        inner.last_crash = Some((crash, at));
        let now = Instant::now();
        match inner.ledger.crashed(now, uptime, &self.timings) {
            Restart::GiveUp => {
                inner.next_try_at = None;
                self.set(HostState::Unavailable { reason: error });
                None
            }
            Restart::After(wait) => {
                inner.restart_count = inner.restart_count.saturating_add(1);
                inner.next_try_at = Some(now + wait);
                self.set(HostState::Restarting {
                    attempt: u32::try_from(inner.ledger.restarts()).unwrap_or(u32::MAX),
                    last_error: error,
                });
                Some(wait)
            }
        }
    }

    async fn ended(self: &Arc<Self>, connection: &Arc<Connection>, reason: ClosedReason) {
        let mut inner = self.inner.lock().await;
        if inner
            .current
            .as_ref()
            .is_none_or(|current| current.id != connection.id)
        {
            return;
        }
        inner.current = None;
        inner.stop = None;
        inner.next_try_at = None;
        match &reason {
            ClosedReason::Idle | ClosedReason::Stopped | ClosedReason::Shutdown => {
                if !self.state().is_final() {
                    self.set(if reason == ClosedReason::Shutdown {
                        HostState::Idle
                    } else {
                        HostState::Stopped
                    });
                }
            }
            ClosedReason::Crash(error) => {
                if let Some(wait) = self.after_crash(&mut inner, error.clone(), connection.uptime())
                {
                    self.spawn_restart(wait);
                }
            }
        }
    }

    fn spawn_restart(self: &Arc<Self>, wait: Duration) {
        let pool = self.clone();
        tokio::spawn(async move { pool.restart(wait).await });
    }

    /// Waits out the backoff and starts the host again, until it stays up or the family gives up.
    async fn restart(self: Arc<Self>, mut wait: Duration) {
        loop {
            tokio::select! {
                () = self.shutdown.cancelled() => return,
                () = tokio::time::sleep(wait) => {}
            }
            let mut inner = self.inner.lock().await;
            if !matches!(self.state(), HostState::Restarting { .. }) {
                return;
            }
            match self.connect(false).await {
                Ok((connection, stop)) => {
                    self.adopt(&mut inner, connection, stop);
                    return;
                }
                Err(ConnectError::Mismatch { app, host }) => {
                    inner.next_try_at = None;
                    self.set(HostState::Mismatch { app, host });
                    return;
                }
                Err(ConnectError::Failed(error)) => {
                    match self.after_crash(&mut inner, error, Duration::ZERO) {
                        Some(next) => wait = next,
                        None => return,
                    }
                }
            }
        }
    }

    /// The person asked again after the family gave up: nothing counts against it any more,
    /// and the next run starts the host. A mismatch stays: only an update fixes it.
    pub async fn retry(&self) {
        let mut inner = self.inner.lock().await;
        inner.ledger.clear();
        inner.next_try_at = None;
        if matches!(self.state(), HostState::Unavailable { .. }) {
            self.set(HostState::Crashed);
        }
    }

    /// Ends the live host with the pool locked, when the person's stop is allowed to. Runs on it end
    /// as cancelled. A family that is waiting to restart stops waiting; one that gave up stays so.
    async fn stop_locked(&self, inner: &mut Inner, stop_background: bool) -> Result<(), Refusal> {
        inner.next_try_at = None;
        if let Some(connection) = inner.current.clone() {
            let open = connection.open_background();
            if open > 0 && !stop_background {
                return Err(Refusal::BackgroundOpen(open));
            }
            if let Some(stop) = inner.stop.take() {
                stop.cancel();
            }
            let _ = tokio::time::timeout(self.timings.exit_grace * 2, connection.closed()).await;
            inner.current = None;
            self.set(HostState::Stopped);
        } else if matches!(self.state(), HostState::Restarting { .. }) {
            // The restart loop sees the state change and ends.
            self.set(HostState::Stopped);
        }
        Ok(())
    }

    /// The person's stop. A host with a background task still open is left running unless
    /// `stop_background` says to end that work too (the person was told). The next run starts it again.
    pub async fn stop(&self, stop_background: bool) -> Result<(), Refusal> {
        let mut inner = self.inner.lock().await;
        if let HostState::Mismatch { .. } = self.state() {
            return Err(Refusal::VersionMismatch);
        }
        self.stop_locked(&mut inner, stop_background).await
    }

    /// The person's restart: a stop (under the same background rule), then a fresh start. It forgets
    /// every crash and backoff, so a family that gave up (`cant_start`) is tried again. The restart
    /// count stays: it is the tally since the app started. A failed start shows in the state, as any
    /// other does.
    pub async fn restart_now(self: &Arc<Self>, stop_background: bool) -> Result<(), Refusal> {
        let mut inner = self.inner.lock().await;
        if let HostState::Mismatch { .. } = self.state() {
            return Err(Refusal::VersionMismatch);
        }
        self.stop_locked(&mut inner, stop_background).await?;
        inner.ledger.clear();
        if matches!(self.state(), HostState::Unavailable { .. }) {
            self.set(HostState::Crashed);
        }
        if matches!(
            self.state(),
            HostState::Idle | HostState::Stopped | HostState::Crashed
        ) {
            let _ = self.start_locked(&mut inner).await;
        }
        Ok(())
    }

    /// The app is exiting or the family is being turned off: stdin closes so the host exits
    /// (it is killed after the grace), and nothing restarts.
    pub async fn shutdown(&self) {
        self.shutdown.cancel();
        let live = {
            let mut inner = self.inner.lock().await;
            inner.next_try_at = None;
            inner.current.take()
        };
        if let Some(connection) = live {
            let _ = tokio::time::timeout(self.timings.exit_grace * 2, connection.closed()).await;
        }
        if !self.state().is_final() {
            self.set(HostState::Idle);
        }
    }
}
