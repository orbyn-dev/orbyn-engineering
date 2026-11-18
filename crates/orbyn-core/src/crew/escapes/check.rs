//! Create a check (slice 4k, PR 4; docs/crew/escapes.md, "Create a check"): from a confirmed escape a person asks Crew to propose a
//! test that would have caught it, accepts the proposal, and the check goes through the proof and the merge queue.
//!
//! - **Authority.** `propose`, `accept`, `retry` and `withdraw` are a person's alone: `Person` routes, listed in
//!   `crew::changes::STATE_CHANGING` (an agent's request is refused by path), no tool, CLI command or timer calls them. **`accept` is the
//!   only code that starts a worker**; it writes the audit row and `writing` first (architecture rule 6) and starts the task after. A
//!   proposal is data: it writes no task. Nothing here confirms an escape, removes a lane or edits `.orbyn/crew.json`.
//! - **The worker** is an ordinary crew task started through [`Core::launch_crew_task`], the entry point `crew_start` and the review
//!   fixes use, so the router (refusals, the same-provider default) and the capacity gate apply. The brief is in [`super::check_brief`].
//! - **Only test files.** When the task passes, the core reads the worker's committed change from git and judges every path against the
//!   test-file globs in [`super::check_paths`]: only added test files, the proposed one among them. Anything else makes the check `failed`
//!   with the reason; the proof never runs on it.
//! - **The proof** runs the person's command twice in throwaway detached worktrees of the project's repository: on the escaped merge
//!   with the new files laid over it (it must fail) and on the revert or fix with them laid over it (it must pass). Only both together
//!   prove it. A check that passes on both is refused (`failed`, "passes on both"); a command that could not run is not a failure of
//!   the test and is `failed` too, with that reason. The proof stores two commit ids and two booleans, never output.
//! - **The merge queue.** A proven check is queued through [`Core::crew_merge_request`], the same entry the Review queue's merge route
//!   uses, so its own risk tier, holds, owed lanes and the person's required CI policy apply as for any change. When the queue refuses
//!   (no CI policy set, no remote) the check stays `proven` with the reason and the person queues it from the Review queue like any
//!   change. Lanes are never removed; merging is not authorised here.
//! - **Recovery.** The worker's end calls [`Core::spawn_escape_check_ended`]; the service's tick calls [`Core::crew_escape_check_reconcile`],
//!   which also finishes a check whose task ended while the core was down and records the merge of a proven one.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use orbyn_contracts as wire;
use orbyn_contracts::{CrewError, EscapeCheckState, ResultState};
use serde_json::Value;

use super::check_brief::drafts;
use super::check_paths::{self, Change};
use super::view::row_wire;
use crate::app::Core;
use crate::crew::autopilot::valid_id;
use crate::crew::background::text;
use crate::crew::start::{Launch, Links};
use crate::crew::{MergeOperation, MergeRequest, refuse_secret};
use crate::error::{CoreError, Result};
use crate::source_control::git::{git_hardened, git_hardened_capped};
use crate::store::{CheckFinish, CheckProposal, EscapeCheckRecord, EscapeRecord};
use crate::util::unix_now;

/// The longest `asserts` and `run`, in characters.
const TEXT_MAX: usize = 300;
/// The largest new test file the proof lays over a commit.
const FILE_MAX: usize = 256 * 1024;
/// How long one side of the proof may run.
const PROOF_SECONDS: u64 = 10 * 60;

fn is_hex(text: &str, min: usize) -> bool {
    (min..=40).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

fn crew_error(error: CrewError) -> CoreError {
    CoreError::Invalid(error.message)
}

fn driving() -> &'static Mutex<HashSet<String>> {
    static SET: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(Mutex::default)
}

/// One plain sentence for where a check stands.
fn line(check: &EscapeCheckRecord) -> String {
    let path = &check.path;
    match check.state {
        EscapeCheckState::Proposed => {
            format!("Proposed: add {path}. Nothing runs until a person accepts.")
        }
        EscapeCheckState::Writing => format!("A worker is writing {path}."),
        EscapeCheckState::Proven => format!(
            "Proven: {path} fails on the escaped merge and passes on the fix. It goes through the merge queue like any change."
        ),
        EscapeCheckState::Failed => match &check.proof {
            Some(proof) if !proof.fails_on_escape && proof.passes_on_fix => format!(
                "Refused: {path} passes on the escaped merge too, so it proves nothing. Retry or withdraw."
            ),
            Some(proof) if proof.fails_on_escape && !proof.passes_on_fix => {
                format!("Refused: {path} fails on the fix too. Retry or withdraw.")
            }
            Some(_) => format!(
                "Refused: {path} passes on the escaped merge and fails on the fix, the wrong way round. Retry or withdraw."
            ),
            None => format!("Failed: {path} was not proven. Retry or withdraw."),
        },
        EscapeCheckState::Withdrawn => "Withdrawn by a person.".into(),
        EscapeCheckState::Merged => format!("Merged: {path} is in the repository."),
    }
}

pub(super) fn check_wire(check: &EscapeCheckRecord) -> wire::EscapeCheckRow {
    wire::EscapeCheckRow {
        state: check.state,
        path: crate::crew::redact(&check.path),
        asserts: crate::crew::redact(&check.asserts),
        run: crate::crew::redact(&check.run),
        fix_commit: check.fix_commit.clone(),
        task_id: check.task_id.clone(),
        proof: check.proof.clone(),
        note: check.note.as_deref().map(crate::crew::redact),
        attempts: check.attempts,
        line: line(check),
        proposed_unix: check.proposed_unix,
        updated_unix: check.updated_unix,
    }
}

fn answer(escape: &EscapeRecord, check: &EscapeCheckRecord) -> wire::EscapeCheckAnswer {
    wire::EscapeCheckAnswer {
        escape: row_wire(escape),
        check: check_wire(check),
    }
}

/// A `git rev-parse` of `rev` as a commit in `repo`: its full id, or `None` when it is not one.
async fn commit_of(repo: &Path, rev: &str) -> Option<String> {
    if !is_hex(rev, 7) {
        return None;
    }
    let out = git_hardened(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .await
    .ok()?;
    let id = String::from_utf8_lossy(&out).trim().to_owned();
    is_hex(&id, 40).then_some(id)
}

/// The worker's committed change, read from git and judged against the test-file rule ([`check_paths`]): the new files and their
/// bytes, or the plain reason the change is refused. Only added test files, the proposed one among them, nothing else; an error only
/// when git could not be used.
pub(crate) async fn escape_check_change(
    repo: &Path,
    base: &str,
    commit: &str,
    wanted: &str,
) -> Result<std::result::Result<Vec<(String, Vec<u8>)>, String>> {
    let raw = git_hardened_capped(
        repo,
        &[
            "diff",
            "--raw",
            "-z",
            "--no-renames",
            "--no-abbrev",
            base,
            commit,
        ],
        64 * 1024,
    )
    .await?;
    let changes: Vec<Change> = match check_paths::parse_raw(&raw) {
        Ok(changes) => changes,
        Err(why) => return Ok(Err(why)),
    };
    let paths = match check_paths::judge(&changes, wanted) {
        Ok(paths) => paths,
        Err(why) => return Ok(Err(why)),
    };
    let mut files = Vec::new();
    for path in paths {
        let blob = git_hardened_capped(
            repo,
            &["cat-file", "blob", &format!("{commit}:{path}")],
            FILE_MAX,
        )
        .await;
        match blob {
            Ok(bytes) => files.push((path, bytes)),
            Err(_) => return Ok(Err(format!("{path} could not be read or is over 256 KiB."))),
        }
    }
    Ok(Ok(files))
}

impl Core {
    fn check_scope(&self, project: &str, id: &str) -> Result<()> {
        self.crew_preview_only()?;
        valid_id(project)?;
        valid_id(id)?;
        self.store.project(project)?;
        Ok(())
    }

    fn check_escape(&self, project: &str, id: &str) -> Result<EscapeRecord> {
        self.store
            .crew_escape(id)?
            .filter(|escape| escape.project_id == project)
            .ok_or_else(|| CoreError::NotFound("escape".into()))
    }

    fn check_of(&self, id: &str) -> Result<EscapeCheckRecord> {
        self.store
            .crew_escape_check(id)?
            .ok_or_else(|| CoreError::NotFound("check".into()))
    }

    fn check_answer(&self, project: &str, id: &str) -> Result<wire::EscapeCheckAnswer> {
        Ok(answer(
            &self.check_escape(project, id)?,
            &self.check_of(id)?,
        ))
    }

    /// `POST /api/crew/escapes/{id}/check`: a person proposes a check for a confirmed escape. A proposal only: no task, no worker, no
    /// repository change. The fix commit is the one the body names, else the revert or fix the escape recorded; an escape with
    /// neither needs the person to name one.
    pub async fn crew_escape_check_propose(
        &self,
        id: &str,
        body: &wire::EscapeCheckPropose,
    ) -> Result<wire::EscapeCheckAnswer> {
        self.check_scope(&body.project, id)?;
        let path = body.path.trim();
        if let Some(why) = check_paths::refusal(path) {
            return Err(CoreError::Invalid(why.into()));
        }
        refuse_secret(
            &serde_json::json!({ "path": path, "asserts": body.asserts, "run": body.run }),
        )
        .map_err(crew_error)?;
        let asserts = text("asserts", &body.asserts, TEXT_MAX)?;
        let run = text("run", &body.run, TEXT_MAX)?;
        let escape = self.check_escape(&body.project, id)?;
        // A view that is not the escape's is stale before anything is read from git. (A repeat of a standing proposal is the store's to
        // answer: it changes nothing whatever was seen.)
        let held = self.store.crew_escape_check(id)?;
        if held
            .as_ref()
            .is_none_or(|c| c.state != EscapeCheckState::Proposed)
            && (escape.state, escape.gap, held.map(|c| c.state))
                != (body.seen.state, body.seen.gap, body.seen.check)
        {
            return Err(CoreError::Stale(
                "This check changed since you looked. Reload it and choose again.".into(),
            ));
        }
        let merge = self
            .store
            .crew_merge(&escape.merge_task_id)?
            .ok_or_else(|| CoreError::NotFound("merge".into()))?;
        let repo = PathBuf::from(self.store.project(&body.project)?.path);
        let escaped = commit_of(&repo, &merge.merge_commit).await.ok_or_else(|| {
            CoreError::Invalid("The merge that escaped is not in this project's repository.".into())
        })?;
        let wanted = match body.fix_commit.as_deref().map(str::trim) {
            Some(commit) if !commit.is_empty() => commit.to_owned(),
            _ => self.recorded_fix(&escape, merge.reverted_commit.as_deref())?,
        };
        let fix = commit_of(&repo, &wanted).await.ok_or_else(|| {
            CoreError::Invalid(
                "The commit the check must pass on is not in this project's repository. Name the revert or fix commit.".into(),
            )
        })?;
        if fix == escaped {
            return Err(CoreError::Invalid(
                "The fix commit is the merge that escaped. Name the revert or fix commit.".into(),
            ));
        }
        let (escape, check) = self.store.propose_escape_check(
            &body.project,
            id,
            &body.seen,
            &CheckProposal {
                path,
                asserts: &asserts,
                run: &run,
                fix_commit: &fix,
            },
            unix_now(),
        )?;
        Ok(answer(&escape, &check))
    }

    /// The revert or fix commit the escape recorded: the merge's own revert, else the commit its detection named.
    fn recorded_fix(&self, escape: &EscapeRecord, reverted: Option<&str>) -> Result<String> {
        if let Some(commit) = reverted.filter(|c| is_hex(c, 7)) {
            return Ok(commit.to_owned());
        }
        for event in self
            .store
            .crew_escape_events(&escape.id, crate::store::EVENTS_PER_ESCAPE_MAX)?
        {
            if event.kind != wire::EscapeEventKind::Detected {
                continue;
            }
            if let Some(commit) = event
                .cause
                .strip_prefix("revert:")
                .or_else(|| event.cause.strip_prefix("follow_up_fix:"))
                .filter(|c| is_hex(c, 7))
            {
                return Ok(commit.to_owned());
            }
        }
        Err(CoreError::Invalid(
            "This escape recorded no revert or fix commit. Name the commit the check must pass on."
                .into(),
        ))
    }

    /// `POST /api/crew/escapes/{id}/check/accept`: the only route that starts a worker. The audit row and `writing` are written first;
    /// the worker is started after. If the start is refused before any task exists, the check goes back to `proposed` and the person
    /// sees the reason; if a task exists and cannot run, the check is `failed` with the reason.
    pub async fn crew_escape_check_accept(
        self: &Arc<Self>,
        id: &str,
        body: &wire::EscapeCheckAct,
    ) -> Result<wire::EscapeCheckAnswer> {
        self.check_scope(&body.project, id)?;
        let escape = self.check_escape(&body.project, id)?;
        let check = self.check_of(id)?;
        if check.state == EscapeCheckState::Writing {
            return Ok(answer(&escape, &check));
        }
        let merge = self
            .store
            .crew_merge(&escape.merge_task_id)?
            .ok_or_else(|| CoreError::NotFound("merge".into()))?;
        let thread = self
            .store
            .crew_run(&escape.merge_task_id)?
            .and_then(|run| self.store.thread(&run.lead_thread_id).ok())
            .filter(|thread| thread.project_id == body.project)
            .ok_or_else(|| {
                CoreError::Invalid(
                    "The lead of this merge is gone, so no check can be written for it.".into(),
                )
            })?;
        let stamper = self
            .review_stamper(&thread, &thread.engine)
            .map_err(crew_error)?;
        let (brief_draft, key_draft) = drafts(&escape, &check, &merge.merge_commit);
        let brief = stamper.brief(&brief_draft).map_err(crew_error)?;
        let key = stamper
            .answer_key(&key_draft, &brief.id)
            .map_err(crew_error)?;
        let task_id = brief.id.as_str().to_owned();
        let (escape, check, started) =
            self.store
                .accept_escape_check(&body.project, id, &body.seen, &task_id, unix_now())?;
        if !started {
            return Ok(answer(&escape, &check));
        }
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
                    crew_started: true,
                },
            })
            .await;
        let now = unix_now();
        let failed = |note: String| {
            self.store.finish_escape_check(
                id,
                &task_id,
                &CheckFinish::Failed { proof: None, note },
                now,
            )
        };
        match launched {
            Ok(started) if started["routed"] == Value::Bool(true) => {}
            Ok(refused) => {
                let why = refused["refusal"]["message"]
                    .as_str()
                    .unwrap_or("No engine could take the task.");
                failed(format!("The worker was not started: {why}"))?;
            }
            Err(error) if self.store.crew_task(&task_id)?.is_some() => {
                failed(format!("The worker was not started: {}", error.message))?;
            }
            Err(error) => {
                self.store.unwind_escape_check_accept(
                    &body.project,
                    id,
                    &task_id,
                    &error.message,
                    now,
                )?;
                return Err(crew_error(error));
            }
        }
        self.check_answer(&body.project, id)
    }

    /// `POST /api/crew/escapes/{id}/check/retry`: a failed check goes back to `proposed`. It starts nothing; only accept does.
    pub fn crew_escape_check_retry(
        &self,
        id: &str,
        body: &wire::EscapeCheckAct,
    ) -> Result<wire::EscapeCheckAnswer> {
        self.check_scope(&body.project, id)?;
        self.check_escape(&body.project, id)?;
        let (escape, check) =
            self.store
                .retry_escape_check(&body.project, id, &body.seen, unix_now())?;
        Ok(answer(&escape, &check))
    }

    /// `POST /api/crew/escapes/{id}/check/withdraw`: a person withdraws a check that is not merged. A worker still writing it is
    /// cancelled and a queued merge of it is turned down, after the audit row and the state are written.
    pub async fn crew_escape_check_withdraw(
        self: &Arc<Self>,
        id: &str,
        body: &wire::EscapeCheckAct,
    ) -> Result<wire::EscapeCheckAnswer> {
        self.check_scope(&body.project, id)?;
        self.check_escape(&body.project, id)?;
        let (escape, check, was) =
            self.store
                .withdraw_escape_check(&body.project, id, &body.seen, unix_now())?;
        if let (Some(task), Some(was)) = (&check.task_id, was) {
            match was {
                EscapeCheckState::Writing => {
                    self.crew_cancel_task(task, Some("The check was withdrawn."), "user")
                        .await?;
                }
                EscapeCheckState::Proven if self.store.merge_entry(task)?.is_some() => {
                    self.crew_reject_merge(task, "The check was withdrawn.")?;
                }
                _ => {}
            }
        }
        Ok(answer(&escape, &check))
    }

    /// Called when a crew task ends. Cheap for every task that is not a check's worker.
    pub(crate) fn spawn_escape_check_ended(self: &Arc<Self>, task_id: &str) {
        let Ok(Some(check)) = self.store.crew_escape_check_for_task(task_id) else {
            return;
        };
        let (core, id) = (self.clone(), check.escape_id);
        tokio::spawn(async move { core.escape_check_drive(&id).await });
    }

    /// The service's tick: finishes a `writing` check whose task ended while nothing was watching and records the merge of a proven
    /// one. A failure is logged and the next tick tries again.
    pub async fn crew_escape_check_reconcile(self: &Arc<Self>) {
        match self
            .store
            .crew_escape_checks_in(&[EscapeCheckState::Writing])
        {
            Ok(open) => {
                for check in open {
                    self.escape_check_drive(&check.escape_id).await;
                }
            }
            Err(error) => tracing::warn!(%error, "the escape checks could not be read"),
        }
        match self
            .store
            .crew_escape_checks_in(&[EscapeCheckState::Proven])
        {
            Ok(proven) => {
                for check in proven {
                    let Some(task) = check.task_id.as_deref() else {
                        continue;
                    };
                    if matches!(self.store.crew_merge(task), Ok(Some(_)))
                        && let Err(error) =
                            self.store
                                .merge_escape_check(&check.escape_id, task, unix_now())
                    {
                        tracing::warn!(%error, "a merged escape check could not be recorded");
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "the escape checks could not be read"),
        }
    }

    /// Takes a `writing` check whose task has ended to its end: the proof when the task passed, `failed` when it did not.
    async fn escape_check_drive(self: &Arc<Self>, escape_id: &str) {
        {
            let mut busy = driving().lock().unwrap_or_else(|e| e.into_inner());
            if !busy.insert(escape_id.to_owned()) {
                return;
            }
        }
        let outcome = self.escape_check_finish(escape_id).await;
        driving()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(escape_id);
        if let Err(error) = outcome {
            tracing::warn!(%error, "an escape check could not be finished");
        }
    }

    async fn escape_check_finish(self: &Arc<Self>, escape_id: &str) -> Result<()> {
        let Some(check) = self.store.crew_escape_check(escape_id)? else {
            return Ok(());
        };
        let Some(task_id) = check.task_id.clone() else {
            return Ok(());
        };
        if check.state != EscapeCheckState::Writing {
            return Ok(());
        }
        let Some(task) = self.store.crew_task(&task_id)? else {
            return Ok(());
        };
        let finish = match task.state.as_str() {
            "passed" => match self.escape_check_prove(&check, &task_id).await {
                Ok(finish) => finish,
                Err(error) => CheckFinish::Failed {
                    proof: None,
                    note: format!("The proof could not run: {}", error.public_message()),
                },
            },
            "stopped" => CheckFinish::Failed {
                proof: None,
                note: "The worker stopped before it finished the check.".into(),
            },
            "cancelled" => CheckFinish::Failed {
                proof: None,
                note: "The worker's task was cancelled.".into(),
            },
            _ => return Ok(()),
        };
        let proven = matches!(finish, CheckFinish::Proven(_));
        let wrote = self
            .store
            .finish_escape_check(escape_id, &task_id, &finish, unix_now())?;
        if proven && wrote {
            self.escape_check_queue(&check, &task_id).await;
        }
        Ok(())
    }

    /// Queues a proven check like any change, through the merge queue's own entry. A refusal is noted on the check, never retried here.
    async fn escape_check_queue(self: &Arc<Self>, check: &EscapeCheckRecord, task_id: &str) {
        let branch = self
            .store
            .crew_escape(&check.escape_id)
            .ok()
            .flatten()
            .and_then(|escape| self.store.crew_merge(&escape.merge_task_id).ok().flatten())
            .map_or_else(|| "main".to_owned(), |merge| merge.branch);
        let note = match self
            .crew_merge_request(
                task_id,
                &MergeRequest {
                    operation: MergeOperation::Queue,
                    branch,
                },
            )
            .await
        {
            Ok(_) => "Queued for merge like any change.".to_owned(),
            Err(error) => format!(
                "Proven, not queued: {} Queue it from the Review queue.",
                error.public_message()
            ),
        };
        if let Err(error) =
            self.store
                .note_escape_check(&check.escape_id, task_id, &note, unix_now())
        {
            tracing::warn!(%error, "a check's queue note could not be saved");
        }
    }

    /// The worker's change, judged and proved. `Failed` for anything that is not a proven check; an error only when git or the
    /// scratch space could not be used.
    async fn escape_check_prove(
        &self,
        check: &EscapeCheckRecord,
        task_id: &str,
    ) -> Result<CheckFinish> {
        let failed = |note: String| Ok(CheckFinish::Failed { proof: None, note });
        let escape = self
            .store
            .crew_escape(&check.escape_id)?
            .ok_or_else(|| CoreError::NotFound("escape".into()))?;
        let merge = self
            .store
            .crew_merge(&escape.merge_task_id)?
            .ok_or_else(|| CoreError::NotFound("merge".into()))?;
        let repo = PathBuf::from(self.store.project(&check.project_id)?.path);
        let Some(base) = self
            .store
            .crew_run(task_id)?
            .and_then(|run| run.base_commit)
            .filter(|b| is_hex(b, 40))
        else {
            return failed(
                "The worker's starting commit is not known, so its change cannot be judged.".into(),
            );
        };
        let Some(commit) = self
            .store
            .crew_results(task_id)?
            .into_iter()
            .rfind(|r| r.state == ResultState::Passed)
            .and_then(|r| r.commit)
            .map(|c| c.as_str().to_owned())
            .filter(|c| is_hex(c, 40) && c.bytes().any(|b| b != b'0'))
        else {
            return failed("The worker's result names no commit.".into());
        };
        let files = match escape_check_change(&repo, &base, &commit, &check.path).await? {
            Ok(files) => files,
            Err(why) => return failed(why),
        };
        let escaped = merge.merge_commit.clone();
        let fix = check.fix_commit.clone();
        let Some(on_escape) = self
            .escape_check_run_at(&repo, &escaped, &files, &check.run)
            .await?
        else {
            return failed("The check could not be run on the escaped merge.".into());
        };
        let Some(on_fix) = self
            .escape_check_run_at(&repo, &fix, &files, &check.run)
            .await?
        else {
            return failed("The check could not be run on the fix.".into());
        };
        let proof = wire::EscapeCheckProof {
            escaped_commit: escaped,
            fix_commit: fix,
            fails_on_escape: !on_escape,
            passes_on_fix: on_fix,
        };
        if proof.fails_on_escape && proof.passes_on_fix {
            return Ok(CheckFinish::Proven(proof));
        }
        let note = match (proof.fails_on_escape, proof.passes_on_fix) {
            (false, true) => "It passes on the escaped merge too, so it proves nothing.",
            (true, false) => {
                "It fails on the fix too, so it does not separate the escape from its fix."
            }
            _ => "It passes on the escaped merge and fails on the fix, the wrong way round.",
        };
        Ok(CheckFinish::Failed {
            proof: Some(proof),
            note: note.into(),
        })
    }

    /// Runs `run` on `commit` of `repo` in a throwaway detached worktree with `files` laid over it: `Some(passed)`, or `None` when the
    /// command could not run (it did not start or ran out of time), which is not a verdict on the test. The worktree is always removed.
    async fn escape_check_run_at(
        &self,
        repo: &Path,
        commit: &str,
        files: &[(String, Vec<u8>)],
        run: &str,
    ) -> Result<Option<bool>> {
        let scratch = self
            .config
            .data_dir
            .join("crew-escape-proof")
            .join(crate::util::new_id());
        if let Some(parent) = scratch.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let dir = scratch.to_string_lossy().into_owned();
        git_hardened(
            repo,
            &["worktree", "add", "--detach", "--force", &dir, commit],
        )
        .await?;
        let outcome = async {
            for (path, bytes) in files {
                let target = scratch.join(path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, bytes)?;
            }
            let ran = crate::crew::checks::run_line(
                run,
                &scratch,
                std::time::Instant::now() + std::time::Duration::from_secs(PROOF_SECONDS),
            )
            .await;
            Ok::<_, CoreError>(match ran.outcome {
                orbyn_contracts::CheckOutcome::Pass => Some(true),
                orbyn_contracts::CheckOutcome::Fail => Some(false),
                _ => None,
            })
        }
        .await;
        if git_hardened(repo, &["worktree", "remove", "--force", &dir])
            .await
            .is_err()
        {
            let _ = std::fs::remove_dir_all(&scratch);
            let _ = git_hardened(repo, &["worktree", "prune"]).await;
        }
        outcome
    }
}
