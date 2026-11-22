//! Crew Run page (Preview): one work task's own page, read from what this device already stores. Nothing here writes,
//! starts a process or reads a file: the store's read connection answers every list.
//!
//! What it shows is `docs/crew-run-page.md`: the person's prompt, the checks of the latest verification round, what was not
//! verified and the pull request the task made. The route is Local, so a worker that reads `service.json` can call it; a
//! check's command and what it proves are the key's own words, so they are withheld until the task is final (`passed`,
//! `stopped` or `cancelled`, the same rule as the Flow task read, H1), and a finished task never names a key of a task of
//! the same crew that has not ended. Repro and finding evidence is not a check on this page at all. Every other text passes
//! the ordinary scrub, which withholds a text that still holds a key (H1, H2), and the web cleans it again.
//!
//! Still missing, and named in `docs/crew-run-page.md`: the plan (no table, object or read stores a task's plan steps), and
//! the forge's own pull request URL and the pull request's current head (a crew pull request's publish records neither, so
//! the page cannot say whether the proof is stale).

use std::collections::HashMap;

use orbyn_contracts as wire;

use super::facts::Facts;
use super::scrub::Scrub;
use crate::app::Core;
use crate::crew::lifecycle::is_final;
use crate::crew::{valid_id, wire_name};
use crate::error::{CoreError, Result};
use crate::store::ForgeMerge;

/// The most checks and the most not-verified lines one page shows.
const CHECKS_MAX: usize = 50;
const NOT_VERIFIED_MAX: usize = 20;
/// The longest prompt, check label and proves line the page shows, in characters.
const PROMPT_MAX: usize = 2000;
const CHECK_TEXT_MAX: usize = 200;

impl Core {
    /// One work task's Run page: its prompt, the checks of its latest verification round, what was not verified and the
    /// pull request it made. A read: no state changes, so Autopilot's rule refuses nothing here.
    pub fn crew_flow_run(&self, id: &str) -> Result<wire::FlowRunPage> {
        self.crew_preview_only()?;
        valid_id(id)?;
        let Some(task) = self.store.crew_task(id)? else {
            return Err(CoreError::NotFound("task".into()));
        };
        if !self.store.crew_task_is_work(id)? {
            return Err(CoreError::NotFound("task".into()));
        }
        let person = self.display_redactor()?;
        let facts = self.flow_facts(task)?;
        let scrub = Scrub::of_run(self, &person, &facts.task.crew_id, &facts.task.id)?;
        let finished = is_final(&facts.task.state);
        let mut truncated = false;

        let prompt = facts.brief.as_ref().map(|brief| {
            let (text, hidden) = scrub.marked(&brief.outcome, PROMPT_MAX);
            wire::FlowRunPrompt {
                text,
                hidden,
                truncated: brief.outcome.chars().count() > PROMPT_MAX,
            }
        });

        // The checks, commit and not-verified lines are one round: the latest checks gate of the build now under way (the
        // latest fix when it has one, else the task), and the result that round wrote. Never every evidence row of the task.
        let (round_task, round_state, round_evidence, round_gate) = self.run_page_round(&facts)?;
        let results = self.store.crew_results(&round_task)?;
        // The gate names the checks the verifier ran; a result written after it may name a newer round when the gate is
        // missing or older. A later result with no checks (a cancel) is not a round of its own: the last checks stay.
        let gate_index = round_gate.as_ref().and_then(|gate| {
            results.iter().position(|result| {
                result.checks.len() == gate.evidence.len()
                    && result
                        .checks
                        .iter()
                        .all(|check| gate.evidence.contains(&check.evidence))
            })
        });
        let latest_index = results.iter().rposition(|result| !result.checks.is_empty());
        // A gate whose evidence no result names is a round whose result is missing (or a test's later gate): its checks are
        // the only record. Otherwise the newest result with checks wins when it comes after the gate's own result.
        let (result, from_result) = match (gate_index, latest_index) {
            (Some(gate), Some(latest)) if latest > gate => (Some(&results[latest]), true),
            (Some(gate), _) => (Some(&results[gate]), false),
            (None, _) if round_gate.is_some() => (None, false),
            (None, Some(latest)) => (Some(&results[latest]), true),
            (None, None) => (results.last(), false),
        };

        // A repro and a finding's comparison are core-written evidence too, but they are as secret as a key and are not
        // checks of the run. The latest round's gate or result cites the checks it ran; only those are shown.
        let by_id: HashMap<&str, &wire::Evidence> = round_evidence
            .iter()
            .map(|evidence| (evidence.id.as_str(), evidence))
            .collect();
        let mut cited: Vec<&wire::Evidence> = Vec::new();
        if from_result && let Some(result) = result {
            cited.extend(
                result
                    .checks
                    .iter()
                    .filter_map(|check| by_id.get(check.evidence.as_str()).copied()),
            );
        } else if let Some(gate) = &round_gate {
            cited.extend(
                gate.evidence
                    .iter()
                    .filter_map(|id| by_id.get(id.as_str()).copied()),
            );
        }
        if cited.is_empty()
            && let Some(result) = result
        {
            cited.extend(
                result
                    .checks
                    .iter()
                    .filter_map(|check| by_id.get(check.evidence.as_str()).copied()),
            );
        }
        let repros: Vec<(String, String)> = self
            .store
            .crew_repros_of(&facts.task.id)?
            .iter()
            .filter_map(|repro| serde_json::from_str::<wire::CrewCheck>(&repro.repro).ok())
            .map(|check| {
                (
                    check.id.as_str().to_owned(),
                    check.proves.as_str().to_owned(),
                )
            })
            .collect();
        let is_repro_or_finding = |evidence: &wire::Evidence| {
            evidence.check.as_str() == "finding-agreement"
                || repros.iter().any(|(id, proves)| {
                    (evidence.check.as_str() == id.as_str()
                        || evidence.check.as_str().strip_suffix("-fix") == Some(id.as_str()))
                        && evidence.proves.as_str() == proves.as_str()
                })
        };
        cited.retain(|evidence| !is_repro_or_finding(evidence));
        let skip = cited.len().saturating_sub(CHECKS_MAX);
        truncated |= skip > 0;

        // A command is shown only once the round that ran it has ended; a fix that is still running is not final either.
        let show_text = finished && is_final(&round_state);
        let mut checks = Vec::new();
        for evidence in cited.into_iter().skip(skip) {
            let (label, label_cut) = if show_text {
                scrub.check_text(evidence.command.as_str(), CHECK_TEXT_MAX)
            } else {
                (String::new(), false)
            };
            let (proves, proves_cut) = if show_text {
                scrub.check_text(evidence.proves.as_str(), CHECK_TEXT_MAX)
            } else {
                (String::new(), false)
            };
            truncated |= label_cut || proves_cut;
            checks.push(wire::FlowRunCheck {
                label,
                outcome: wire_name(&evidence.outcome),
                exit_code: evidence.exit_code,
                duration_ms: evidence.duration_ms,
                proves,
                commit: evidence.commit.as_str().to_owned(),
            });
        }
        let mut not_verified = Vec::new();
        if show_text && let Some(result) = result {
            truncated |= result.not_verified.len() > NOT_VERIFIED_MAX;
            for text in result.not_verified.iter().take(NOT_VERIFIED_MAX) {
                truncated |= text.as_str().chars().count() > CHECK_TEXT_MAX;
                not_verified.push(scrub.text(text.as_str(), CHECK_TEXT_MAX));
            }
        }

        // The commit and branch the round published: the same result the checks came from, never another round's. A round
        // whose result is not readable still has the commit its own checks ran on.
        let commit = result
            .and_then(|result| result.commit.as_ref())
            .map(|commit| commit.as_str().to_owned())
            .or_else(|| checks.last().map(|check| check.commit.clone()));
        let branch = result
            .and_then(|result| result.branch.as_ref())
            .map(|branch| branch.as_str().to_owned());

        // Only the forge intent says Orbyn opened a pull request for this task: no match by branch name or title.
        let pull = self
            .store
            .crew_forge_merge(&facts.task.id)?
            .map(|forge| pull_of(&forge, branch.as_deref(), commit.as_deref()));

        Ok(wire::FlowRunPage {
            task: facts.task.id.clone(),
            crew: facts.task.crew_id.clone(),
            state: facts.task.state.clone(),
            finished,
            prompt,
            checks,
            not_verified,
            commit,
            pull,
            truncated,
        })
    }

    /// The round the page shows: the latest fix's checks gate when the fix has one, else a fix that has written a result,
    /// else the task's own newest checks gate. The evidence is that build's, and the result is matched to the gate's
    /// evidence later, so the checks, commit and not-verified lines agree.
    #[allow(clippy::type_complexity)]
    fn run_page_round(
        &self,
        facts: &Facts,
    ) -> Result<(String, String, Vec<wire::Evidence>, Option<wire::Gate>)> {
        if let Some((fix, state)) = facts.fixes.last() {
            if let Some(gate) = facts.build_gates.last() {
                return Ok((
                    fix.fix_task_id.clone(),
                    state.clone(),
                    facts.build_evidence.clone(),
                    Some(gate.clone()),
                ));
            }
            // A fix whose verification gate is not readable still has its own result; never fall back to the original
            // task's round when the fix has already written one.
            if !self.store.crew_results(&fix.fix_task_id)?.is_empty() {
                return Ok((
                    fix.fix_task_id.clone(),
                    state.clone(),
                    facts.build_evidence.clone(),
                    None,
                ));
            }
        }
        let brief = facts.brief.as_ref().map(|brief| brief.id.clone());
        let gate = facts
            .gates
            .iter()
            .rev()
            .find(|gate| {
                gate.stage == wire::GateStage::Checks
                    && brief.as_ref().is_some_and(|brief| *brief == gate.subject)
            })
            .cloned();
        Ok((
            facts.task.id.clone(),
            facts.task.state.clone(),
            self.store.crew_evidence(&facts.task.id)?,
            gate,
        ))
    }
}

/// The pull request one task made, from its forge intent and what the run published. A run that named no branch still
/// has the branch the forge intent published, so the page never drops it.
fn pull_of(forge: &ForgeMerge, branch: Option<&str>, commit: Option<&str>) -> wire::FlowRunPull {
    let branch = branch
        .filter(|branch| !branch.is_empty())
        .map(str::to_owned)
        .or_else(|| Some(forge.source.clone()).filter(|source| !source.is_empty()));
    wire::FlowRunPull {
        number: forge.pull.and_then(|n| u32::try_from(n).ok()),
        branch,
        commit: commit.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ForgePhase;
    use orbyn_contracts::CrewHash;

    fn hash(byte: char) -> String {
        CrewHash::new(format!("sha256:{}", byte.to_string().repeat(64)))
            .unwrap()
            .as_str()
            .to_owned()
    }

    /// A forge intent for one task, published as pull request `pull`.
    fn forge(pull: Option<u64>) -> ForgeMerge {
        ForgeMerge {
            task_id: "sha256:task".into(),
            repo: "/checkout".into(),
            remote: "origin".into(),
            branch: "main".into(),
            source: "crew/task-1".into(),
            candidate: "a".repeat(40),
            base: "b".repeat(40),
            combined: Some("c".repeat(40)),
            pull,
            phase: ForgePhase::AwaitingCi,
            merge_commit: None,
            evidence: vec![hash('d')],
            note: None,
            merge_sent: false,
            revision: 1,
        }
    }

    #[test]
    fn the_pull_request_is_the_forges_number_and_what_the_run_published() {
        let pull = pull_of(&forge(Some(142)), Some("crew/task-1"), Some("aaaa"));
        assert_eq!(pull.number, Some(142));
        assert_eq!(pull.branch.as_deref(), Some("crew/task-1"));
        assert_eq!(pull.commit.as_deref(), Some("aaaa"));
    }

    #[test]
    fn an_intent_the_forge_has_not_numbered_yet_names_only_the_branch_and_commit() {
        let pull = pull_of(&forge(None), Some("crew/task-1"), Some("aaaa"));
        assert_eq!(pull.number, None);
        assert_eq!(pull.branch.as_deref(), Some("crew/task-1"));
    }

    #[test]
    fn a_run_with_no_branch_names_the_forge_intents_source() {
        let pull = pull_of(&forge(Some(142)), None, Some("aaaa"));
        assert_eq!(pull.branch.as_deref(), Some("crew/task-1"));
        // A result may carry the field and leave it empty.
        let pull = pull_of(&forge(Some(142)), Some(""), Some("aaaa"));
        assert_eq!(pull.branch.as_deref(), Some("crew/task-1"));
    }
}
