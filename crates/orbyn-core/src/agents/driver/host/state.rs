//! What a family's host is doing, and the rules that decide when to restart it. Everything
//! here is pure: time comes in as an argument, so the tests do not wait.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Why a family does not use the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotUsed {
    /// The family's env flag is off: the Rust driver runs it.
    FlagOff,
    /// The host does not run here (Intel Macs keep the Rust drivers).
    UnsupportedPlatform,
}

/// One family's host, for the UI. `Idle` is not in the protocol's list: the host is not
/// running and starts on the next run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostState {
    /// Never started; it starts on the first run.
    Idle,
    /// Stopped for being idle; it starts again on the next run.
    Stopped,
    /// The person asked again after it crashed; it starts on the next run.
    Crashed,
    Starting,
    Ready,
    Restarting {
        attempt: u32,
        last_error: String,
    },
    Unavailable {
        reason: String,
    },
    /// The host speaks another protocol than this app. Final: it never restarts.
    Mismatch {
        app: u64,
        host: u64,
    },
    NotUsed(NotUsed),
}

impl HostState {
    /// States the core never leaves by itself.
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            Self::Unavailable { .. } | Self::Mismatch { .. } | Self::NotUsed(_)
        )
    }

    /// A plain sentence for a state that stops runs; `None` when the host works or will.
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Mismatch { app, host } => Some(format!(
                "The engine host does not match this app version (app protocol {app}, host protocol {host}). Update Orbyn."
            )),
            Self::Unavailable { reason } => Some(format!(
                "The engine host is unavailable after repeated failures: {reason}"
            )),
            Self::Restarting { .. } => {
                Some("The engine host stopped and is restarting.".to_owned())
            }
            Self::Idle
            | Self::Stopped
            | Self::Crashed
            | Self::Starting
            | Self::Ready
            | Self::NotUsed(_) => None,
        }
    }

    /// What the person can do about it, when there is one thing.
    pub fn action(&self) -> Option<&'static str> {
        match self {
            Self::Mismatch { .. } => Some("Update Orbyn"),
            _ => None,
        }
    }
}

/// How a host process ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExitRecord {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

impl ExitRecord {
    pub fn of(status: std::process::ExitStatus) -> Self {
        #[cfg(unix)]
        let signal = std::os::unix::process::ExitStatusExt::signal(&status);
        #[cfg(not(unix))]
        let signal = None;
        Self {
            code: status.code(),
            signal,
        }
    }

    /// For the engine log and the state's detail.
    pub fn describe(&self) -> String {
        match (self.code, self.signal) {
            (Some(code), _) => format!("exit code {code}"),
            (None, Some(signal)) => format!("signal {signal}"),
            (None, None) => "no exit status".to_owned(),
        }
    }
}

/// How a host failed, from a closed set the UI can word without parsing text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashKind {
    Exited,
    ClosedOutput,
    Stalled,
    Malformed,
    Oversize,
    HelloTimeout,
    StartFailed,
}

/// A host that failed: what happened and its last words, already redacted and capped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashInfo {
    pub kind: CrashKind,
    pub message: String,
    pub exit: Option<ExitRecord>,
    pub stderr_tail: String,
}

/// The longest stderr tail kept, in characters.
pub const STDERR_TAIL_CHARS: usize = 1024;

impl CrashInfo {
    pub fn new(kind: CrashKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            exit: None,
            stderr_tail: String::new(),
        }
    }

    /// With the host's stderr: credential-looking lines are dropped and the rest is capped.
    pub fn with_stderr(mut self, raw: &str) -> Self {
        self.stderr_tail = redacted_tail(raw);
        self
    }
}

/// Drops terminal codes and credential-looking lines, then keeps the last
/// `STDERR_TAIL_CHARS` characters.
pub fn redacted_tail(raw: &str) -> String {
    let clean = crate::agents::driver::diagnostics::clean(raw);
    let count = clean.chars().count();
    if count <= STDERR_TAIL_CHARS {
        return clean;
    }
    clean.chars().skip(count - STDERR_TAIL_CHARS).collect()
}

/// Every time and count the core applies. `Default` is the protocol's values; tests shrink them.
#[derive(Clone, Debug)]
pub struct Timings {
    /// `hello` after a restart.
    pub hello: Duration,
    /// `hello` on the first spawn of this app run, which a signed binary's first launch may delay.
    pub first_hello: Duration,
    /// How long the host may take to answer `init`.
    pub init: Duration,
    pub backoff_base: Duration,
    pub backoff_cap: Duration,
    /// Uptime after which the backoff starts over.
    pub backoff_reset: Duration,
    /// Restarts inside `restart_window` after which the family is unavailable.
    pub max_restarts: usize,
    pub restart_window: Duration,
    /// The gap between pings and how long one may go unanswered; only while a run is open.
    pub ping_every: Duration,
    pub ping_misses: u32,
    /// Malformed frames inside `malformed_window` that count as a crash.
    pub max_malformed: usize,
    pub malformed_window: Duration,
    /// Closing stdin, then killing.
    pub exit_grace: Duration,
    /// How long a host with no run and no background work stays up. `None` never stops it.
    pub idle_stop: Option<Duration>,
    /// How long a cancelled run waits for its `runDone` before it ends as cancelled anyway.
    pub cancel_grace: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            hello: Duration::from_secs(10),
            first_hello: Duration::from_secs(30),
            init: Duration::from_secs(10),
            backoff_base: Duration::from_secs(1),
            backoff_cap: Duration::from_secs(30),
            backoff_reset: Duration::from_secs(60),
            max_restarts: 5,
            restart_window: Duration::from_secs(5 * 60),
            ping_every: Duration::from_secs(5),
            ping_misses: 3,
            max_malformed: 10,
            malformed_window: Duration::from_secs(60),
            exit_grace: Duration::from_secs(5),
            idle_stop: Some(Duration::from_secs(10 * 60)),
            cancel_grace: Duration::from_secs(15),
        }
    }
}

/// What to do after a crash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restart {
    After(Duration),
    GiveUp,
}

/// Backoff and the five-in-five rule.
#[derive(Debug, Default)]
pub struct Ledger {
    streak: u32,
    restarts: VecDeque<Instant>,
}

impl Ledger {
    /// A crash at `now` of a host that had been up for `uptime` (zero when it never came up).
    /// The first restart waits `backoff_base`, each next one doubles it up to `backoff_cap`,
    /// and a host that stayed up `backoff_reset` starts the count over. After `max_restarts`
    /// restarts inside `restart_window`, the next crash gives up.
    pub fn crashed(&mut self, now: Instant, uptime: Duration, timings: &Timings) -> Restart {
        if uptime >= timings.backoff_reset {
            self.streak = 0;
        }
        while self
            .restarts
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) > timings.restart_window)
        {
            self.restarts.pop_front();
        }
        if self.restarts.len() >= timings.max_restarts {
            return Restart::GiveUp;
        }
        let wait = timings
            .backoff_base
            .saturating_mul(1u32.checked_shl(self.streak).unwrap_or(u32::MAX))
            .min(timings.backoff_cap);
        self.streak = self.streak.saturating_add(1);
        self.restarts.push_back(now);
        Restart::After(wait)
    }

    /// How many restarts are counted so far.
    pub fn restarts(&self) -> usize {
        self.restarts.len()
    }

    /// The person asked again: nothing counts against the host any more.
    pub fn clear(&mut self) {
        self.streak = 0;
        self.restarts.clear();
    }
}

/// Counts malformed frames inside a sliding window.
#[derive(Debug, Default)]
pub struct MalformedWindow {
    seen: VecDeque<Instant>,
}

impl MalformedWindow {
    /// Records one at `now`; true when the window now holds `max` or more, which is a crash.
    pub fn record(&mut self, now: Instant, timings: &Timings) -> bool {
        while self
            .seen
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) > timings.malformed_window)
        {
            self.seen.pop_front();
        }
        self.seen.push_back(now);
        self.seen.len() >= timings.max_malformed
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
