//! The engine host driver: runs an engine SDK in `orbyn-engine-host` and speaks
//! docs/engine-host-protocol.md to it. Off unless the desktop shell sets `ORBYN_ENGINE_HOST`
//! and the family's own flag (`ORBYN_ENGINE_HOST_CLAUDE=1`, `ORBYN_ENGINE_HOST_CURSOR=1`) is on; with either missing the
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
use std::sync::{Arc, OnceLock, Weak};

pub use driver::HostDriver;
pub use frames::{ErrorKind, PROTOCOL};
pub use outcome::HOST_STOPPED;
pub use pool::{HostStatus, Pool, Refusal, SecretSource};
pub use process::HostSpec;
pub use state::{CrashInfo, CrashKind, ExitRecord, HostState, NotUsed, Timings};

use crate::agents::driver::EngineDriver;
use crate::app::Core;

/// How the service was told to use the host.
#[derive(Clone, Debug)]
pub struct HostConfig {
    /// `ORBYN_ENGINE_HOST`: the host binary the desktop shell found.
    pub program: PathBuf,
    /// `ORBYN_ENGINE_HOST_CLAUDE=1`.
    pub claude: bool,
    /// `ORBYN_ENGINE_HOST_CURSOR=1`.
    pub cursor: bool,
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
            cursor: false,
            timings: Timings::default(),
        })
    }

    /// Also reads `ORBYN_ENGINE_HOST_CURSOR`, which wants exactly `1` as the Claude flag does.
    #[must_use]
    pub fn with_cursor(mut self, cursor_flag: Option<&str>) -> Self {
        self.cursor = cursor_flag == Some("1");
        self
    }
}

/// Intel Macs keep the Rust drivers: the single executable crashes there.
fn platform_supported() -> bool {
    !cfg!(all(target_os = "macos", target_arch = "x86_64"))
}

/// Where the hub finds the core that owns the person's keys. The core is shared after the engines
/// are built, so it is bound once, later; until then no key is handed out.
pub type CoreCell = Arc<OnceLock<Weak<Core>>>;

/// The secret source of a family: every spawn asks the core, which honours the person's per-engine
/// opt-in (`Core::engine_spawn_secret`). It is the only place a spawn's key comes from. A core that is
/// not bound, an engine the allowlist does not name, an opt-in that is off and a keychain that refuses
/// all mean no key, never a fallback.
pub fn secrets_from(core: CoreCell, family: &str) -> SecretSource {
    let family = family.to_owned();
    Arc::new(move || {
        let core = core.get()?.upgrade()?;
        core.engine_spawn_secret(&family).ok().flatten()
    })
}

/// Why the person's stop or restart reached no host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlError {
    /// No family of this name.
    UnknownFamily,
    Refused(Refusal),
}

enum Slot {
    Pool(Arc<Pool>),
    NotUsed(&'static str, NotUsed),
}

/// Every family's host state, one pool per family.
pub struct HostHub {
    slots: Vec<Slot>,
    core: CoreCell,
}

impl HostHub {
    pub fn new(config: &HostConfig) -> Self {
        let core = CoreCell::default();
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
                Some(secrets_from(core.clone(), "claude")),
            ))
        };
        let mut slots = vec![claude];
        // Off means no slot at all: nothing about Cursor changes until the flag is on.
        if config.cursor {
            slots.push(if platform_supported() {
                Slot::Pool(Pool::new(
                    "cursor",
                    HostSpec {
                        program: config.program.clone(),
                        args: vec!["cursor".into()],
                        env: vec![("ORBYN_ENGINE_HOST_CURSOR".into(), "1".into())],
                        instance: "cursor-default".into(),
                    },
                    config.timings.clone(),
                    // The key is `CURSOR_API_KEY` in the host's spawn environment, from the core and no other place.
                    Some(secrets_from(core.clone(), "cursor")),
                ))
            } else {
                Slot::NotUsed("cursor", NotUsed::UnsupportedPlatform)
            });
        }
        Self { slots, core }
    }

    /// Hands the hub the core that owns the person's keys. Called once, when the core is shared.
    pub fn bind(&self, core: &Arc<Core>) {
        let _ = self.core.set(Arc::downgrade(core));
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
                    threads: Vec::new(),
                    last_error: None,
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

    /// The pool of `family`, or why the person's stop or restart cannot reach one.
    fn pool_of(&self, family: &str) -> Result<&Arc<Pool>, ControlError> {
        for slot in &self.slots {
            match slot {
                Slot::Pool(pool) if pool.family == family => return Ok(pool),
                Slot::NotUsed(name, _) if *name == family => {
                    return Err(ControlError::Refused(Refusal::NotUsed));
                }
                _ => {}
            }
        }
        Err(ControlError::UnknownFamily)
    }

    /// Whether `family` is one of the hub's (running on the host or not).
    pub fn knows(&self, family: &str) -> bool {
        !matches!(self.pool_of(family), Err(ControlError::UnknownFamily))
    }

    /// The person stopped a family's host (see [`Pool::stop`]).
    pub async fn stop(
        &self,
        family: &str,
        stop_background: bool,
    ) -> Result<HostStatus, ControlError> {
        let pool = self.pool_of(family)?;
        pool.stop(stop_background)
            .await
            .map_err(ControlError::Refused)?;
        Ok(pool.status().await)
    }

    /// The person restarted a family's host (see [`Pool::restart_now`]).
    pub async fn restart(
        &self,
        family: &str,
        stop_background: bool,
    ) -> Result<HostStatus, ControlError> {
        let pool = self.pool_of(family)?;
        pool.restart_now(stop_background)
            .await
            .map_err(ControlError::Refused)?;
        Ok(pool.status().await)
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
