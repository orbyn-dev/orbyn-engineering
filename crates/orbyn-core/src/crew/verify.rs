//! The verifier: code, not a model. It runs every check of a task's answer key in the task's
//! checkout, writes one Evidence per check and one Gate, and judges the run. It decides
//! nothing about the lifecycle; `settle` does that from the judgement.
//!
//! A failed check yields a *reason* for the worker: what failed, never the check's command or
//! any other part of the key (H1). The reason is redacted of secrets, has the key's text
//! withheld, and is proven free of both before it is used.

use std::path::Path;
use std::sync::OnceLock;

use orbyn_contracts::{
    AnswerKey, CheckOutcome, CrewAuthor, CrewCommit, CrewError, CrewHash, CrewId, CrewObject,
    CrewShortText, ErrorCode, Evidence, Gate, GateDecider, GateDecision, GateStage, ResultCheck,
};
use regex::Regex;
use sha2::{Digest, Sha256};

use super::checks::{Ran, Site, run_check, tail};
use super::guard::{key_needles, redact, refuse_leak, refuse_secret, withhold};
use super::leases::{core_author, internal, timestamp};
use crate::app::Core;
use crate::store::{CrewRun, CrewTask};

const ZERO_COMMIT: &str = "0000000000000000000000000000000000000000";
const NOT_PROVEN: &str = "Anything this check does not run.";

/// One check that did not pass.
#[derive(Clone, Debug)]
pub struct Failed {
    pub id: String,
    pub outcome: CheckOutcome,
    pub exit_code: Option<u32>,
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
    pub passes: usize,
}

impl Judgement {
    pub fn passed(&self) -> bool {
        self.failures.is_empty() && self.passes > 0
    }

    /// Nothing failed, and nothing passed either: every check was one this version cannot run.
    pub fn undecided(&self) -> bool {
        self.failures.is_empty() && self.passes == 0
    }

    /// A hash of the failures, so the loop guard can tell the same failure from another.
    /// Timings, colour codes and temporary paths are dropped first, since they differ between
    /// two runs of one failure.
    pub fn fingerprint(&self) -> String {
        let mut lines: Vec<String> = self
            .failures
            .iter()
            .map(|f| {
                format!(
                    "{}|{:?}|{:?}|{}",
                    f.id,
                    f.outcome,
                    f.exit_code,
                    normalize(&f.reason)
                )
            })
            .collect();
        lines.sort();
        crate::util::hex(&Sha256::digest(lines.join("\n").as_bytes()))
    }
}

fn noise() -> &'static [Regex; 3] {
    static NOISE: OnceLock<[Regex; 3]> = OnceLock::new();
    NOISE.get_or_init(|| {
        [
            Regex::new("\u{1b}\\[[0-9;]*[A-Za-z]").expect("a fixed pattern"),
            Regex::new(r"(?i)\b\d+(\.\d+)?\s*(ms|secs?|seconds?|s|m)\b").expect("a fixed pattern"),
            Regex::new(r"(?i)(/tmp|/var/folders|\\temp)[^\s]*").expect("a fixed pattern"),
        ]
    })
}

fn normalize(text: &str) -> String {
    noise().iter().fold(text.to_owned(), |text, pattern| {
        pattern.replace_all(&text, "").into_owned()
    })
}

fn short(text: &str) -> Result<CrewShortText, CrewError> {
    let cut: String = text.chars().take(1000).collect();
    let cut = if cut.trim().is_empty() {
        "(empty)".to_owned()
    } else {
        cut
    };
    CrewShortText::new(cut).map_err(|why| CrewError::new(ErrorCode::InvalidObject, why))
}

async fn head(worktree: &Path) -> Option<CrewCommit> {
    let out = crate::source_control::git::git(worktree, &["rev-parse", "HEAD"])
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
        self.judge(task, run, &key, Path::new(worktree)).await
    }

    async fn judge(
        &self,
        task: &CrewTask,
        run: &CrewRun,
        key: &AnswerKey,
        worktree: &Path,
    ) -> Result<Judgement, CrewError> {
        let site = Site {
            worktree,
            base: run.base_commit.as_deref(),
        };
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
            passes: 0,
        };
        for check in &key.checks {
            let ran = run_check(check, &site).await;
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
                does_not_prove: match &check.does_not_prove {
                    Some(text) => text.clone(),
                    None => short(NOT_PROVEN)?,
                },
            };
            evidence.validate()?;
            evidence.id = CrewObject::Evidence(evidence.clone()).computed_id()?;
            let object = CrewObject::Evidence(evidence.clone());
            refuse_secret(&object.to_value())?;
            self.store
                .add_crew_object(&task.id, &object)
                .map_err(internal)?;
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
                    outcome: ran.outcome,
                    exit_code: ran.exit_code,
                    reason: worker_reason(&ran, &needles),
                }),
            }
        }
        if !verdict.undecided() {
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
