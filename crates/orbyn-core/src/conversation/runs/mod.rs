//! The run manager: admission, execution and cleanup. Admission is durable
//! before any process exists. The event loop is the single writer of a run's
//! progress, and cleanup runs on every path out of it.

mod shape;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

use orbyn_contracts::{
    AgUiEvent, Attachment, EngineTier, ForcedStop, ModelSelection, PermissionMode, QueuedMessage,
    Run, RunState, SteerState, Thread, ThreadGoal,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::agents::driver::diagnostics;
use crate::agents::driver::{
    EngineDriver, EngineEvent, EngineOutcome, GoalAction, PermissionRequest, RunControl, RunHandle,
    RunKind, RunSpec,
};
use crate::agents::engines::EngineManager;
use crate::agents::selection;
use crate::agents::titles::{self, TitleCall};
use crate::conversation::checkpoints::checkpoint_ref;
use crate::conversation::composer::check_prompt;
use crate::conversation::questions::QuestionDesk;
use crate::error::{CoreError, Result};
use crate::governance::approvals::{ActionAsk, ApprovalGate, RunScope, WriteScope};
use crate::project::workspace::{Workspace, check_target};
use crate::source_control::git::{git, snapshot, text};
use crate::store::{Admission, AuditRecord, LimitStop, Store};
use crate::tools::mcp::{McpBinding, McpService};
use crate::tools::skills::{SkillRoute, SkillService};
use crate::util::new_id;
use shape::EventShaper;

/// Longest a driver gets to stop before its task is aborted, which drops the
/// process owner and kills the tree.
const STOP_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a forced stop lets a stage finish before dropping it.
const FORCE_GRACE: Duration = Duration::from_secs(1);
/// How long a forced stop waits for a run to close before closing it itself.
const FORCE_WAIT: Duration = Duration::from_secs(4);
const FORCE_POLL: Duration = Duration::from_millis(20);
/// A delegated run this quiet is stuck: twice the longest a tool may take.
const SILENCE_LIMIT: Duration = Duration::from_secs(2 * 240);
const EVENT_BUFFER: usize = 1024;
/// How long a process started for a thread's next turn waits to be adopted.
const WARM_IDLE: Duration = Duration::from_secs(5 * 60);

/// Where a thread's run may write: its checkout, and on a library thread never
/// the nested projects. `engine_cwd` is where the engine's relative paths land.
fn write_scope(root: &Path, checkout: &Path, from_checkout: bool) -> WriteScope {
    WriteScope {
        checkout: checkout.to_path_buf(),
        engine_cwd: if from_checkout { checkout } else { root }.to_path_buf(),
        excluded: if checkout == root {
            vec![root.join("repo")]
        } else {
            Vec::new()
        },
    }
}

/// Where a thread's engine process starts for a run, and whether its relative paths
/// then mean the checkout. Claude starts in the checkout, or in the library for a
/// session that began there; Codex's process starts at the root with its turns on the
/// checkout; the rest start in the checkout.
fn engine_start(
    driver: &dyn EngineDriver,
    native: bool,
    root: &Path,
    checkout: &Path,
    session: Option<&str>,
) -> (PathBuf, bool) {
    if let Some(cwd) = driver.start_dir(root, checkout, session) {
        let at_checkout = cwd == checkout;
        return (cwd, at_checkout);
    }
    let cwd = if native { root } else { checkout };
    (cwd.to_path_buf(), driver.resolves_from_checkout())
}

async fn branch_name(checkout: &Path) -> Option<String> {
    let name = git(checkout, &["symbolic-ref", "--short", "-q", "HEAD"])
        .await
        .ok()?;
    Some(text(&name))
}

async fn checkout_revision(checkout: &Path) -> Option<String> {
    let head = git(checkout, &["rev-parse", "HEAD"]).await.ok()?;
    Some(text(&head))
}

/// A worktree's target Git root is its primary clone.
async fn target_root(checkout: &Path) -> PathBuf {
    let common = git(
        checkout,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await;
    common
        .ok()
        .and_then(|path| crate::util::canonicalize(Path::new(&text(&path))).ok())
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| checkout.to_path_buf())
}

struct RunContext<'a> {
    library: &'a Path,
    revision: &'a str,
    dirty: bool,
    target_root: &'a Path,
    checkout: &'a Path,
    branch: Option<&'a str>,
    /// `None` outside Git.
    checkout_revision: Option<&'a str>,
    /// The engine's working directory is the library, not the checkout: a session that
    /// began there before runs started in the checkout.
    runs_at_library: bool,
    library_thread: bool,
    route: &'a SkillRoute,
    /// The engine has the MCP server, and so `skill_search` and `skill_load`.
    skill_tools: bool,
    plan: bool,
    files: &'a [PathBuf],
    handoff: Option<&'a str>,
}

/// The short bootstrap in front of each run's prompt. It leads with what the engine
/// can do, since a first turn that read mostly limits once said it had no files to
/// look at. Nesting does not prove an engine loaded a target's rules, so
/// this tells it to read them.
fn run_context(context: &RunContext<'_>) -> String {
    let library = format!(
        "{} at {}{}",
        context.library.display(),
        context.revision,
        if context.dirty {
            " with local edits"
        } else {
            ""
        },
    );
    let mut text = String::from("<orbyn_context>\n");
    if context.library_thread {
        text.push_str(&format!(
            "Checkout: the shared library, {library}. Leave repo/ alone.\n"
        ));
    } else {
        text.push_str(&format!(
            "Checkout: {} on {}{}.\n",
            context.checkout.display(),
            context
                .branch
                .unwrap_or(if context.checkout_revision.is_some() {
                    "a detached HEAD"
                } else {
                    "no branch"
                }),
            context
                .checkout_revision
                .map_or_else(String::new, |revision| format!(" at {revision}")),
        ));
        if context.target_root != context.checkout {
            text.push_str(&format!(
                "It is a worktree of {}.\n",
                context.target_root.display()
            ));
        }
        text.push_str(&format!(
            "You can read it with your usual file and shell tools, so open its files directly.\n\
             Make changes only in the checkout. The shared library is for reference: {library}.\n\
             Before changing code, read the instructions at the checkout's root and near the \
             files you touch (AGENTS.md, AGENT.md, CLAUDE.md, codex.md, .cursorrules, \
             CONTRIBUTING.md). They specialize shared methods.\n"
        ));
    }
    if !context.library_thread {
        if context.runs_at_library {
            // An older session keeps the folder it began in. Say outright which one is the work.
            text.push_str(&format!(
                "The project you are working on is the checkout above, {}. Your shell starts in the \
                 shared library, which is not the project: do not cd into the library or read it \
                 to answer questions about the project; use absolute paths under the checkout and \
                 run commands from there.\n",
                context.checkout.display()
            ));
        } else {
            text.push_str(&format!(
                "Your working directory is the checkout, {}: that is the project. The library is \
                 for skills only, not the project.\n",
                context.checkout.display()
            ));
        }
    }
    text.push_str(&skills_line(context));
    if context.plan {
        text.push_str("Plan mode: read and propose a plan. Change nothing.\n");
    }
    if !context.files.is_empty() {
        text.push_str("Attached files:\n");
        for file in context.files {
            text.push_str(&format!("- {}\n", file.display()));
        }
    }
    if let Some(handoff) = context.handoff {
        text.push_str("Earlier conversation, carried over from another session:\n");
        text.push_str(handoff);
        text.push('\n');
    }
    text.push_str("</orbyn_context>");
    text
}

/// Names skills only where nothing else can: native discovery and the tools already
/// know the selection, and every name here is read on every first turn.
fn skills_line(context: &RunContext<'_>) -> String {
    let route = context.route;
    let names = route.selection.join(", ");
    if route.selection.is_empty() {
        String::new()
    } else if !route.native && context.skill_tools {
        "Find shared skills with the skill_search and skill_load tools.\n".to_owned()
    } else if !route.native {
        format!("Shared skills, each at skills/<name>/SKILL.md in the library: {names}.\n")
    } else if route.limited {
        format!("Use only these shared skills: {names}.\n")
    } else {
        String::new()
    }
}

/// What the run manager is built from.
pub(crate) struct RunServices {
    pub store: Arc<Store>,
    pub gate: Arc<ApprovalGate>,
    pub questions: Arc<QuestionDesk>,
    pub engines: Arc<EngineManager>,
    pub skills: Arc<SkillService>,
    pub workspace: Arc<Workspace>,
    pub scm_reads: Arc<crate::source_control::scm::read_cache::ScmReads>,
    pub mcp: Arc<McpService>,
    pub base_url: String,
    pub runs_dir: PathBuf,
    /// Where published renders live, so an MCP App page joins its thread's renders.
    pub renders_dir: PathBuf,
    pub setups: Arc<crate::project::project_setup::SetupManager>,
    pub terminals: Arc<crate::host::terminal::TerminalManager>,
}

pub struct RunManager {
    store: Arc<Store>,
    gate: Arc<ApprovalGate>,
    questions: Arc<QuestionDesk>,
    engines: Arc<EngineManager>,
    skills: Arc<SkillService>,
    workspace: Arc<Workspace>,
    scm_reads: Arc<crate::source_control::scm::read_cache::ScmReads>,
    mcp: Arc<McpService>,
    base_url: String,
    runs_dir: PathBuf,
    renders_dir: PathBuf,
    setups: Arc<crate::project::project_setup::SetupManager>,
    /// A live engine console owns its thread's session, so no run is admitted.
    terminals: Arc<crate::host::terminal::TerminalManager>,
    active: Mutex<HashMap<String, ActiveRun>>,
    /// Runs stopped because their parent was. Reporting back would start the parent
    /// again, undoing the Stop.
    stopped_with_parent: Mutex<HashSet<String>>,
    /// Limits engines reported, until their run ends. Unix seconds, when they said.
    limit_signals: Mutex<HashMap<String, Option<i64>>>,
    /// The latest note an MCP App page left for the model, per thread, until the
    /// thread's next turn takes it. Kept in memory: a restart forgets an unsent note.
    app_notes: Mutex<HashMap<String, crate::tools::mcp_apps::AppNote>>,
    /// A taken note's timeline record, per run, written once the run is active.
    notes_sent: Mutex<HashMap<String, AgUiEvent>>,
    /// Threads being rewound. An engine rollback rewrites the session a run would
    /// resume, so no run is admitted until the rewind ends.
    rewinding: Mutex<HashSet<String>>,
    /// Crew Preview: told when a run ends, so a worker's task can follow it. Set once the
    /// core exists, which is built after this manager.
    crew: OnceLock<Weak<crate::app::Core>>,
}

/// Keeps runs off a thread while it is rewound; dropping it lets them in again.
pub(crate) struct RewindHold {
    runs: Arc<RunManager>,
    thread_id: String,
}

impl Drop for RewindHold {
    fn drop(&mut self) {
        self.runs
            .rewinding
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.thread_id);
        self.runs.drain(&self.thread_id);
    }
}

/// How a run is asked to end. `cancel` lets the engine stop itself; `force` ends it
/// now and is for an engine that stopped answering.
#[derive(Clone, Default)]
struct Stops {
    cancel: CancellationToken,
    force: CancellationToken,
}

struct ActiveRun {
    stops: Stops,
    task: Option<JoinHandle<()>>,
    input: Option<mpsc::UnboundedSender<String>>,
    control: Option<mpsc::UnboundedSender<RunControl>>,
}

async fn sleep_until_due(due: Option<tokio::time::Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}

impl RunManager {
    pub(crate) fn new(services: RunServices) -> Self {
        let RunServices {
            store,
            gate,
            questions,
            engines,
            skills,
            workspace,
            scm_reads,
            mcp,
            base_url,
            runs_dir,
            renders_dir,
            setups,
            terminals,
        } = services;
        Self {
            store,
            gate,
            questions,
            engines,
            skills,
            workspace,
            scm_reads,
            mcp,
            base_url,
            runs_dir,
            renders_dir,
            setups,
            terminals,
            active: Mutex::new(HashMap::new()),
            stopped_with_parent: Mutex::new(HashSet::new()),
            limit_signals: Mutex::new(HashMap::new()),
            app_notes: Mutex::new(HashMap::new()),
            notes_sent: Mutex::new(HashMap::new()),
            rewinding: Mutex::new(HashSet::new()),
            crew: OnceLock::new(),
        }
    }

    /// Connects the Crew group, which follows a worker's runs to their end.
    pub(crate) fn bind_crew(&self, core: &Arc<crate::app::Core>) {
        let _ = self.crew.set(Arc::downgrade(core));
    }

    /// Tells the Crew group that a run ended. It looks the thread up itself and does nothing
    /// for a thread that is not a Crew worker.
    pub(super) fn crew_run_ended(&self, thread_id: &str, run_id: &str) {
        let Some(core) = self.crew.get().and_then(Weak::upgrade) else {
            return;
        };
        if !core.crew_enabled() {
            return;
        }
        let (thread_id, run_id) = (thread_id.to_owned(), run_id.to_owned());
        tokio::spawn(async move { core.crew_worker_run_ended(thread_id, run_id).await });
    }
}

mod admission;
mod checkpoints;
mod execute;
mod stops;
mod turn;

#[cfg(test)]
mod tests;
