//! The verifier: code, not a model. It runs every check of a task's answer key in the task's
//! checkout, writes one Evidence per check and one Gate, and judges the run. It decides
//! nothing about the lifecycle; `settle` does that from the judgement.
//!
//! A failed check yields a *reason* for the worker: what failed, never the check's command or
//! any other part of the key (H1). The reason is redacted of secrets, has the key's text
//! withheld, and is proven free of both before it is used.

use std::path::Path;

use orbyn_contracts::{
    AnswerKey, CheckKind, CheckOutcome, CrewAuthor, CrewCheck, CrewCommit, CrewError, CrewHash,
    CrewId, CrewObject, CrewShortText, ErrorCode, Evidence, Gate, GateDecider, GateDecision,
    GateStage, ResultCheck,
};
use sha2::{Digest, Sha256};

use super::checks::{Ran, Site, run_check, tail};
use super::guard::{key_needles, redact, refuse_leak, refuse_secret, withhold};
use super::leases::{core_author, internal, timestamp};
use super::protect::{Snapshot, protected_changes};
use crate::app::Core;
use crate::store::{CrewRun, CrewTask, ManualCheckState};

const ZERO_COMMIT: &str = "0000000000000000000000000000000000000000";
const NOT_PROVEN: &str = "Anything this check does not run.";
const DIRTY: &str = "The checkout had uncommitted changes, so this commit is not exactly what ran.";

/// What a failure names, in words a worker may read: a check's kind (never its id, its command or any other
/// part of the key), or the core's own words for a failure no check ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// A check in the answer key, by the kind it declares. The word is the protocol's own kind name, so a new
    /// kind gains its word with the contract.
    Check(CheckKind),
    /// A failure the core raises itself before any check runs, named by its own fixed words.
    Core(&'static str),
}

impl FailureKind {
    /// The word a stored lesson may name: the check's kind, never its id.
    pub fn word(self) -> String {
        match self {
            Self::Check(kind) => super::wire_name(&kind),
            Self::Core(word) => word.to_owned(),
        }
    }
}

/// One check that did not pass.
#[derive(Clone, Debug)]
pub struct Failed {
    /// The check's id, or the core's own name for a synthetic failure. Internal to the core: a fingerprint and
    /// the loop guard use it, and it never reaches a lesson, a brief or a worker.
    pub id: String,
    /// What the failure names in words a worker may read: the check's kind, never its id (H1).
    pub kind: FailureKind,
    pub outcome: CheckOutcome,
    pub exit_code: Option<u32>,
    /// What failed, by name. The fingerprint is made of these and not of the output.
    pub names: Vec<String>,
    /// Safe for a worker to read.
    pub reason: String,
}

/// What a run of the key found.
#[derive(Clone, Debug)]
pub struct Judgement {
    pub commit: CrewCommit,
    pub checks: Vec<ResultCheck>,
    pub evidence: Vec<CrewHash>,
    pub failures: Vec<Failed>,
    /// Checks this version does not run, so the result never implies more than was checked.
    pub not_verified: Vec<String>,
    /// The `manual` checks nobody has decided yet, by id: the task waits for a person and must not be judged either way.
    pub awaiting: Vec<String>,
    pub passes: usize,
}

impl Judgement {
    /// Nothing failed, at least one check passed, and no check is still waiting for a person.
    pub fn passed(&self) -> bool {
        self.failures.is_empty() && self.passes > 0 && self.awaiting.is_empty()
    }

    /// Nothing failed, and nothing passed either: every check was one this version cannot run.
    pub fn undecided(&self) -> bool {
        self.failures.is_empty() && self.passes == 0 && self.awaiting.is_empty()
    }

    /// A check waits for a person's decision: the task parks instead of passing or failing.
    pub fn waiting(&self) -> bool {
        !self.awaiting.is_empty()
    }

    /// A hash of the failures, so the loop guard can tell the same failure from another: the
    /// check, how it ended and the names of what failed. The output is left out, since it
    /// changes between two runs of one failure.
    pub fn fingerprint(&self) -> String {
        let mut lines: Vec<String> = self
            .failures
            .iter()
            .map(|f| {
                let mut names = f.names.clone();
                names.sort();
                format!(
                    "{}|{:?}|{:?}|{}",
                    f.id,
                    f.outcome,
                    f.exit_code,
                    names.join(",")
                )
            })
            .collect();
        lines.sort();
        crate::util::hex(&Sha256::digest(lines.join("\n").as_bytes()))
    }
}

/// How the core runs the key again for a person (`rerun`): checks the screen refused are not run, the whole run has an upper
/// bound of its own, and **nothing is stored**: Review and the merge proof count every Evidence and Gate of a task as the task's own
/// verification, and the protocol's Evidence has no field that says "a person's re-run", so a re-run's rows would change what a
/// reviewer is told and the commit it reads. The re-run's record is its audit rows (`rerun`).
pub(super) struct Rerun<'a> {
    /// The checks not to run, by id, with the rule that refused each.
    pub refused: &'a [(String, &'static str)],
    /// The longest the whole run may take, whatever the task's budget was.
    pub cap: std::time::Duration,
}

/// What is left of a task's time budget: its start plus its minutes, less `now`.
pub(super) fn budget_left(created_at: &str, minutes: u64, now: i64) -> std::time::Duration {
    let start = crate::util::parse_time(created_at).unwrap_or(now);
    let end = start.saturating_add(i64::try_from(minutes * 60).unwrap_or(i64::MAX));
    std::time::Duration::from_secs(u64::try_from(end - now).unwrap_or(0))
}

/// What is left of a task's time budget after a restart: the time between its start and its last
/// recorded event, less the downtime already excused, clamped to the budget. The app being off
/// spends no minutes; `last_event` is a unix second from `Store::crew_task_last_event` (or now
/// for a live check, where `downtime_secs` is the only part of the elapsed time excused).
pub(super) fn budget_left_after_downtime(
    created_at: &str,
    minutes: u64,
    last_event: i64,
    downtime_secs: i64,
) -> std::time::Duration {
    let start = crate::util::parse_time(created_at).unwrap_or(last_event);
    let budget = i64::try_from(minutes.saturating_mul(60)).unwrap_or(i64::MAX);
    let spent = last_event
        .saturating_sub(start)
        .saturating_sub(downtime_secs)
        .clamp(0, budget);
    std::time::Duration::from_secs(u64::try_from(budget - spent).unwrap_or(0))
}

/// What a refused check records: that it did not run, and the rule, never the command.
fn refused_check(check: &orbyn_contracts::CrewCheck, rule: &str) -> Ran {
    let why = format!("The check was not run: the command guard refused it ({rule}).");
    Ran {
        outcome: CheckOutcome::Error,
        exit_code: None,
        command: format!("refused: {}", check.id),
        log: why.clone(),
        duration_ms: 0,
        reason: Some(why),
        names: Vec::new(),
    }
}

/// One `manual` check of the key: the person's stored decision once they made it, and otherwise the wait for one. A wait parks the check
/// in `crew_manual_checks` and is listed on the judgement's `awaiting`, so the task holds rather than passing on the other checks or
/// failing on a check nobody ran. A person's own words go only into the worker-facing reason, where the usual leak and secret checks
/// apply; no Evidence excerpt carries them, and the pass and fail cases log nothing of their own.
fn manual_check(
    core: &Core,
    task: &CrewTask,
    check: &CrewCheck,
    verdict: &mut Judgement,
) -> Result<Ran, CrewError> {
    let held = core
        .store
        .crew_manual_check(&task.id, check.id.as_str())
        .map_err(internal)?;
    let (decided, note) = match held {
        Some(row) => (row.state, row.note),
        None => (ManualCheckState::Awaiting, None),
    };
    let ran = |outcome: CheckOutcome, command: String| Ran {
        outcome,
        exit_code: None,
        command,
        log: String::new(),
        duration_ms: 0,
        reason: None,
        names: Vec::new(),
    };
    match decided {
        ManualCheckState::Passed => Ok(ran(
            CheckOutcome::Pass,
            format!("manual: {} checked by a person", check.id),
        )),
        ManualCheckState::Failed => {
            let why = note
                .filter(|text| !text.trim().is_empty())
                .unwrap_or_else(|| "A person checked this and it does not hold.".to_owned());
            let mut failed = ran(
                CheckOutcome::Fail,
                format!("manual: {} checked by a person", check.id),
            );
            failed.reason = Some(why);
            failed.names = vec![check.id.as_str().to_owned()];
            Ok(failed)
        }
        ManualCheckState::Awaiting => {
            core.store
                .park_manual_check(&task.id, check.id.as_str(), crate::util::unix_now())
                .map_err(internal)?;
            verdict.awaiting.push(check.id.as_str().to_owned());
            Ok(ran(
                CheckOutcome::Skipped,
                format!("manual: {} waits for a person", check.id),
            ))
        }
    }
}

pub(super) fn short(text: &str) -> Result<CrewShortText, CrewError> {
    let cut: String = text.chars().take(1000).collect();
    let cut = if cut.trim().is_empty() {
        "(empty)".to_owned()
    } else {
        cut
    };
    CrewShortText::new(cut).map_err(|why| CrewError::new(ErrorCode::InvalidObject, why))
}

pub(super) async fn head(worktree: &Path) -> Option<CrewCommit> {
    let out = crate::source_control::git::git_hardened(worktree, &["rev-parse", "HEAD"])
        .await
        .ok()?;
    CrewCommit::new(crate::source_control::git::text(&out)).ok()
}

/// The reason a worker is shown: redacted, with the key's text withheld, and checked.
pub(super) fn worker_reason(ran: &Ran, needles: &[String]) -> String {
    let reason = ran
        .reason
        .clone()
        .unwrap_or_else(|| "The check did not pass.".to_owned());
    let safe = withhold(&redact(&reason), needles);
    let value = serde_json::json!({ "reason": safe });
    if refuse_leak(&value, needles).is_err() || refuse_secret(&value).is_err() {
        return "A check failed. Its output is withheld.".to_owned();
    }
    safe
}

impl Core {
    /// Runs every check of the task's key, stores their Evidence and the Gate, and judges.
    /// Can be called again at any time, including after the run has ended: it adds Evidence
    /// and a Gate and changes no state (SPEC.md H7).
    pub async fn crew_verify(
        &self,
        task: &CrewTask,
        run: &CrewRun,
    ) -> Result<Judgement, CrewError> {
        let key = self
            .store
            .crew_answer_key(&task.id)
            .map_err(internal)?
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no key"))?;
        let worktree = run
            .worktree
            .as_deref()
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no checkout"))?;
        self.judge(task, run, &key, Path::new(worktree), false, None)
            .await
    }

    /// The verification of a task that is `verifying` now: it runs inside what is left of the
    /// task's time budget, and stops as soon as the task is no longer `verifying`, such as when
    /// the budget timer stopped it.
    pub(super) async fn crew_verify_live(
        &self,
        task: &CrewTask,
        run: &CrewRun,
    ) -> Result<Judgement, CrewError> {
        let key = self
            .store
            .crew_answer_key(&task.id)
            .map_err(internal)?
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no key"))?;
        let worktree = run
            .worktree
            .as_deref()
            .ok_or_else(|| CrewError::new(ErrorCode::InvalidObject, "the task has no checkout"))?;
        self.judge(task, run, &key, Path::new(worktree), true, None)
            .await
    }

    pub(super) async fn judge(
        &self,
        task: &CrewTask,
        run: &CrewRun,
        key: &AnswerKey,
        worktree: &Path,
        live: bool,
        rerun: Option<&Rerun<'_>>,
    ) -> Result<Judgement, CrewError> {
        let minutes = self
            .store
            .crew_brief(&task.id)
            .map_err(internal)?
            .map_or(30, |brief| u64::from(brief.budget.minutes));
        // A verification inside the task gets what is left of its budget; one the core runs
        // again after the run has ended gets a fresh one.
        let left = if live {
            budget_left_after_downtime(
                &task.created_at,
                minutes,
                crate::util::unix_now(),
                run.downtime_secs,
            )
        } else {
            std::time::Duration::from_secs(minutes * 60)
        };
        let left = rerun.map_or(left, |r| left.min(r.cap));
        let site = Site {
            worktree,
            deadline: std::time::Instant::now() + left,
            base: run.base_commit.as_deref(),
        };
        let dirty = crate::source_control::git::git_hardened(worktree, &["status", "--porcelain"])
            .await
            .is_ok_and(|out| !out.is_empty());
        let commit = head(worktree)
            .await
            .unwrap_or_else(|| CrewCommit::new(ZERO_COMMIT).expect("a fixed commit"));
        let needles = key_needles(key);
        let run_id = CrewId::new(task.run_id.clone())
            .map_err(|why| CrewError::new(ErrorCode::InvalidObject, why))?;
        let author: CrewAuthor = core_author();
        let mut verdict = Judgement {
            commit: commit.clone(),
            checks: Vec::new(),
            evidence: Vec::new(),
            failures: Vec::new(),
            not_verified: Vec::new(),
            awaiting: Vec::new(),
            passes: 0,
        };
        // N7, H9 and H11: if the worker changed a file the checks use, nothing is run. The
        // checks would be the worker's own. The reason does not name the file.
        // A key with protected paths needs the record taken when the task started. Without a
        // usable one nothing can be compared, so nothing is run.
        let before: Option<Snapshot> = if key.protected_paths.is_empty() {
            None
        } else {
            Some(
                run.protected
                    .as_deref()
                    .and_then(|text| serde_json::from_str(text).ok())
                    .ok_or_else(|| {
                        CrewError::new(
                            ErrorCode::InvalidObject,
                            "the protected files were not recorded when the task started",
                        )
                    })?,
            )
        };
        if let Some(before) = before {
            let touched = protected_changes(&before, worktree, &key.protected_paths);
            if !touched.is_empty() {
                verdict.failures.push(Failed {
                    id: "protected_paths".to_owned(),
                    kind: FailureKind::Core("protected paths"),
                    outcome: CheckOutcome::Fail,
                    exit_code: None,
                    names: touched,
                    reason: "Your changes include files you may not modify. Revert them, and \
                             change only the paths you were given."
                        .to_owned(),
                });
                return Ok(verdict);
            }
        }
        // A flag that hides a file from git blinds every git-based check, so any present flag fails the attempt. If the
        // flags cannot be read, or were recorded for another directory than the one checked, nothing is run: the task goes to a
        // person rather than passing a check that could not be made.
        if let Some(text) = run.index_flags.as_deref() {
            let recorded: serde_json::Value = serde_json::from_str(text).map_err(|_| {
                CrewError::new(
                    ErrorCode::InvalidObject,
                    "the index flags were not recorded",
                )
            })?;
            serde_json::from_value::<Vec<String>>(recorded["flags"].clone()).map_err(|_| {
                CrewError::new(
                    ErrorCode::InvalidObject,
                    "the index flags were not recorded",
                )
            })?;
            if recorded["dir"].as_str().map(std::path::Path::new) != Some(worktree) {
                return Err(CrewError::new(
                    ErrorCode::InvalidObject,
                    "the index flags were recorded for another directory than the one checked",
                ));
            }
            let now = super::index_flags::flagged(worktree).await.ok_or_else(|| {
                CrewError::new(
                    ErrorCode::InvalidObject,
                    "the checkout's git index cannot be listed",
                )
            })?;
            let hidden = now;
            if !hidden.is_empty() {
                verdict.failures.push(Failed {
                    id: "index_flags".to_owned(),
                    kind: FailureKind::Core("index flags"),
                    outcome: CheckOutcome::Fail,
                    exit_code: None,
                    names: hidden,
                    reason: "Your changes hide files from git (assume-unchanged or skip-worktree). Undo that, \
                             and change only the paths you were given."
                        .to_owned(),
                });
                return Ok(verdict);
            }
        }
        for check in &key.checks {
            if live
                && self
                    .store
                    .crew_task(&task.id)
                    .map_err(internal)?
                    .is_none_or(|t| t.state != "verifying")
            {
                return Err(CrewError::new(
                    ErrorCode::RunEnded,
                    "the task is no longer being verified",
                ));
            }
            let ran = match rerun
                .and_then(|r| r.refused.iter().find(|(id, _)| id == check.id.as_str()))
            {
                // A check the screen refused is never run. It counts as an error, so the Gate cannot pass on the rest.
                Some((_, rule)) => refused_check(check, rule),
                None => {
                    if check.id.as_str() == "chk-fix-touched"
                        || check.params["name"] == "crew-fix-touched"
                    {
                        if check.id.as_str() != "chk-fix-touched"
                            || !matches!(check.kind, orbyn_contracts::CheckKind::RepoRequired)
                            || check.params["name"] != "crew-fix-touched"
                        {
                            return Err(CrewError::new(
                                ErrorCode::InvalidObject,
                                "the byte check marker is invalid",
                            ));
                        }
                        let fix = self
                            .store
                            .crew_fix_for_task(&task.id)
                            .map_err(internal)?
                            .ok_or_else(|| {
                                CrewError::new(
                                    ErrorCode::InvalidObject,
                                    "the byte check has no fix task",
                                )
                            })?;
                        let found = self
                            .store
                            .crew_finding(&fix.finding_id)
                            .map_err(internal)?
                            .ok_or_else(|| {
                                CrewError::new(ErrorCode::InvalidObject, "the fix has no finding")
                            })?;
                        let stored = self
                            .store
                            .crew_answer_key(&task.id)
                            .map_err(internal)?
                            .ok_or_else(|| {
                                CrewError::new(ErrorCode::InvalidObject, "the fix has no key")
                            })?;
                        if fix.fix_task_id != task.id
                            || fix.subject_id != found.subject_id
                            || run.task_id != task.id
                            || run.worktree.as_deref().map(Path::new) != Some(worktree)
                            || stored.id != key.id
                            || stored.checks.iter().find(|c| c.id == check.id) != Some(check)
                        {
                            return Err(CrewError::new(
                                ErrorCode::InvalidObject,
                                "the byte check does not match the stored fix",
                            ));
                        }
                        super::checks::fix_touched(
                            found.finding.path.as_str(),
                            check.params["note"].as_str().unwrap_or(""),
                            &site,
                        )
                    } else if check.kind == CheckKind::Manual {
                        // A check only a person can make: their stored answer, or the wait for one.
                        manual_check(self, task, check, &mut verdict)?
                    } else {
                        // Boxed: this future is the biggest one in the verify chain, and the chain is awaited from the
                        // forge watch, so it sits on a small stack (a 2 MiB thread overflowed there).
                        Box::pin(run_check(check, &site)).await
                    }
                }
            };
            let mut evidence = Evidence {
                v: 0,
                id: key.id.clone(),
                crew_id: key.crew_id.clone(),
                author: author.clone(),
                created_at: timestamp(crate::util::unix_now())?,
                run_id: run_id.clone(),
                check: check.id.clone(),
                claim: None,
                command: short(&ran.command)?,
                outcome: ran.outcome,
                exit_code: ran.exit_code,
                duration_ms: ran.duration_ms,
                commit: commit.clone(),
                log_hash: CrewHash::new(format!(
                    "sha256:{}",
                    crate::util::hex(&Sha256::digest(ran.log.as_bytes()))
                ))
                .map_err(|why| CrewError::new(ErrorCode::InvalidObject, why))?,
                // Only the excerpt is redacted; a secret in any other field is refused.
                excerpt: redact(&tail(&ran.log, 4000)),
                artifacts: Vec::new(),
                proves: check.proves.clone(),
                does_not_prove: {
                    let own = check
                        .does_not_prove
                        .as_ref()
                        .map_or(NOT_PROVEN, CrewShortText::as_str);
                    // The commit is HEAD, but the checks ran on the checkout as it was.
                    if dirty {
                        short(&format!("{own} {DIRTY}"))?
                    } else {
                        short(own)?
                    }
                },
            };
            evidence.validate()?;
            evidence.id = CrewObject::Evidence(evidence.clone()).computed_id()?;
            let object = CrewObject::Evidence(evidence.clone());
            refuse_secret(&object.to_value())?;
            if rerun.is_none() {
                self.store
                    .add_crew_object(&task.id, &object)
                    .map_err(internal)?;
            }
            verdict.checks.push(ResultCheck {
                check: check.id.clone(),
                evidence: evidence.id.clone(),
                outcome: ran.outcome,
            });
            verdict.evidence.push(evidence.id);
            match ran.outcome {
                CheckOutcome::Pass => verdict.passes += 1,
                CheckOutcome::Skipped => verdict.not_verified.push(ran.command.clone()),
                CheckOutcome::Fail | CheckOutcome::Error => verdict.failures.push(Failed {
                    id: check.id.as_str().to_owned(),
                    kind: FailureKind::Check(check.kind),
                    outcome: ran.outcome,
                    exit_code: ran.exit_code,
                    names: ran.names.clone(),
                    reason: worker_reason(&ran, &needles),
                }),
            }
        }
        // A check waiting for a person decides nothing yet: the checks gate is written once the wait is answered.
        if rerun.is_none()
            && !verdict.undecided()
            && !verdict.evidence.is_empty()
            && !verdict.waiting()
        {
            self.store_checks_gate(task, key, &run_id, &verdict)?;
        }
        Ok(verdict)
    }

    fn store_checks_gate(
        &self,
        task: &CrewTask,
        key: &AnswerKey,
        run_id: &CrewId,
        verdict: &Judgement,
    ) -> Result<(), CrewError> {
        let passed = verdict.passed();
        let reason = if passed {
            format!("All {} runnable checks passed.", verdict.passes)
        } else {
            format!(
                "{} of {} checks did not pass.",
                verdict.failures.len(),
                verdict.checks.len()
            )
        };
        let mut gate = Gate {
            v: 0,
            id: key.id.clone(),
            crew_id: key.crew_id.clone(),
            author: core_author(),
            created_at: timestamp(crate::util::unix_now())?,
            run_id: run_id.clone(),
            stage: GateStage::Checks,
            subject: key.brief.clone(),
            decision: if passed {
                GateDecision::Pass
            } else {
                GateDecision::Fail
            },
            decided_by: GateDecider::Code,
            evidence: verdict.evidence.clone(),
            reason: short(&reason)?,
            lane: None,
        };
        gate.validate()?;
        gate.id = CrewObject::Gate(gate.clone()).computed_id()?;
        self.store
            .add_crew_object(&task.id, &CrewObject::Gate(gate))
            .map_err(internal)
    }
}
