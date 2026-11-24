//! What a Flow screen may read of a stored text: redacted, the answer key's text withheld, the person's own redaction
//! patterns applied, and cut to a length. A text that still holds the key is replaced, never shown (H1, H2).

use std::sync::Arc;

use serde_json::json;

use orbyn_contracts::CrewCheck;

use super::super::guard::{check_needles, key_needles, redact, refuse_leak, withhold};
use crate::app::Core;
use crate::crew::lifecycle::is_final;
use crate::error::{CoreError, Result};
use crate::redaction::{Redactor, cut_safely};

/// Longest name, claim or reason a screen is given, in characters.
pub(super) const TEXT_MAX: usize = 300;

pub(in crate::crew) struct Scrub {
    needles: Vec<String>,
    /// The keys and repros of the crew's tasks that have not ended. A finished task's Run page may name its own commands,
    /// but never one that is still a live task's key.
    check_needles: Vec<String>,
    person: Arc<Redactor>,
}

impl Scrub {
    /// The scrub for one crew: the answer keys of all its tasks and the lead's stored repros are withheld, as everywhere
    /// else in Crew (`crew_key_needles`), because a lead's free-text reason can quote any of them. Read once per call.
    /// Fails when the redaction settings or the keys cannot be read, so a row is not shown unscrubbed.
    pub(in crate::crew) fn of_crew(
        core: &Core,
        person: &Arc<Redactor>,
        crew_id: &str,
    ) -> Result<Self> {
        let needles = core
            .crew_key_needles(crew_id)
            .map_err(|_| CoreError::Internal("the crew could not be read".into()))?;
        Ok(Self {
            needles,
            check_needles: Vec::new(),
            person: Arc::clone(person),
        })
    }

    /// `of_crew`, plus what a Run page's checks must still withhold: the keys and repros of every task of the crew that is
    /// not final, except this page's own task. A finished task names its own checks; a running sibling's key stays secret.
    pub(super) fn of_run(
        core: &Core,
        person: &Arc<Redactor>,
        crew_id: &str,
        task_id: &str,
    ) -> Result<Self> {
        let mut scrub = Self::of_crew(core, person, crew_id)?;
        scrub.check_needles = live_check_needles(core, crew_id, task_id)?;
        Ok(scrub)
    }

    pub(in crate::crew) fn text(&self, text: &str, max: usize) -> String {
        self.marked(text, max).0
    }

    /// As [`Scrub::text`], and whether anything was hidden: the key's text, a built-in secret format or the person's own
    /// pattern. The Run page says so beside a text it quotes.
    pub(in crate::crew) fn marked(&self, text: &str, max: usize) -> (String, bool) {
        let safe = withhold(&redact(text), &self.needles);
        // The person's patterns run on the whole text: a cut first could leave the front of a value they no longer match.
        let trimmed = safe.trim();
        let hidden = self.person.labelled(trimmed);
        // The comparison is against the original text, not its trimmed form: trimming alone did not hide anything.
        let was_hidden = safe != text || hidden != trimmed;
        let cut = cut_safely(&hidden, max);
        if refuse_leak(&json!(cut), &self.needles).is_err() {
            return ("Withheld.".to_owned(), true);
        }
        (cut, was_hidden)
    }

    pub(super) fn texts(&self, items: &[String], max: usize) -> Vec<String> {
        items.iter().map(|t| self.text(t, max)).collect()
    }

    /// A check's own record for the Run page: its command and what it proves, and whether the page cut either at `max`.
    /// The text is redacted and the person's patterns still run. It is the one place Crew names a finished task's own key
    /// text, so it withholds every key and repro of a task that has not ended ([`Scrub::of_run`]); a text that still holds
    /// one is replaced, never shown. Nothing here reaches a worker before the task is final, and the page is a Local read.
    pub(super) fn check_text(&self, text: &str, max: usize) -> (String, bool) {
        let safe = withhold(&redact(text), &self.check_needles);
        let labelled = self.person.labelled(safe.trim());
        let truncated = labelled.chars().count() > max;
        let cut = cut_safely(&labelled, max);
        if refuse_leak(&json!(cut), &self.check_needles).is_err() {
            return ("Withheld.".to_owned(), false);
        }
        (cut, truncated)
    }
}

/// The answer keys and repros of a crew's tasks that have not ended, except `task_id`'s own: what a finished task's Run
/// page must not name. It mirrors `Core::crew_key_needles`, which cannot filter by state once it has flattened the crews.
fn live_check_needles(core: &Core, crew_id: &str, task_id: &str) -> Result<Vec<String>> {
    let mut needles = Vec::new();
    let tasks = core
        .store
        .crew_map_tasks(crew_id, u32::MAX)
        .map_err(|_| CoreError::Internal("the crew could not be read".into()))?;
    for task in tasks {
        if task.id == task_id || is_final(&task.state) {
            continue;
        }
        if let Some(key) = core
            .store
            .crew_answer_key(&task.id)
            .map_err(|_| CoreError::Internal("the crew could not be read".into()))?
        {
            needles.extend(key_needles(&key));
        }
        let repros = core
            .store
            .crew_repros_of(&task.id)
            .map_err(|_| CoreError::Internal("the crew could not be read".into()))?;
        for repro in repros {
            if let Ok(check) = serde_json::from_str::<CrewCheck>(&repro.repro) {
                needles.extend(check_needles(std::slice::from_ref(&check)));
            }
        }
    }
    Ok(needles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction::Custom;
    use orbyn_contracts::PatternScope;

    fn scrub() -> Scrub {
        let acme = Custom {
            name: "Acme key",
            regex: regex::Regex::new("ACME-[A-Z0-9]{32}").unwrap(),
            applies_to: PatternScope::Anywhere,
        };
        Scrub {
            needles: Vec::new(),
            check_needles: Vec::new(),
            person: Arc::new(Redactor::only(vec![acme])),
        }
    }

    #[test]
    fn text_that_only_has_spaces_around_it_is_not_hidden() {
        let (shown, hidden) = scrub().marked("  plain text  ", TEXT_MAX);
        assert_eq!(shown, "plain text");
        assert!(!hidden);
    }

    #[test]
    fn the_persons_pattern_runs_before_the_cut_so_no_fragment_of_a_secret_is_shown() {
        let secret = format!("ACME-{}", "Q9Z8".repeat(8));
        // The secret starts before, at and after the cut, on both sides of 299/300.
        for lead in 270..=300 {
            let text = format!("{}{secret} and more words after it", "a".repeat(lead));
            let shown = scrub().text(&text, TEXT_MAX);
            assert!(
                !shown.contains("ACME") && !shown.contains("Q9Z8"),
                "{lead}: {shown}"
            );
            assert!(shown.chars().count() <= TEXT_MAX, "{lead}");
            assert_eq!(
                shown.matches("[hidden: ").count(),
                shown.matches(']').count(),
                "{lead}: a marker was cut in two: {shown}"
            );
        }
    }

    #[test]
    fn a_checks_command_is_named_and_the_secret_scan_still_runs_on_it() {
        let secret = format!("ACME-{}", "Q9Z8".repeat(8));
        let shown = scrub()
            .check_text(&format!("curl -H x:{secret} example.test"), TEXT_MAX)
            .0;
        assert!(
            !shown.contains("ACME") && !shown.contains("Q9Z8"),
            "{shown}"
        );
        // The command is the key's own text, and naming it is what the Run page is for.
        assert_eq!(
            scrub().check_text("cargo test -p x", TEXT_MAX).0,
            "cargo test -p x"
        );
    }

    #[test]
    fn a_check_text_cut_at_its_cap_says_so() {
        let long = format!("cargo test {}", "a".repeat(TEXT_MAX));
        let (shown, truncated) = scrub().check_text(&long, TEXT_MAX);
        assert!(shown.chars().count() <= TEXT_MAX, "{shown}");
        assert!(truncated);
    }

    #[test]
    fn a_stored_text_that_is_the_keys_own_words_is_withheld() {
        let mut scrub = scrub();
        scrub.needles = vec!["cargo test -p x".to_owned()];
        assert_eq!(scrub.text("cargo test -p x", TEXT_MAX), "[check]");
    }

    #[test]
    fn a_checks_text_that_is_a_live_tasks_key_is_withheld() {
        let mut scrub = scrub();
        scrub.check_needles = vec!["cargo test -p x".to_owned()];
        assert_eq!(scrub.check_text("cargo test -p x", TEXT_MAX).0, "[check]");
    }
}
