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
//! A Codex worker gets its own Codex home under the data folder, holding only a copy of the
//! API-key login. The real Codex home (`CODEX_HOME` or `~/.codex`) is a closed folder, so a
//! worker cannot read the person's own sessions or credentials there.
//!
//! Whatever the platform, the worker's environment is scrubbed of the data folder and of the
//! `orbyn` command line ([`Confinement::scrub_env`]): no inherited name keeps the folder, and
//! the search path keeps no folder that offers the command. That covers the route a shell takes
//! by name; it does not stop a worker that already knows an absolute path to a binary, which is
//! what the data folder's denial is for.
//!
//! What this does not cover is listed in `docs/crew.md`: the network and the loopback service,
//! the OS keychain, the engine's own login folders, other engines' saved conversations, and
//! other same-user files.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use orbyn_contracts::EngineTier;

use super::process::ResolvedCommand;
use crate::error::{CoreError, Result};

mod codex;
#[cfg(all(test, target_os = "macos"))]
pub(crate) use codex::home_path;
pub(crate) use codex::{remove_codex_home, sweep_codex_homes};
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
use unsupported as platform;

/// Whether a Codex engine may run a crew worker. While false, both the router and the run
/// start refuse Codex workers. The design is built and tested behind this constant: a
/// confined Codex worker gets its own home per task (`<data>/crew/codex-homes/<task>`), the
/// real Codex home is closed in the profile, and Codex's own sandbox is turned off inside
/// Orbyn's (it cannot nest). Flip this one line only after the real-Mac probe passes.
pub const CODEX_WORKERS_ALLOWED: bool = false;

/// The one line a person sees when a worker is refused because no restriction can be applied.
pub const UNSUPPORTED_PLATFORM: &str = "Crew runs on macOS in the beta; Windows later.";
pub(crate) const CODEX_REFUSED: &str = "Crew workers on Codex are not available yet.";
const TERMINAL_REFUSED: &str =
    "A terminal engine cannot take a crew task, because Orbyn cannot restrict it.";
/// The service's own variable for its data folder, and the program a worker's search path must
/// not offer: the `orbyn` command line.
const DATA_DIR_VAR: &str = "ORBYN_DATA_DIR";
const CLI_NAMES: &[&str] = &["orbyn", "orbyn.exe"];

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
    /// Other folders closed whole, such as the real Codex home. Denied after every allowance,
    /// and every folder above them is protected from a rename, as for the data folder.
    pub closed: Vec<PathBuf>,
    /// The worker's own Codex home, when this is a Codex worker: inside `data`, writable, and
    /// the only Codex home the engine may see.
    pub codex_home: Option<PathBuf>,
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
    /// The project folder under `root` for the worker's start folder, which the core makes
    /// before the engine starts so the worker never has to create anything there.
    pub folder: Option<PathBuf>,
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
            closed: Vec::new(),
            codex_home: None,
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
    /// turn, or `None` for its first turn, which then starts under a new id, and `start` the
    /// folder the engine starts in. This makes `projects` and the project folder for `start`
    /// (private to the person) so the worker may create nothing there but its own session.
    /// A folder that does not resolve or goes through a link, a start folder whose Claude
    /// name cannot be told with confidence, or an id the engine could not have made, is an
    /// error: a rule that names nothing would leave the lead's transcript open.
    pub fn with_claude_transcripts(
        self,
        projects: &Path,
        session: Option<&str>,
        start: &Path,
    ) -> Result<Self> {
        let transcripts = Transcripts::claude(projects, session, start)?;
        let rules = self.with_transcripts(transcripts)?;
        if let Some(transcripts) = &rules.transcripts {
            make_private(&transcripts.root)?;
            if let Some(folder) = &transcripts.folder {
                make_private(folder)?;
            }
        }
        Ok(rules)
    }

    /// Closes Claude's saved conversations to a worker that is not Claude, with no allowance:
    /// an engine of another family has no session there. A folder that does not exist yet is
    /// named as given (it holds nothing to read, and the rules still stop a worker making it
    /// a link); one that exists resolves as for a Claude worker.
    pub fn with_closed_claude_folder(self, projects: &Path) -> Result<Self> {
        self.with_transcripts(Transcripts::closed(projects)?)
    }

    /// Closes another folder whole. The real Codex home is one: the worker's own home is
    /// elsewhere, so nothing there needs an allowance.
    pub fn with_closed_folder(mut self, folder: &Path) -> Result<Self> {
        let folder = canonical(folder)?;
        if self.data.starts_with(&folder) || folder.starts_with(&self.data) {
            return Err(CoreError::EngineUnavailable(format!(
                "A crew worker could not be restricted: {} overlaps Orbyn's data folder.",
                folder.display()
            )));
        }
        if !self.closed.contains(&folder) {
            self.closed.push(folder);
        }
        Ok(self)
    }

    /// Gives a Codex worker its own home under `data`, copies the API-key login into it, and
    /// closes the real Codex home. The auth rule runs before any folder is made, so a
    /// ChatGPT sign-in leaves only the refusal.
    pub fn with_codex_worker_home(self, task: &str) -> Result<Self> {
        let (real, others) = codex::real_codex_homes()?;
        let mut rules = self;
        for folder in &others {
            rules = rules.with_closed_folder(folder)?;
        }
        rules.with_codex_worker_home_at(task, &real)
    }

    /// [`Self::with_codex_worker_home`] with the real home given, for tests and for a caller
    /// that already resolved it.
    pub fn with_codex_worker_home_at(mut self, task: &str, real_home: &Path) -> Result<Self> {
        // Validate the closed folder before the auth copy, so a real home inside the data
        // folder leaves no home behind.
        self = self.with_closed_folder(real_home)?;
        let home = codex::prepare_home(&self.data, task, real_home)?;
        if !self.allow_write.contains(&home) {
            self.allow_write.push(home.clone());
        }
        codex::check_private_home(&self.data, &home, &self.closed)?;
        self.codex_home = Some(home);
        Ok(self)
    }

    /// The private Codex home this worker may use. A Codex worker never starts without one,
    /// and a home that escaped its private folder is an error, never a run.
    pub fn codex_home(&self) -> Result<&Path> {
        let home = self.codex_home.as_deref().ok_or_else(|| {
            CoreError::EngineUnavailable(
                "A Codex worker cannot start without its private Codex home.".into(),
            )
        })?;
        codex::check_private_home(&self.data, home, &self.closed)?;
        Ok(home)
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

    /// Puts the escape-check proof's restriction in front of `command`: every rule a worker gets, and in addition no network, no
    /// launchd job, no clipboard and writes only in the run folder (`docs/crew/escapes.md`). Refused where nothing can restrict.
    pub fn wrap_proof(&self, command: &mut ResolvedCommand) -> Result<()> {
        platform::wrap_proof(self, command)
    }

    /// What a restricted child must not inherit, and what its search path may not offer.
    ///
    /// The service's own data-folder variable would point the worker's `orbyn` command at the
    /// folder it cannot read, and any other name whose value is that path hands the worker the
    /// same route, so the value decides and not the name. The search path keeps only absolute
    /// folders outside the data folder that offer no `orbyn` program, so a worker's shell finds
    /// neither the app's folder nor the command line by name. It is not a boundary against a
    /// worker that already knows an absolute path to the binary: the macOS profile and the data
    /// folder's denial are what stop that (`docs/crew.md`). Run it after the run's own variables
    /// are set, so one of those cannot put a route back.
    pub fn scrub_env(&self, command: &mut Command) {
        // The folder as the rules compare it, resolved once for every name and search entry.
        let folder = comparable(&self.data);
        command.env_remove(DATA_DIR_VAR);
        let mut named: Vec<OsString> = Vec::new();
        for (name, value) in std::env::vars_os() {
            if names_data(&name, &value, &folder) {
                named.push(name);
            }
        }
        for (name, value) in command.get_envs() {
            if let Some(value) = value
                && names_data(name, value, &folder)
            {
                named.push(name.to_os_string());
            }
        }
        for name in named {
            command.env_remove(name);
        }
        let inherited = command
            .get_envs()
            .filter_map(|(name, value)| value.map(|value| (name, value)))
            .find(|(name, _)| is_path_name(name))
            .map(|(_, value)| value.to_os_string())
            .or_else(|| std::env::var_os("PATH"));
        let kept = worker_path(inherited.as_deref(), &folder);
        match std::env::join_paths(&kept)
            .ok()
            .filter(|path| !path.is_empty())
        {
            Some(path) => command.env("PATH", path),
            // No folder is left to search, which is what the rules say; a child with no search
            // path has nothing to find rather than a system default that may hold the command.
            None => command.env_remove("PATH"),
        };
    }
}

/// Whether an inherited `name` and `value` hand the worker the data folder. The search path is not decided
/// here: it is a list of folders, and one entry naming the folder does not spoil the others.
fn names_data(name: &OsStr, value: &OsStr, folder: &str) -> bool {
    !is_path_name(name) && mentions(value, folder)
}

/// Whether `name` is the search-path variable. Environment names are case-insensitive on Windows, where
/// `Path` is the same one, and case-sensitive everywhere else, where `Path` is a variable of its own that
/// hands over whatever it names like any other.
fn is_path_name(name: &OsStr) -> bool {
    if cfg!(windows) {
        name.eq_ignore_ascii_case("PATH")
    } else {
        name == "PATH"
    }
}

/// The worker's search path: the absolute folders that are outside the data folder (`folder`) and
/// offer no `orbyn` program. A relative or empty entry means the child's own start folder, which
/// these rules cannot name, so it goes too.
fn worker_path(path: Option<&OsStr>, folder: &str) -> Vec<PathBuf> {
    let Some(path) = path else {
        return Vec::new();
    };
    std::env::split_paths(path)
        .filter(|entry| entry.is_absolute())
        .filter(|entry| !mentions(entry.as_os_str(), folder))
        .filter(|entry| !offers_cli(entry))
        .collect()
}

/// Whether `value` hands over `data`: the folder itself, or a file or folder inside it. The value
/// is compared as text after resolving the spellings that name the same folder (a link above it,
/// the Windows verbatim prefix, case), because a variable can name it in any shape.
fn mentions(value: &OsStr, folder: &str) -> bool {
    if comparable_text(&value.to_string_lossy()).contains(folder) {
        return true;
    }
    std::env::split_paths(value)
        .filter(|entry| entry.is_absolute())
        .any(|entry| comparable(&entry).contains(folder))
}

/// Whether `folder` offers the `orbyn` command line. A folder that does loses its other programs
/// for the worker too: the rules cannot take one name out of a search folder, and the documented
/// install folder (`~/.local/bin`) holds launchers.
fn offers_cli(folder: &Path) -> bool {
    CLI_NAMES.iter().any(|name| folder.join(name).is_file())
}

/// A path written so that two spellings of the same folder compare equal: resolved when it
/// exists, `/` separators, no Windows verbatim prefix, and lowercase on Windows.
fn comparable(path: &Path) -> String {
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    comparable_text(&real.to_string_lossy())
}

fn comparable_text(text: &str) -> String {
    let text = text.replace('\\', "/");
    let text = text.strip_prefix("//?/").unwrap_or(&text).to_owned();
    if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text
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
    // The skills-local area holds the skills a worker later reads through the skills tools: a worker that could write it could
    // plant what a later run loads. Refused for the area and anything inside it.
    if allowed.starts_with(data.join(crate::tools::local_skills::AREA_DIR)) {
        return Err(CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} is Orbyn's skills-local folder or inside it.",
            allowed.display()
        )));
    }
    Ok(allowed)
}

impl Transcripts {
    fn claude(projects: &Path, session: Option<&str>, start: &Path) -> Result<Self> {
        let config = unlinked(&config_of(projects)?)?;
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
            folder: Some(config.join("projects").join(project_folder(start)?)),
        })
    }

    fn closed(projects: &Path) -> Result<Self> {
        let config = config_of(projects)?;
        let config = match std::fs::canonicalize(&config) {
            Ok(_) => unlinked(&config)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Nothing there yet; the part that exists must not be a link either.
                if let Some(existing) = config.ancestors().find(|a| a.exists()) {
                    unlinked(existing)?;
                }
                config
            }
            Err(_) => canonical(&config)?,
        };
        Ok(Self {
            root: not_a_link(&config, "projects")?,
            session: None,
            fresh: false,
            history: not_a_link(&config, "history.jsonl")?,
            folder: None,
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

/// `config` when it is already its own canonical path. A link in front of it (the folder
/// itself or any folder above) could be swapped later, and the rules protect only the folders
/// above the resolved path, so such a worker is refused, not followed.
fn unlinked(config: &Path) -> Result<PathBuf> {
    let real = canonical(config)?;
    if real != config {
        return Err(CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} goes through a link.",
            config.display()
        )));
    }
    Ok(real)
}

/// Claude's folder name for a start folder: the real path with every character that is not an
/// ASCII letter or digit turned into `-`. Seen with Claude 2.1.318 for ASCII paths below the
/// length where Claude shortens the name; any other path is refused, never guessed.
fn project_folder(start: &Path) -> Result<String> {
    let real = canonical(start)?;
    match real.to_str() {
        Some(text) if text.is_ascii() && text.len() < 200 => {
            Ok(crate::agents::claude_history::project_folder_name(text))
        }
        _ => Err(CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: Claude's folder name for {} is not certain.",
            real.display()
        ))),
    }
}

/// Makes a folder private to the person if it is not there; one that is there must be a
/// folder, not a link.
fn make_private(path: &Path) -> Result<()> {
    let failed = |error: &dyn std::fmt::Display| {
        CoreError::EngineUnavailable(format!(
            "A crew worker could not be restricted: {} could not be prepared ({error}).",
            path.display()
        ))
    };
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Ok(()),
        Ok(_) => Err(failed(&"not a folder")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            builder.create(path).map_err(|error| failed(&error))
        }
        Err(error) => Err(failed(&error)),
    }
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

/// Why the escape-check proof cannot be confined here, or `None` when it can: [`unavailable`], and on macOS also a `HOME` that
/// resolves, since the proof's credential-folder denials are built from it and must never be dropped.
pub fn proof_unavailable() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        unavailable().or_else(platform::proof_unavailable)
    }
    #[cfg(not(target_os = "macos"))]
    {
        unavailable()
    }
}

/// A reason the engine itself cannot take a crew task, whatever the machine.
pub fn engine_refusal(tier: EngineTier, family: &str) -> Option<&'static str> {
    if tier == EngineTier::Terminal {
        return Some(TERMINAL_REFUSED);
    }
    (family == "codex" && !CODEX_WORKERS_ALLOWED).then_some(CODEX_REFUSED)
}

/// Why a crew worker cannot run, whichever check said so.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerRefusal {
    pub reason: String,
    /// The whole machine cannot restrict a worker, so no engine would do.
    pub machine: bool,
}

impl WorkerRefusal {
    /// One plain answer for a person: which engine cannot run Crew workers (or that none can),
    /// why, and what to do instead.
    pub fn plain(&self, engine: &str) -> String {
        if self.machine {
            format!(
                "Crew can't run workers on this machine. {} Do this work in a normal chat for now.",
                self.reason
            )
        } else {
            format!(
                "{engine} can't run Crew workers. {} Choose another engine for this crew, or do the work in a normal chat.",
                self.reason
            )
        }
    }
}

/// The single check that decides whether this engine may run a crew worker on this machine:
/// the machine's restriction first, then the engine itself. The run start and the router both
/// ask it, so they cannot disagree. Starts a harmless process once (see [`unavailable`]), so
/// call it off the async threads.
pub fn worker_refusal(tier: EngineTier, family: &str) -> Option<WorkerRefusal> {
    if let Some(reason) = unavailable() {
        return Some(WorkerRefusal {
            reason,
            machine: true,
        });
    }
    engine_refusal(tier, family).map(|reason| WorkerRefusal {
        reason: reason.to_owned(),
        machine: false,
    })
}

#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod env_tests;
#[cfg(test)]
mod tests;
