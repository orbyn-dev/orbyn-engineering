//! Starting one run of a background crew by hand (docs/crew/background-engine-start.md, PR 1: the `manual` event). A person presses
//! Run now on an enabled background crew; this opens one run, one lead thread for it, and starts a Crew task through
//! `launch_crew_task` as a Crew-started, Low-priority task, so review capacity can pause it (`capacity_start_check`) and the pause is
//! noted as a held `background` start. Nothing here listens for anything: no CI watch, no schedule, no route.
//!
//! **The lead thread runs no engine.** It exists so the task has a crew and a project, and it is never sent a prompt: the run is
//! the person's to read, not a lead's to answer. A run's id is its lead thread's id, which is also the `crew_id` of every task of
//! that thread, so a task finds its run (and the run its tasks) with no extra column.
//!
//! **What a task's end means for the run.** A task that passed ends the run `needs_review`. A budget stop, a loop-guard stop and an
//! engine error end it `stopped` with that reason. Anything else that ends a task without a person (a cancel, too many denials, text
//! that tried to give orders) or that waits for one ends it `needs_review` with a plain sentence, because the stored reasons
//! (`budget`, `repeated_failure`, `engine_error`, `disabled`) do not name it. A worker turn that produces nothing is an engine error
//! (`docs/engine-run-contract.md`), never a finished run.
//!
//! **No background merge lands without a person.** The run's lead thread is its crew, and the crew's Autopilot mode is pinned `off`
//! before the worker can pass, so the merge queue holds every merge of the run as `waiting` whatever the repository's mode says.
//!
//! **Budget.** The crew's turns and minutes become the brief's budget. Minutes are the task's existing timer, which cancels the worker
//! mid-turn; turns are the worker's runs, checked each time a verification fails (the same place the minutes are), so a run whose last
//! allowed turn did not pass stops with the reason `budget`.

use std::sync::Arc;

use orbyn_contracts as wire;
use orbyn_contracts::{CrewError, ErrorCode, ResultState, StopReason};
use serde_json::json;

use super::background::run_wire;
use super::capacity::{PausedKind, PausedTrigger};
use super::start::{Launch, Links};
use super::{Mode, ModeScope, redact, wire_name};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{BackgroundRow, CrewTask, NewThread, RunEnd};

/// The most top-level entries of a repository a run may write under. Past it the run is given the first of them, which only narrows it.
const WRITE_ENTRIES_MAX: usize = 256;
/// Where a background run never writes: the policy file's folder and the workflows. A person changes those.
const PROTECTED: [&str; 2] = [".orbyn/**", ".github/**"];
/// The largest change a run may make and still pass its own check.
const MAX_FILES: u64 = 50;
const MAX_LINES: u64 = 2000;
/// The longest sentence kept as why a run waits, in characters.
const WHY_MAX: usize = 300;

/// What a template asks for, in the words of the task. Code-owned: no repository text and no event text is part of it.
fn outcome(template: &str, branch: Option<&str>) -> String {
    let on = branch.map_or_else(String::new, |branch| format!(" on branch {branch}"));
    match template {
        "fix_failing_ci" => format!(
            "Find why the latest CI run{on} fails and fix it with the smallest change that makes it pass. Do not weaken or remove a check."
        ),
        "flaky_test" => "Find one test in this repository that fails at random and make it reliable. Do not skip, delete or loosen the test."
            .to_owned(),
        "dependency_bump" => "Update one outdated dependency to its newest compatible version and fix what the update breaks.".to_owned(),
        "security_advisory" => {
            "Find one dependency with a published security advisory and update it to a version that fixes it.".to_owned()
        }
        _ => "Do the work this background crew was made for.".to_owned(),
    }
}

/// The top-level entries of `HEAD` a run may write: a folder as `name/**`, a file by its name. `.git`, `.github` and `.orbyn` are
/// left out, and so is anything the task format cannot name.
async fn write_entries(root: &std::path::Path) -> Result<Vec<String>> {
    let listing = match crate::source_control::git::git_hardened(root, &["ls-tree", "-z", "HEAD"])
        .await
    {
        Ok(listing) => listing,
        Err(_) => {
            // An unborn HEAD is a plain refusal, not a project that cannot be read.
            let unborn =
                crate::source_control::git::git_hardened(root, &["rev-parse", "--verify", "HEAD"])
                    .await
                    .is_err();
            let why = if unborn {
                "The project has nothing committed yet, so there is nothing for a background run to work on."
            } else {
                "The project's files could not be listed."
            };
            return Err(CoreError::Invalid(why.into()));
        }
    };
    let mut entries: Vec<String> = listing
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let text = String::from_utf8_lossy(entry);
            let (facts, name) = text.split_once('\t')?;
            let tree = facts.split(' ').nth(1)? == "tree";
            let pattern = if tree {
                format!("{name}/**")
            } else {
                name.to_owned()
            };
            let hidden = matches!(name, ".git" | ".github" | ".orbyn");
            (!hidden && orbyn_contracts::CrewPathPattern::new(pattern.clone()).is_ok())
                .then_some(pattern)
        })
        .collect();
    entries.sort();
    entries.truncate(WRITE_ENTRIES_MAX);
    if entries.is_empty() {
        return Err(CoreError::Invalid(
            "The project has nothing committed yet, so there is nothing for a background run to work on.".into(),
        ));
    }
    Ok(entries)
}

/// A refusal from the start, as the plain sentence a person reads.
fn refusal(error: CrewError) -> CoreError {
    if error.code == ErrorCode::NotAuthorized {
        CoreError::PolicyDenied(error.message)
    } else {
        CoreError::Invalid(error.message)
    }
}

impl Core {
    /// Starts one run of an enabled background crew by hand: the `manual` event, the same start a listener firing takes with
    /// its own event key. Behind the Crew Preview flag.
    pub async fn crew_background_run_now(
        self: &Arc<Self>,
        id: &str,
    ) -> Result<wire::BackgroundRun> {
        self.crew_background_start(id, "manual").await
    }

    /// Opens one run of an enabled background crew and starts it, whatever event asked for it. `event` names the firing in the
    /// start's `PausedTrigger` key (`<crew>:<event>`) and nothing else: it is never part of the brief, the worker's prompt or any
    /// text a model reads, which stays code-owned exactly as the manual start's is. The run is Low priority and Crew-started, so
    /// review capacity may refuse it (nothing is stored then, and the refusal is a held `background` start); a crew that is off, a
    /// trigger kind nothing can start yet, a crew that already has a run in progress and a project that is not trusted are refused
    /// with a plain reason. Behind the Crew Preview flag.
    pub(crate) async fn crew_background_start(
        self: &Arc<Self>,
        id: &str,
        event: &str,
    ) -> Result<wire::BackgroundRun> {
        self.crew_preview_only()?;
        let crew = self.background_row(id)?;
        self.refuse_background_start(&crew)?;
        let project = self.store.project(&crew.project_id)?;
        if !project.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        let engine = self.background_lead_engine(&crew.project_id).await?;
        let files = write_entries(std::path::Path::new(&project.path)).await?;

        // The lead thread is the run's identity: it is made first and the run takes its id.
        let thread = self.store.create_thread(NewThread {
            project_id: crew.project_id.clone(),
            title: format!("Background crew: {}", crew.name),
            engine: engine.clone(),
            model: None,
            cwd: project.path.clone(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })?;
        let crew_scope = ModeScope::Crew(thread.id.clone());
        if let Err(error) = self.start_background_run_for(&crew, &thread, &crew_scope) {
            self.discard_background_lead(&thread.id, &crew_scope);
            return Err(error);
        }
        let stamper = match self.review_stamper(&thread, &engine) {
            Ok(stamper) => stamper,
            Err(error) => {
                return self.unwind_background_start(&thread.id, &crew_scope, refusal(error));
            }
        };
        let draft = json!({
            "kind": "implement",
            "outcome": outcome(&crew.template, crew.trigger_settings["branch"].as_str()),
            "inputs": [],
            "files": files.iter().map(|path| json!({ "path": path, "mode": "write" })).collect::<Vec<_>>(),
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["The change is made and the checks pass."],
            "budget": { "turns": crew.budget_turns, "minutes": crew.budget_minutes },
        });
        let key_draft = json!({
            "checks": [
                {
                    "id": "chk-scope", "kind": "file_scope", "params": { "only": files },
                    "proves": "Only files the run was allowed to change were changed.",
                },
                {
                    "id": "chk-size", "kind": "diff_size",
                    "params": { "max_files": MAX_FILES, "max_lines": MAX_LINES },
                    "proves": "The change is small enough to read.",
                    "does_not_prove": "That the change is right: a person reads every background change.",
                },
            ],
            "protected_paths": PROTECTED,
        });
        let drafted = stamper.brief(&draft).and_then(|brief| {
            let key = stamper.answer_key(&key_draft, &brief.id)?;
            Ok((brief, key))
        });
        let (brief, key) = match drafted {
            Ok(both) => both,
            Err(error) => {
                return self.unwind_background_start(&thread.id, &crew_scope, refusal(error));
            }
        };
        let launched = self
            .launch_crew_task(Launch {
                lead: &thread,
                now: stamper.now.clone(),
                brief,
                key,
                role: None,
                lane: None,
                lane_keeps_lead: false,
                author_engine: None,
                prefix: "worker",
                prompt: None,
                links: Links {
                    review: None,
                    fix: None,
                    correction_subject: None,
                    crew_started: true,
                    trigger: Some(PausedTrigger {
                        kind: PausedKind::Background,
                        key: format!("{}:{event}", crew.id),
                    }),
                },
            })
            .await;
        // A task that could not be routed or started ended the run itself (it waits for a person). A start refused before any task
        // existed, such as one held by review capacity, left no run: the run is taken back and the reason goes to the person.
        let run = self.store.background_run(&thread.id)?;
        match (launched, run) {
            (Err(error), Some(run)) if run.state == "running" => {
                self.unwind_background_start(&thread.id, &crew_scope, refusal(error))
            }
            (_, Some(run)) => run_wire(run),
            (Err(error), None) => Err(refusal(error)),
            (Ok(_), None) => Err(CoreError::Internal("a background run vanished".into())),
        }
    }

    /// The refusals that need only the stored crew.
    fn refuse_background_start(&self, crew: &BackgroundRow) -> Result<()> {
        if !crew.enabled {
            return Err(CoreError::PolicyDenied(
                "This background crew is off. Turn it on, then start a run.".into(),
            ));
        }
        match crew.trigger_kind.as_str() {
            "ci_failure" | "schedule" => {}
            kind => {
                let words = kind.replace('_', " ");
                return Err(CoreError::PolicyDenied(format!(
                    "A {words} crew cannot be started yet: nothing in Orbyn can start it."
                )));
            }
        }
        if let (Some(newest), _) = self.store.background_standing(&crew.id)?
            && newest.state == "running"
        {
            return Err(CoreError::RunsActive);
        }
        Ok(())
    }

    /// The engine the run's lead thread is made on: the person's default engine when it is usable, else the first usable one. The
    /// lead never runs it; the router still picks the worker's.
    async fn background_lead_engine(&self, project_id: &str) -> Result<String> {
        let machine = self.crew_machine(Some(project_id)).await?;
        let usable = |id: &str| {
            machine
                .views
                .iter()
                .any(|view| view.id.as_str() == id && view.worth_reading())
        };
        machine
            .default_engine
            .clone()
            .filter(|id| usable(id))
            .or_else(|| {
                machine
                    .views
                    .iter()
                    .find(|view| view.worth_reading())
                    .map(|view| view.id.as_str().to_owned())
            })
            .ok_or_else(|| {
                CoreError::EngineUnavailable(
                    "No engine is ready to run a background crew. Install one or sign in.".into(),
                )
            })
    }

    /// Opens the run with the lead thread's id and pins the crew's Autopilot mode `off`, so no merge of the run lands by itself.
    fn start_background_run_for(
        &self,
        crew: &BackgroundRow,
        thread: &orbyn_contracts::Thread,
        scope: &ModeScope,
    ) -> Result<()> {
        self.set_crew_autopilot_mode(scope, Mode::Off)?;
        self.store.start_background_run_as(&crew.id, &thread.id)?;
        Ok(())
    }

    /// Takes back what a refused start made: the run, the pinned mode and the lead thread. Returns the refusal.
    fn unwind_background_start(
        &self,
        thread_id: &str,
        scope: &ModeScope,
        refusal: CoreError,
    ) -> Result<wire::BackgroundRun> {
        if let Err(error) = self
            .store
            .unwind_background_run(thread_id, &redact(&refusal.public_message()))
        {
            tracing::warn!(%error, "could not take back a refused background run");
        }
        self.discard_background_lead(thread_id, scope);
        Err(refusal)
    }

    fn discard_background_lead(&self, thread_id: &str, scope: &ModeScope) {
        if let Err(error) = self.clear_crew_autopilot_mode(scope) {
            tracing::warn!(%error, "could not remove a refused background run's mode");
        }
        if let Err(error) = self.store.delete_thread(thread_id, "crew") {
            tracing::warn!(%error, "could not remove a refused background run's lead thread");
        }
    }

    /// Whether `thread_id` is the lead thread of a background run.
    pub(super) fn crew_background_lead(&self, thread_id: &str) -> bool {
        matches!(self.store.background_run(thread_id), Ok(Some(_)))
    }

    /// Whether a background run has used all the turns it was given: its worker has had `turns` turns.
    pub(super) fn crew_background_out_of_turns(&self, task: &CrewTask, turns: u32) -> bool {
        self.crew_background_lead(&task.crew_id)
            && self
                .store
                .crew_worker_turns(&task.id)
                .is_ok_and(|spent| spent >= turns)
    }

    /// A task ended: the run of a background crew that owns it ends with it. A task of any other crew finds no run and nothing
    /// happens. A failure is logged and never changes how the task ended.
    pub(super) fn crew_background_task_ended(
        &self,
        task: &CrewTask,
        state: ResultState,
        stop: Option<StopReason>,
    ) {
        let (stopped, waits): (Option<wire::BackgroundStopReason>, &str) = match (state, stop) {
            (ResultState::Passed, _) => (None, "The run finished and has work to review."),
            (ResultState::Stopped, Some(StopReason::Budget)) => {
                (Some(wire::BackgroundStopReason::Budget), "")
            }
            (ResultState::Stopped, Some(StopReason::RepeatedFailure)) => {
                (Some(wire::BackgroundStopReason::RepeatedFailure), "")
            }
            (ResultState::Stopped, Some(StopReason::EngineError)) => {
                (Some(wire::BackgroundStopReason::EngineError), "")
            }
            (_, Some(StopReason::TooManyDenials)) => {
                (None, "The run stopped after too many refused actions.")
            }
            (_, Some(StopReason::InjectionSuspected)) => (
                None,
                "The run stopped because it read text that tried to give it orders.",
            ),
            (ResultState::Cancelled, _) => (None, "The run's task was cancelled."),
            _ => (None, "The run was stopped."),
        };
        let end = match &stopped {
            Some(reason) => RunEnd::Stopped(&wire_name(reason)),
            None => RunEnd::NeedsReview(waits),
        };
        self.end_background_run_of(task, end);
    }

    /// A task waits for a person: its run does not stay `running` with nobody at it.
    pub(super) fn crew_background_task_waits(&self, task: &CrewTask, why: &str) {
        let why: String = redact(why).chars().take(WHY_MAX).collect();
        self.end_background_run_of(task, RunEnd::NeedsReview(&why));
    }

    fn end_background_run_of(&self, task: &CrewTask, end: RunEnd<'_>) {
        let Ok(Some(run)) = self.store.background_run(&task.crew_id) else {
            return;
        };
        if run.state != "running" {
            return;
        }
        let turns = self.store.crew_worker_turns(&task.id).unwrap_or(0);
        let minutes = crate::util::parse_time(&task.created_at).map_or(0, |at| {
            u32::try_from((crate::util::unix_now() - at).max(0) / 60).unwrap_or(u32::MAX)
        });
        if let Err(error) =
            self.store
                .end_background_run_spent(&run.id, end, Some((turns, minutes)))
        {
            tracing::warn!(%error, run = %run.id, "could not end a background run");
        }
    }
}
