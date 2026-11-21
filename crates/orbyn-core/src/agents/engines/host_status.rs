//! The engine host's state as the wire contract the UI reads (`GET /api/engine-host`).

use orbyn_contracts::{
    EngineHostCrash, EngineHostErrorKind, EngineHostNotUsed, EngineHostState, EngineHostStatus,
};

use super::EngineManager;
use crate::agents::driver::host::{CrashKind, HostState, HostStatus, NotUsed};

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
