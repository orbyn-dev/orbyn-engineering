//! The engine host's state as the wire contract the UI reads (`GET /api/engine-host`).

use orbyn_contracts::{
    EngineHostCrash, EngineHostErrorKind, EngineHostNotUsed, EngineHostRunError, EngineHostState,
    EngineHostStatus, EngineHostThread, EngineHostThreadState,
};

use super::EngineManager;
use crate::agents::driver::host::{
    ControlError, CrashKind, ErrorKind, HostState, HostStatus, NotUsed,
};
use crate::error::CoreError;
use crate::store::AuditRecord;

/// Why the person's stop or restart of a host did nothing.
#[derive(Debug)]
pub enum HostControlFailure {
    Control(ControlError),
    /// The audit row could not be written, so nothing was done.
    Audit(CoreError),
}

fn error_kind(kind: CrashKind) -> EngineHostErrorKind {
    match kind {
        CrashKind::Exited => EngineHostErrorKind::Exited,
        CrashKind::ClosedOutput => EngineHostErrorKind::ClosedOutput,
        CrashKind::Stalled => EngineHostErrorKind::Stalled,
        CrashKind::Malformed => EngineHostErrorKind::Malformed,
        CrashKind::Oversize => EngineHostErrorKind::Oversize,
        CrashKind::HelloTimeout => EngineHostErrorKind::HelloTimeout,
        CrashKind::StartFailed => EngineHostErrorKind::StartFailed,
    }
}

fn run_error(kind: ErrorKind) -> Option<EngineHostRunError> {
    Some(match kind {
        ErrorKind::Auth => EngineHostRunError::Auth,
        ErrorKind::RateLimit => EngineHostRunError::RateLimit,
        ErrorKind::Network => EngineHostRunError::Network,
        ErrorKind::InvalidRequest => EngineHostRunError::InvalidRequest,
        ErrorKind::Unsupported => EngineHostRunError::Unsupported,
        ErrorKind::Cancelled => EngineHostRunError::Cancelled,
        ErrorKind::Engine => EngineHostRunError::Engine,
        ErrorKind::Host => EngineHostRunError::Host,
        ErrorKind::SessionLost => EngineHostRunError::SessionLost,
        ErrorKind::Unknown => return None,
    })
}

/// One family's status for the wire. Never carries an environment, arguments or a key: the
/// crash holds a message and the already redacted, capped stderr tail.
pub fn contract(status: &HostStatus) -> EngineHostStatus {
    let state = match &status.state {
        HostState::Idle => EngineHostState::NotStarted,
        HostState::Stopped => EngineHostState::Stopped,
        HostState::Crashed => EngineHostState::Crashed,
        HostState::Starting => EngineHostState::Starting,
        HostState::Ready => EngineHostState::Running,
        HostState::Restarting { .. } => EngineHostState::Restarting,
        HostState::Unavailable { .. } => EngineHostState::CantStart,
        HostState::Mismatch { .. } => EngineHostState::VersionMismatch,
        HostState::NotUsed(_) => EngineHostState::NotUsed,
    };
    let (protocol_app, protocol_host) = match &status.state {
        HostState::Mismatch { app, host } => (u32::try_from(*app).ok(), u32::try_from(*host).ok()),
        _ => (None, None),
    };
    let not_used = match &status.state {
        HostState::NotUsed(NotUsed::FlagOff) => Some(EngineHostNotUsed::FlagOff),
        HostState::NotUsed(NotUsed::UnsupportedPlatform) => {
            Some(EngineHostNotUsed::UnsupportedPlatform)
        }
        _ => None,
    };
    EngineHostStatus {
        family: status.family.clone(),
        state,
        host_version: status.host_version.clone(),
        restart_count: status.restart_count,
        last_crash: status
            .last_crash
            .as_ref()
            .map(|(crash, at)| EngineHostCrash {
                exit_code: crash.exit.and_then(|exit| exit.code),
                signal: crash.exit.and_then(|exit| exit.signal),
                at: crate::conversation::automations::rfc3339(*at).unwrap_or_default(),
                error_kind: error_kind(crash.kind),
                message: crash.message.clone(),
                stderr_tail: crash.stderr_tail.clone(),
            }),
        protocol_app,
        protocol_host,
        not_used,
        detail: status.state.problem(),
        action: status.state.action().map(str::to_owned),
        threads: (!status.threads.is_empty()).then(|| {
            status
                .threads
                .iter()
                .map(|thread| EngineHostThread {
                    thread_id: thread.clone(),
                    family: status.family.clone(),
                    state: EngineHostThreadState::Running,
                })
                .collect()
        }),
        last_error_kind: status.last_error.and_then(run_error),
    }
}

impl EngineManager {
    /// Each family's engine host. Empty when the service does not use the host.
    pub async fn host_status(&self) -> Vec<EngineHostStatus> {
        match &self.host {
            Some(hub) => hub.statuses().await.iter().map(contract).collect(),
            None => Vec::new(),
        }
    }

    /// Hands the host the core that owns the person's keys, so a spawn asks it for the engine's key.
    pub fn bind_host_core(&self, core: &std::sync::Arc<crate::app::Core>) {
        if let Some(hub) = &self.host {
            hub.bind(core);
        }
    }

    /// The person's stop (`restart` false) or restart of a family's host. The audit row is written first, so
    /// it always precedes the effect, and a failed write stops it. The row names the person's choice about
    /// background work and carries nothing the host said.
    pub async fn host_control(
        &self,
        family: &str,
        restart: bool,
        stop_background: bool,
        actor: &str,
    ) -> Result<EngineHostStatus, HostControlFailure> {
        let Some(hub) = self.host.as_ref().filter(|hub| hub.knows(family)) else {
            return Err(HostControlFailure::Control(ControlError::UnknownFamily));
        };
        self.store
            .append_audit(&AuditRecord {
                actor: actor.to_owned(),
                action: if restart {
                    "engine_host.restart"
                } else {
                    "engine_host.stop"
                }
                .into(),
                target: family.to_owned(),
                decision: None,
                source: "user".into(),
                thread_id: None,
                run_id: None,
                detail: serde_json::json!({ "stop_background": stop_background }),
            })
            .map_err(HostControlFailure::Audit)?;
        let status = if restart {
            hub.restart(family, stop_background).await
        } else {
            hub.stop(family, stop_background).await
        };
        status
            .map(|status| contract(&status))
            .map_err(HostControlFailure::Control)
    }

    /// The person asked again after a family gave up.
    pub async fn host_retry(&self, family: &str) {
        if let Some(hub) = &self.host {
            hub.retry(family).await;
        }
    }

    /// The service is exiting: every host's stdin closes.
    pub async fn host_shutdown(&self) {
        if let Some(hub) = &self.host {
            hub.shutdown().await;
        }
    }
}

#[cfg(test)]
#[path = "host_status_tests.rs"]
mod tests;
