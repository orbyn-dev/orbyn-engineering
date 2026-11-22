//! Crew Run page (Preview): one work task's own page, read from what this device already stores. Nothing here writes,
//! starts a process or reads a file: the store's read connection answers every list.
//!
//! What it shows is `docs/crew-run-page.md`: the person's prompt, the checks the verifier ran, what was not verified and
//! the pull request the task made. A check's command and what it proves are the key's own words, so they are the one place
//! Crew does not withhold the key's text ([`Scrub::check_text`]): the page exists to name the command that ran, and this
//! route is the person's, never a worker's. The key's id and its expectations are still never in it, and every other text
//! here passes the ordinary scrub, which withholds a text that still holds the key (H1, H2). The web cleans it again.
//!
//! Still missing, and named in `docs/crew-run-page.md`: the plan (no table, object or read stores a task's plan steps), and
//! the forge's own pull request URL and the pull request's current head (a crew pull request's publish records neither, so
//! the page cannot say whether the proof is stale).

use orbyn_contracts as wire;

use super::scrub::Scrub;
use crate::app::Core;
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
    /// One work task's Run page: its prompt, the checks that decided it, what was not verified and the pull request it
    /// made. A read: no state changes, so Autopilot's rule refuses nothing here.
    pub fn crew_flow_run(&self, id: &str) -> Result<wire::FlowRunPage> {
        self.crew_preview_only()?;
        valid_id(id)?;
        let task = self
            .store
            .crew_task(id)?
            .filter(|_| self.store.crew_task_is_work(id).unwrap_or(false))
            .ok_or_else(|| CoreError::NotFound("task".into()))?;
        let person = self.display_redactor()?;
        let facts = self.flow_facts(task)?;
        let scrub = Scrub::of_crew(self, &person, &facts.task.crew_id)?;
        let result = self.store.crew_results(&facts.task.id)?.pop();
        let mut truncated = false;

        let prompt = facts.brief.as_ref().map(|brief| {
            let (text, hidden) = scrub.marked(&brief.outcome, PROMPT_MAX);
            wire::FlowRunPrompt {
                text,
                hidden,
                truncated: brief.outcome.chars().count() > PROMPT_MAX,
            }
        });

        // The latest round's evidence is what decided the task: a fix's, when it had one.
        truncated |= facts.evidence.len() > CHECKS_MAX;
        let checks: Vec<wire::FlowRunCheck> = facts
            .evidence
            .iter()
            .take(CHECKS_MAX)
            .map(|e| wire::FlowRunCheck {
                label: scrub.check_text(e.command.as_str(), CHECK_TEXT_MAX),
                outcome: wire_name(&e.outcome),
                exit_code: e.exit_code,
                duration_ms: e.duration_ms,
                proves: scrub.check_text(e.proves.as_str(), CHECK_TEXT_MAX),
                commit: e.commit.as_str().to_owned(),
            })
            .collect();
        let mut not_verified = Vec::new();
        if let Some(result) = &result {
            truncated |= result.not_verified.len() > NOT_VERIFIED_MAX;
            not_verified = result
                .not_verified
                .iter()
                .take(NOT_VERIFIED_MAX)
                .map(|text| scrub.text(text.as_str(), CHECK_TEXT_MAX))
                .collect();
        }

        // The commit the proof covers: the result's, else the last check's.
        let commit = result
            .as_ref()
            .and_then(|r| r.commit.as_ref())
            .map(|c| c.as_str().to_owned())
            .or_else(|| facts.evidence.last().map(|e| e.commit.as_str().to_owned()));
        let branch = result
            .as_ref()
            .and_then(|r| r.branch.as_ref())
            .map(|b| b.as_str().to_owned());

        // Only the forge intent says Orbyn opened a pull request for this task: no match by branch name or title.
        let pull = self
            .store
            .crew_forge_merge(&facts.task.id)?
            .map(|forge| pull_of(&forge, branch.as_deref(), commit.as_deref()));

        Ok(wire::FlowRunPage {
            task: facts.task.id.clone(),
            crew: facts.task.crew_id.clone(),
            state: facts.task.state.clone(),
            finished: crate::crew::lifecycle::is_final(&facts.task.state),
            prompt,
            checks,
            not_verified,
            commit,
            pull,
            truncated,
        })
    }
}

/// The pull request one task made, from its forge intent and what the run published.
fn pull_of(forge: &ForgeMerge, branch: Option<&str>, commit: Option<&str>) -> wire::FlowRunPull {
    wire::FlowRunPull {
        number: forge.pull.and_then(|n| u32::try_from(n).ok()),
        branch: branch.map(str::to_owned),
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
}
