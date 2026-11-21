//! One pool per family (per instance where an SDK needs it): the host process every run of
//! that family shares, its state, and the restart policy. The pool starts the host on the
//! first run, restarts it after a crash with backoff, and gives up after repeated failures
//! until the person asks again. The idle timeout lives in the connection; the pool only
//! notices that it ended.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;

use super::connection::{ClosedReason, Connection};
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
}

struct Inner {
    current: Option<Arc<Connection>>,
    ledger: Ledger,
    spawned_before: bool,
    host_version: Option<String>,
    restart_count: u32,
    last_crash: Option<(CrashInfo, i64)>,
}

pub struct Pool {
    pub family: String,
    spec: HostSpec,
    timings: Timings,
    secrets: Option<SecretSource>,
    state: watch::Sender<HostState>,
    inner: Mutex<Inner>,
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
                ledger: Ledger::default(),
                spawned_before: false,
                host_version: None,
                restart_count: 0,
                last_crash: None,
            }),
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
        HostStatus {
            family: self.family.clone(),
            state: self.state(),
            host_version: inner.host_version.clone(),
            restart_count: inner.restart_count,
            last_crash: inner.last_crash.clone(),
        }
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
        self.set(HostState::Starting);
        let first = !std::mem::replace(&mut inner.spawned_before, true);
        match self.connect(first).await {
            Ok(connection) => {
                self.adopt(&mut inner, connection);
                Ok(())
            }
            Err(ConnectError::Mismatch { app, host }) => {
                self.set(HostState::Mismatch { app, host });
                Err(Unavailable(
                    HostState::Mismatch { app, host }
                        .problem()
                        .unwrap_or_default(),
                ))
            }
            Err(ConnectError::Failed(error)) => {
                let message = format!("The engine host could not start: {}", error.message);
                if let Some(wait) = self.after_crash(&mut inner, error, Duration::ZERO) {
                    self.spawn_restart(wait);
                }
                Err(Unavailable(message))
            }
        }
    }

    async fn connect(&self, first: bool) -> Result<Arc<Connection>, ConnectError> {
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
        Ok(Connection::start(
            transport,
            ready,
            id,
            self.timings.clone(),
            self.shutdown.child_token(),
        ))
    }

    fn adopt(self: &Arc<Self>, inner: &mut Inner, connection: Arc<Connection>) {
        inner.host_version = Some(connection.host_version.clone());
        inner.current = Some(connection.clone());
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
        match inner.ledger.crashed(Instant::now(), uptime, &self.timings) {
            Restart::GiveUp => {
                self.set(HostState::Unavailable { reason: error });
                None
            }
            Restart::After(wait) => {
                inner.restart_count = inner.restart_count.saturating_add(1);
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
        match &reason {
            ClosedReason::Idle | ClosedReason::Shutdown => {
                if !self.state().is_final() {
                    self.set(if reason == ClosedReason::Idle {
                        HostState::Stopped
                    } else {
                        HostState::Idle
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
                Ok(connection) => {
                    self.adopt(&mut inner, connection);
                    return;
                }
                Err(ConnectError::Mismatch { app, host }) => {
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
        if matches!(self.state(), HostState::Unavailable { .. }) {
            self.set(HostState::Crashed);
        }
    }

    /// The app is exiting or the family is being turned off: stdin closes so the host exits
    /// (it is killed after the grace), and nothing restarts.
    pub async fn shutdown(&self) {
        self.shutdown.cancel();
        let live = self.inner.lock().await.current.take();
        if let Some(connection) = live {
            let _ = tokio::time::timeout(self.timings.exit_grace * 2, connection.closed()).await;
        }
        if !self.state().is_final() {
            self.set(HostState::Idle);
        }
    }
}
