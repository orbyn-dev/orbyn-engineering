//! Keeps a crew worker's engine process away from the core's own data folder.
//!
//! A worker runs as the same operating-system user as the core, so its shell could read the
//! store, run the `orbyn` command line against the service handle or open another run's MCP
//! address. On macOS the core starts each worker under a profile that denies reading and
//! writing the data folder except the worker's own run folder and the few places it needs,
//! and denies inspecting other processes. Everywhere else no restriction is available and the
//! core refuses to start a worker (hard rule 7: never run with less than was promised).
//!
//! What this does not cover is listed in `docs/crew.md`: the network and the loopback service,
//! the OS keychain, the engine's own login and session folders, and other same-user files.

use std::path::{Path, PathBuf};
use std::process::Command;

use orbyn_contracts::EngineTier;

use super::process::ResolvedCommand;
use crate::error::{CoreError, Result};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
use unsupported as platform;

/// Whether a Codex engine may run a crew worker. Codex starts its own Seatbelt profile for
/// each command, and a profile cannot be started inside another, so until that is checked
/// the core refuses it. Flip this one constant once it is.
pub const CODEX_WORKERS_ALLOWED: bool = false;

/// The one line a person sees when a worker is refused because no restriction can be applied.
pub const UNSUPPORTED_PLATFORM: &str = "Crew runs on macOS in the beta; Windows later.";
const CODEX_REFUSED: &str = "Crew workers on Codex are not available yet.";
const TERMINAL_REFUSED: &str =
    "A terminal engine cannot take a crew task, because Orbyn cannot restrict it.";

/// What one worker process may reach. Every path is canonical, so a link such as macOS's
/// `/var` cannot make a rule miss the folder it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confinement {
    /// The core's data folder: denied, apart from what follows.
    pub data: PathBuf,
    /// The worker's own run folder: read and write.
    pub run_dir: PathBuf,
    /// Places inside `data` the worker may read (the skills library).
    pub allow_read: Vec<PathBuf>,
    /// Places inside `data` the worker may read and write (its own checkout).
    pub allow_write: Vec<PathBuf>,
    /// Places inside `data` the worker may read and run (an installed agent's files).
    pub allow_exec: Vec<PathBuf>,
}

impl Confinement {
    /// Builds the rules for one worker. A path that does not resolve is an error, never a
    /// rule that silently names nothing.
    pub fn new(
        data: &Path,
        run_dir: &Path,
        allow_read: &[&Path],
        allow_write: &[&Path],
    ) -> Result<Self> {
        let data = canonical(data)?;
        let run_dir = canonical(run_dir)?;
        let mut confinement = Self {
            data,
            run_dir,
            allow_read: Vec::new(),
            allow_write: Vec::new(),
            allow_exec: Vec::new(),
        };
        for path in allow_read {
            confinement.allow_read.push(canonical(path)?);
        }
        for path in allow_write {
            confinement.allow_write.push(canonical(path)?);
        }
        Ok(confinement)
    }

    /// These rules for an engine whose executable is `program`: one that lives under
    /// `<data>/acp-agents/<id>` may be read and run from there.
    #[must_use]
    pub fn for_program(&self, program: &Path) -> Self {
        let mut rules = self.clone();
        let agents = self.data.join("acp-agents");
        if let Ok(program) = std::fs::canonicalize(program)
            && let Ok(rest) = program.strip_prefix(&agents)
            && let Some(agent) = rest.components().next()
        {
            rules.allow_exec.push(agents.join(agent));
        }
        rules
    }

    /// Puts the restriction in front of `command`.
    pub fn wrap(&self, command: &mut ResolvedCommand) -> Result<()> {
        platform::wrap(self, command)
    }

    /// What a restricted child must not inherit: the service's data folder variable would
    /// point its `orbyn` command at the folder it cannot read.
    pub fn scrub_env(&self, command: &mut Command) {
        command.env_remove("ORBYN_DATA_DIR");
    }
}

fn canonical(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|error| {
        CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} did not resolve ({error}).",
            path.display()
        ))
    })
}

/// A plain reason a crew worker cannot start on this machine, or `None` when the restriction
/// can be applied. Checked once and kept: it starts a harmless process under the tool.
pub fn unavailable() -> Option<String> {
    platform::unavailable()
}

/// A reason the engine itself cannot take a crew task, whatever the machine.
pub fn engine_refusal(tier: EngineTier, family: &str) -> Option<&'static str> {
    if tier == EngineTier::Terminal {
        return Some(TERMINAL_REFUSED);
    }
    (family == "codex" && !CODEX_WORKERS_ALLOWED).then_some(CODEX_REFUSED)
}

#[cfg(test)]
mod tests;
