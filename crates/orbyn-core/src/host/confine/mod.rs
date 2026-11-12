//! Keeps a crew worker's engine process away from the core's own data folder.
//!
//! A worker runs as the same operating-system user as the core, so its shell could read the
//! store, run the `orbyn` command line against the service handle or open another run's MCP
//! address. On macOS the core starts each worker under a profile that denies reading and
//! writing the data folder except the worker's own run folder and the few places it needs,
//! and stops the data folder or a folder above it being renamed away. Everywhere else no restriction is available and the
//! core refuses to start a worker (hard rule 7: never run with less than was promised).
//!
//! For a Claude worker it also denies reading and writing Claude's saved conversations
//! (`<config>/projects`) except the worker's own session, so the lead's transcript is closed.
//!
//! What this does not cover is listed in `docs/crew.md`: the network and the loopback service,
//! the OS keychain, the engine's own login folders, other engines' saved conversations, and
//! other same-user files.

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
/// the core refuses it. Flip this one constant once it is, and once Codex's own saved
/// conversations are closed: its `thread/start` writes rollout files under `sessions/` only on
/// the first turn (so a worker's own file cannot be named at launch), and 0.166 keeps threads
/// and logs in SQLite files under its home too, which a path rule cannot split per thread.
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
    /// The engine's saved conversations, when the engine is one Orbyn can close them for.
    pub transcripts: Option<Transcripts>,
}

/// Where an engine keeps its saved conversations, and the one session the worker owns. Every
/// other conversation under `root` is denied, including one the engine creates after the
/// worker starts, because the rule is on the folder and the allowance names only this session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transcripts {
    /// The folder of per-project folders (Claude's `<config>/projects`), canonical.
    pub root: PathBuf,
    /// The worker's own session id: its `<id>.jsonl` and `<id>/` stay readable and writable
    /// in whichever project folder the engine keys them under. None for an engine that has no
    /// business in this folder: the whole folder is then closed.
    pub session: Option<String>,
    /// True when the session does not exist yet and the engine is told to use this id
    /// (`--session-id`), so the allowance can name it before it is written.
    pub fresh: bool,
    /// The engine's prompt history across sessions: denied, the engine runs without it.
    pub history: PathBuf,
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
        let run_dir = inside(&data, canonical(run_dir)?)?;
        let mut confinement = Self {
            data,
            run_dir,
            allow_read: Vec::new(),
            allow_write: Vec::new(),
            allow_exec: Vec::new(),
            transcripts: None,
        };
        for path in allow_read {
            let path = inside(&confinement.data, canonical(path)?)?;
            confinement.allow_read.push(path);
        }
        for path in allow_write {
            let path = inside(&confinement.data, canonical(path)?)?;
            confinement.allow_write.push(path);
        }
        Ok(confinement)
    }

    /// Adds the denial of Claude's saved conversations. `projects` is the folder Claude keeps
    /// them in (`agents::usage::history_root`), `session` the worker's own id from an earlier
    /// turn, or `None` for its first turn, which then starts under a new id. A folder that
    /// does not resolve, or an id the engine could not have made, is an error: a rule that
    /// names nothing would leave the lead's transcript open.
    pub fn with_claude_transcripts(self, projects: &Path, session: Option<&str>) -> Result<Self> {
        self.with_transcripts(Transcripts::claude(projects, session)?)
    }

    /// Closes Claude's saved conversations to a worker that is not Claude, with no allowance:
    /// an engine of another family has no session there. A folder that does not exist yet is
    /// named as given (it holds nothing to read, and the rules still stop a worker making it
    /// a link); one that exists resolves as for a Claude worker.
    pub fn with_closed_claude_folder(self, projects: &Path) -> Result<Self> {
        self.with_transcripts(Transcripts::closed(projects)?)
    }

    fn with_transcripts(mut self, transcripts: Transcripts) -> Result<Self> {
        if self.data.starts_with(&transcripts.root) || transcripts.root.starts_with(&self.data) {
            return Err(CoreError::EngineUnavailable(format!(
                "A crew worker could not be restricted: {} overlaps Orbyn's data folder.",
                transcripts.root.display()
            )));
        }
        self.transcripts = Some(transcripts);
        Ok(self)
    }

    /// The id a first turn must start under, when the rules chose one.
    #[must_use]
    pub fn fresh_session(&self) -> Option<&str> {
        self.transcripts
            .as_ref()
            .filter(|transcripts| transcripts.fresh)
            .and_then(|transcripts| transcripts.session.as_deref())
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

/// An allowance is applied after the data folder's denial and overrides it, so one that
/// equals or contains the data folder would open all of it. Refused, never narrowed.
fn inside(data: &Path, allowed: PathBuf) -> Result<PathBuf> {
    if data.starts_with(&allowed) {
        return Err(CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} contains Orbyn's data folder.",
            allowed.display()
        )));
    }
    Ok(allowed)
}

impl Transcripts {
    fn claude(projects: &Path, session: Option<&str>) -> Result<Self> {
        let config = canonical(&config_of(projects)?)?;
        let (session, fresh) = match session {
            Some(id) if usable_session(id) => (id.to_owned(), false),
            Some(_) => {
                return Err(CoreError::EngineUnavailable(
                    "A crew worker could not be restricted: its Claude session id is not usable."
                        .into(),
                ));
            }
            None => (new_session()?, true),
        };
        Ok(Self {
            root: not_a_link(&config, "projects")?,
            session: Some(session),
            fresh,
            history: not_a_link(&config, "history.jsonl")?,
        })
    }

    fn closed(projects: &Path) -> Result<Self> {
        let config = config_of(projects)?;
        let config = match std::fs::canonicalize(&config) {
            Ok(found) => found,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => config,
            Err(_) => canonical(&config)?,
        };
        Ok(Self {
            root: not_a_link(&config, "projects")?,
            session: None,
            fresh: false,
            history: not_a_link(&config, "history.jsonl")?,
        })
    }
}

/// The configuration folder above `projects`; relative would mean something else to the
/// engine, which starts elsewhere.
fn config_of(projects: &Path) -> Result<PathBuf> {
    projects
        .parent()
        .filter(|config| config.is_absolute())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            CoreError::EngineUnavailable(format!(
                "A crew worker could not be restricted: {} is not an absolute folder.",
                projects.display()
            ))
        })
}

/// `config/name`, refused when it is a link. A rule by path cannot follow a link that the
/// worker (or anyone) later swaps, and the folders above a link's target are not protected, so
/// a linked folder or file is not restricted at all: no worker starts.
fn not_a_link(config: &Path, name: &str) -> Result<PathBuf> {
    let path = config.join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} is a link.",
            path.display()
        ))),
        Ok(_) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path),
        Err(error) => Err(CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} did not resolve ({error}).",
            path.display()
        ))),
    }
}

/// The characters Claude's own ids use. The profile puts the id into a pattern, so nothing
/// else may pass.
fn usable_session(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A random version 4 UUID, the form `claude --session-id` takes.
fn new_session() -> Result<String> {
    let mut bytes = crate::util::random_bytes::<16>()?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = crate::util::hex(&bytes);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
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
