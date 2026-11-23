//! Starting one run of a background crew by hand (docs/crew/background-engine-start.md, PR 1: the `manual` event). A person presses
//! Run now on an enabled background crew; this opens one run, one lead thread for it, and starts a Crew task through
//! `launch_crew_task` as a Crew-started, Low-priority task, so review capacity can pause it (`capacity_start_check`) and the pause is
//! noted as a held `background` start. Nothing here listens for anything: no CI watch, no schedule, no route.
//!
//! **The lead thread runs no engine.** It exists so the task has a crew and a project. Nothing in the crew's own paths sends it a
//! prompt (the retry and the lead notices refuse a background lead), and a person's message to it is refused at admission: the run is
//! the person's to read, not a lead's to answer. A run's id is its lead thread's id, which is also the `crew_id` of every task of
//! that thread, so a task finds its run (and the run its tasks) with no extra column. A run that never started has no lead thread.
//!
//! **A start the disk guard refuses keeps its row.** Every other refusal before a task exists takes its run and lead thread back and
//! returns an error. A disk refusal is different: the run stays as the crew's history, ended `stopped` with `reason_code`
//! `disk_low` and the disk sentence, waiting for nobody, and its lead thread is removed with it. The person still gets the error.
//!
//! **What a task's end means for the run.** A task that passed ends the run `needs_review`. A budget stop, a loop-guard stop and an
//! engine error end it `stopped` with that reason. Anything else that ends a task without a person (a cancel, too many denials, text
//! that tried to give orders) or that waits for one ends it `needs_review` with a plain sentence, because the stored reasons
//! (`budget`, `repeated_failure`, `engine_error`, `disabled`) do not name it. A worker turn that produces nothing is an engine error
//! (`docs/engine-run-contract.md`), never a finished run.
//!
//! **No background merge lands without a person.** The run's lead thread is its crew, and `crew_autopilot_mode` answers `off` for a
//! crew that is a background run's lead, so the merge queue holds every merge of the run as `waiting` whatever the repository's mode
//! says and whatever a stored row for the crew's scope holds. It is a rule of the code, not a stored row.
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
use super::{redact, wire_name};
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
/// The `reason_code` of a run the disk guard refused: the volume holding the project was under the person's floor, so no worker ran.
const DISK_LOW: &str = "disk_low";

/// What a template asks for, in the words of the task. Code-owned: no repository text and no event text is part of it.
fn outcome(template: &str, branch: Option<&str>) -> Result<String> {
    let on = branch.map_or_else(String::new, |branch| format!(" on branch {branch}"));
    Ok(match template {
        "fix_failing_ci" => format!(
            "Find why the latest CI run{on} fails and fix it with the smallest change that makes it pass. Do not weaken or remove a check."
        ),
        "flaky_test" => "Find one test in this repository that fails at random and make it reliable. Do not skip, delete or loosen the test."
            .to_owned(),
        "dependency_bump" => "Update one outdated dependency to its newest compatible version and fix what the update breaks.".to_owned(),
        "security_advisory" => {
            "Find one dependency with a published security advisory and update it to a version that fixes it.".to_owned()
        }
        _ => {
            return Err(CoreError::Invalid(
                "This crew's template is not one a run can start. Make the crew again from a template.".into(),
            ));
        }
    })
}

/// The templates a run knows how to ask for. Any other is refused, not run with a vague brief.
fn known_template(template: &str) -> bool {
    matches!(
        template,
        "fix_failing_ci" | "flaky_test" | "dependency_bump" | "security_advisory"
    )
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
                "The project has no files committed yet, so there is nothing for a background run to work on."
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
            "The project has no files committed yet, so there is nothing for a background run to work on.".into(),
        ));
    }
    Ok(entries)
}

/// A refusal from the start, as the plain sentence a person reads.
fn refusal(error: CrewError) -> CoreError {
    if matches!(error.code, ErrorCode::NotAuthorized | ErrorCode::DiskLow) {
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
        actor: &str,
    ) -> Result<wire::BackgroundRun> {
        self.crew_background_start(id, "manual", actor).await
    }

    /// Opens one run of an enabled background crew and starts it, whatever event asked for it. `event` names the firing in the
    /// start's `PausedTrigger` key (`<crew>:<event>`) and nothing else: it is never part of the brief, the worker's prompt or any
    /// text a model reads, which stays code-owned exactly as the manual start's is. `actor` is written on the run's audit rows and is
    /// always a code-owned constant or the person's own actor, never event text. The run is Low priority and Crew-started, so
    /// review capacity may refuse it (nothing is stored then, and the refusal is a held `background` start); a crew that is off, a
    /// trigger kind nothing can start yet, a crew that already has a run in progress and a project that is not trusted are refused
    /// with a plain reason. Behind the Crew Preview flag.
    pub(crate) async fn crew_background_start(
        self: &Arc<Self>,
        id: &str,
        event: &str,
        actor: &str,
    ) -> Result<wire::BackgroundRun> {
        self.crew_preview_only()?;
        let crew = self.background_row(id)?;
        self.refuse_background_start(&crew)?;
        let branch = match crew.trigger_settings["branch"].as_str() {
            None => None,
            Some(name) if orbyn_contracts::CrewBranch::new(name).is_ok() => Some(name),
            Some(_) => {
                return Err(CoreError::Invalid(
                    "This crew's branch is not a branch name a run can use. Change the branch, then start a run.".into(),
                ));
            }
        };
        let project = self.store.project(&crew.project_id)?;
        if !project.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        let outcome = outcome(&crew.template, branch)?;
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
        if let Err(error) = self
            .store
            .start_background_run_as(&crew.id, &thread.id, actor)
        {
            // No run row exists yet or it is still taskless, so this fails closed whatever the cleanup does. The person is owed the
            // refusal, not a store error from the cleanup.
            if let Err(cleanup) = self.store.discard_unopened_background_lead(&thread.id) {
                tracing::error!(%cleanup, run = %thread.id, "could not take back a background start that failed to open");
            }
            return Err(error);
        }
        let stamper = match self.review_stamper(&thread, &engine) {
            Ok(stamper) => stamper,
            Err(error) => {
                return self.unwind_background_start(&thread.id, refusal(error));
            }
        };
        let draft = json!({
            "kind": "implement",
            "outcome": outcome,
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
                return self.unwind_background_start(&thread.id, refusal(error));
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
        // existed, such as one held by review capacity, left no run: the run is taken back and the reason goes to the person as an
        // error. A refusal after a task exists holds the task and ends the run `needs_review` with the reason; it returns that run
        // like the route-refused path does, because the visible outcome is the same: a run waiting for a person, not an error.
        let mut run = self.store.background_run(&thread.id)?;
        // The person turned the crew off while the task was being made: the run is already `disabled`, and the worker that has just
        // started is stopped with it. A run that ended for any other reason leaves its task to the person.
        if run.as_ref().is_some_and(|run| {
            run.state == "stopped" && run.stop_reason.as_deref() == Some("disabled")
        }) {
            self.cancel_background_tasks(&thread.id).await;
            run = self.store.background_run(&thread.id)?;
        }
        match (launched, run) {
            (Err(error), Some(run)) if run.state == "running" => {
                // A disk refusal never deletes its run: the stopped row is the only history of the start that never happened.
                // Nothing of the task was stored (the gate sits before it), so the lead thread goes and the row keeps its id.
                // Every other refusal before a task still takes the run and the thread back.
                if error.code == ErrorCode::DiskLow {
                    let why = redact(&error.message);
                    self.store
                        .end_background_run_not_started(&thread.id, DISK_LOW, &why, "crew")?;
                    return Err(refusal(error));
                }
                self.unwind_background_start(&thread.id, refusal(error))
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
        if !known_template(&crew.template) {
            return Err(CoreError::Invalid(
                "This crew's template is not one a run can start. Make the crew again from a template.".into(),
            ));
        }
        match crew.trigger_kind.as_str() {
            "ci_failure" | "schedule" => {}
            "issue_label" => {
                return Err(CoreError::PolicyDenied(
                    "An issue label crew cannot run yet: Orbyn cannot see new issue labels.".into(),
                ));
            }
            "watch" => {
                return Err(CoreError::PolicyDenied(
                    "A watch crew cannot run yet: Orbyn runs its command from the schedule, not on demand."
                        .into(),
                ));
            }
            kind => {
                let words = kind.replace('_', " ");
                return Err(CoreError::PolicyDenied(format!(
                    "A {words} crew cannot run yet: Orbyn has nothing that can start it."
                )));
            }
        }
        if let (Some(newest), _) = self.store.background_standing(&crew.id)?
            && newest.state == "running"
        {
            return Err(CoreError::PolicyDenied(
                "This crew already has a run in progress.".into(),
            ));
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

    /// Takes back a taskless start (run and lead thread, in one commit) and returns the refusal as an error, or retains a
    /// task-backed start for review: its tasks are held and the retained run is returned. An error of the store unwind is returned
    /// as it is, since the run is still `running` and the caller must know. What follows the commit is best effort and only logged:
    /// each held task gives up its leases (a run that ended `needs_review` leaves none showing as granted, as `end_crew_task` does) and gets a `NeedsPerson` result with the reason
    /// unless it already has one. A task with no run row (that insert is what failed) cannot be given a result, so it has none and
    /// the run's `review_reason` carries the reason.
    pub(super) fn unwind_background_start(
        &self,
        thread_id: &str,
        refusal: CoreError,
    ) -> Result<wire::BackgroundRun> {
        let why = redact(&refusal.public_message());
        let unwound = self.store.unwind_background_run(thread_id, &why)?;
        if unwound.discarded {
            return Err(refusal);
        }
        for task in unwound.held {
            self.settle_held_background_task(task, &why);
        }
        match self.store.background_run(thread_id)? {
            Some(run) => run_wire(run),
            None => Err(refusal),
        }
    }

    /// What follows the commit that held a background run's task for a person, best effort and only logged: the task gives up its
    /// leases (a run that ended `needs_review` leaves none showing as granted, as `end_crew_task` does) and gets a `NeedsPerson`
    /// result with the reason unless it already has one. Used by a refused start and by startup recovery.
    pub(crate) fn settle_held_background_task(&self, task: CrewTask, why: &str) {
        if let Err(error) = self.release_crew_leases(&task.id) {
            tracing::warn!(%error, task = %task.id, "could not release a held background task's leases");
        }
        let has_result = self
            .store
            .crew_results(&task.id)
            .map_or(true, |results| !results.is_empty());
        if has_result {
            return;
        }
        let task = self
            .store
            .crew_task(&task.id)
            .ok()
            .flatten()
            .unwrap_or(task);
        if let Err(error) = self.record_result(&task, ResultState::NeedsPerson, None, why, None) {
            tracing::warn!(%error, task = %task.id, "could not write a held background task's result");
        }
    }

    /// Queues a notice for a crew's lead thread. A background run's lead runs no engine, so the notice is refused there instead of
    /// starting one; every path that writes to a lead's queue goes through here or checks `crew_background_lead` first.
    pub(super) fn enqueue_for_lead(&self, lead_thread_id: &str, text: &str) -> Result<()> {
        if self.crew_background_lead(lead_thread_id) {
            return Err(CoreError::CrewTaskBusy(
                "A background run's lead thread takes no messages.".into(),
            ));
        }
        self.runs.enqueue_crew_note(lead_thread_id, text, &[])?;
        Ok(())
    }

    /// Cancels every task of a run that has not ended, as the person who turned the crew off. A failure is logged: the run already
    /// ended, and a cancel that cannot finish is the task's own watchdog's to settle.
    pub(super) async fn cancel_background_tasks(self: &Arc<Self>, run_id: &str) {
        let tasks = match self.store.crew_tasks_in_crew(run_id) {
            Ok(tasks) => tasks,
            Err(error) => {
                tracing::warn!(%error, "could not list a stopped background run's tasks");
                return;
            }
        };
        for (task, _) in tasks {
            if super::lifecycle::is_final(&task.state) {
                continue;
            }
            if let Err(error) = self
                .crew_cancel_task(
                    &task.id,
                    Some("The person turned this background crew off."),
                    "user",
                )
                .await
            {
                tracing::warn!(%error, task = %task.id, "could not cancel a background run's task");
            }
        }
    }

    /// What a task's run spent: the worker's turns (the runs of its worker thread) and the wall minutes since the task began.
    pub(super) fn crew_background_run_spent(&self, task: &CrewTask) -> (u32, u32) {
        let turns = self.store.crew_worker_turns(&task.id).unwrap_or(0);
        let minutes = crate::util::parse_time(&task.created_at).map_or(0, |at| {
            u32::try_from((crate::util::unix_now() - at).max(0) / 60).unwrap_or(u32::MAX)
        });
        (turns, minutes)
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
        let (turns, minutes) = self.crew_background_run_spent(task);
        if let Err(error) =
            self.store
                .end_background_run_spent(&run.id, end, Some((turns, minutes)))
        {
            tracing::warn!(%error, run = %run.id, "could not end a background run");
        }
    }
}
