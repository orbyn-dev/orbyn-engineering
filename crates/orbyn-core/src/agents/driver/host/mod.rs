//! The engine host driver: runs an engine SDK in `orbyn-engine-host` and speaks
//! docs/engine-host-protocol.md to it. Off unless the desktop shell sets `ORBYN_ENGINE_HOST`
//! and the family's own flag (`ORBYN_ENGINE_HOST_CLAUDE=1`) is on; with either missing the
//! Rust drivers run exactly as before.
//!
//! One responsibility per file: `frames` (wire format), `process` (spawn, handshake),
//! `connection` (one live host), `pool` (restarts, state), `state` (pure rules),
//! `options`, `confinement`, `delta`, `outcome` (what a run gets from the host),
//! `run` (one turn), `driver` (the `EngineDriver`).

mod confinement;
mod connection;
mod delta;
mod driver;
mod frames;
mod options;
mod outcome;
mod pool;
mod process;
mod run;
mod state;

#[cfg(test)]
mod support;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

pub use driver::HostDriver;
pub use frames::PROTOCOL;
pub use outcome::HOST_STOPPED;
pub use pool::{HostStatus, Pool, SecretSource};
pub use process::HostSpec;
pub use state::{CrashInfo, CrashKind, ExitRecord, HostState, NotUsed, Timings};

use crate::agents::driver::EngineDriver;
use crate::credentials::{Credentials, DEV_SERVICE};

/// How the service was told to use the host.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// `ORBYN_ENGINE_HOST`: the host binary the desktop shell found.
    pub program: PathBuf,
    /// `ORBYN_ENGINE_HOST_CLAUDE=1`.
    pub claude: bool,
    pub timings: Timings,
}

impl HostConfig {
    /// From the service's environment: `None` when `ORBYN_ENGINE_HOST` is absent or empty.
    pub fn from_env(host: Option<std::ffi::OsString>, claude_flag: Option<&str>) -> Option<Self> {
        let program = host.filter(|path| !path.is_empty())?;
        Some(Self {
            program: PathBuf::from(program),
            // The host reads the same flag and wants exactly `1`.
            claude: claude_flag == Some("1"),
            timings: Timings::default(),
        })
    }
}

/// Intel Macs keep the Rust drivers: the single executable crashes there.
fn platform_supported() -> bool {
    !cfg!(all(target_os = "macos", target_arch = "x86_64"))
}

/// Whether the person opted in to keeping a key for `family`. The setting is not built yet,
/// so it is off: the call below returns no key until a later slice wires it.
fn key_enabled(_family: &str) -> bool {
    false
}

fn secrets_for(family: &str) -> SecretSource {
    let family = family.to_owned();
    Arc::new(move || {
        Credentials::system(DEV_SERVICE)
            .take_for_spawn(&family, key_enabled(&family))
            .ok()
            .flatten()
    })
}

enum Slot {
    Pool(Arc<Pool>),
    NotUsed(&'static str, NotUsed),
}

/// Every family's host state, one pool per family.
pub struct HostHub {
    slots: Vec<Slot>,
}

impl HostHub {
    pub fn new(config: &HostConfig) -> Self {
        let claude = if !platform_supported() {
            Slot::NotUsed("claude", NotUsed::UnsupportedPlatform)
        } else if !config.claude {
            Slot::NotUsed("claude", NotUsed::FlagOff)
        } else {
            Slot::Pool(Pool::new(
                "claude",
                HostSpec {
                    program: config.program.clone(),
                    args: vec!["claude".into()],
                    env: vec![("ORBYN_ENGINE_HOST_CLAUDE".into(), "1".into())],
                    instance: "claude-default".into(),
                },
                config.timings.clone(),
                Some(secrets_for("claude")),
            ))
        };
        Self {
            slots: vec![claude],
        }
    }

    /// `inner` run through the host, when its family uses it; otherwise `inner` unchanged.
    pub fn wrap(
        &self,
        family: &str,
        inner: Arc<dyn EngineDriver>,
        program: String,
    ) -> Arc<dyn EngineDriver> {
        for slot in &self.slots {
            if let Slot::Pool(pool) = slot
                && pool.family == family
            {
                return Arc::new(HostDriver::new(inner, pool.clone(), program));
            }
        }
        inner
    }

    pub async fn statuses(&self) -> Vec<HostStatus> {
        let mut statuses = Vec::new();
        for slot in &self.slots {
            statuses.push(match slot {
                Slot::Pool(pool) => pool.status().await,
                Slot::NotUsed(family, why) => HostStatus {
                    family: (*family).to_owned(),
                    state: HostState::NotUsed(*why),
                    host_version: None,
                    restart_count: 0,
                    last_crash: None,
                },
            });
        }
        statuses
    }

    /// The person asked again after a family gave up.
    pub async fn retry(&self, family: &str) {
        for slot in &self.slots {
            if let Slot::Pool(pool) = slot
                && pool.family == family
            {
                pool.retry().await;
            }
        }
    }

    /// The app is exiting: every host's stdin closes and it is killed after the grace.
    pub async fn shutdown(&self) {
        for slot in &self.slots {
            if let Slot::Pool(pool) = slot {
                pool.shutdown().await;
            }
        }
    }
}
