//! What a Flow screen may read of a stored text: redacted, the answer key's text withheld, the person's own redaction
//! patterns applied, and cut to a length. A text that still holds the key is replaced, never shown (H1, H2).

use std::sync::Arc;

use serde_json::json;

use super::super::guard::{redact, refuse_leak, withhold};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::redaction::{Redactor, cut_safely};

/// Longest name, claim or reason a screen is given, in characters.
pub(super) const TEXT_MAX: usize = 300;

pub(super) struct Scrub {
    needles: Vec<String>,
    person: Arc<Redactor>,
}

impl Scrub {
    /// The scrub for one crew: the answer keys of all its tasks and the lead's stored repros are withheld, as everywhere
    /// else in Crew (`crew_key_needles`), because a lead's free-text reason can quote any of them. Read once per call.
    /// Fails when the redaction settings or the keys cannot be read, so a row is not shown unscrubbed.
    pub(super) fn of_crew(core: &Core, person: &Arc<Redactor>, crew_id: &str) -> Result<Self> {
        let needles = core
            .crew_key_needles(crew_id)
            .map_err(|_| CoreError::Internal("the crew could not be read".into()))?;
        Ok(Self {
            needles,
            person: Arc::clone(person),
        })
    }

    pub(super) fn text(&self, text: &str, max: usize) -> String {
        self.marked(text, max).0
    }

    /// As [`Scrub::text`], and whether anything was hidden: the key's text, a built-in secret format or the person's own
    /// pattern. The Run page says so beside a text it quotes.
    pub(super) fn marked(&self, text: &str, max: usize) -> (String, bool) {
        let safe = withhold(&redact(text), &self.needles);
        // The person's patterns run on the whole text: a cut first could leave the front of a value they no longer match.
        let hidden = self.person.labelled(safe.trim());
        // A text that is only trimmed or cut was not hiding anything.
        let was_hidden = hidden != text.trim();
        let cut = cut_safely(&hidden, max);
        if refuse_leak(&json!(cut), &self.needles).is_err() {
            return ("Withheld.".to_owned(), true);
        }
        (cut, was_hidden)
    }

    pub(super) fn texts(&self, items: &[String], max: usize) -> Vec<String> {
        items.iter().map(|t| self.text(t, max)).collect()
    }

    /// A check's own record for the Run page: its command and what it proves. The key's text is *not* withheld here, and
    /// that is the one exception in Crew: the page exists to name the command that ran (`docs/crew-run-page.md`), and the
    /// command and its `proves` line are the answer key's own words, so withholding them would leave the page empty. The
    /// secret scan and the person's patterns still run, and the key's id and its expectations are never part of a check's
    /// record. Nothing here reaches a worker: the route is the person's, and the key never leaves the core.
    pub(super) fn check_text(&self, text: &str, max: usize) -> String {
        let safe = redact(text);
        let labelled = self.person.labelled(safe.trim());
        cut_safely(&labelled, max)
    }
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
            person: Arc::new(Redactor::only(vec![acme])),
        }
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
        let shown = scrub().check_text(&format!("curl -H x:{secret} example.test"), TEXT_MAX);
        assert!(
            !shown.contains("ACME") && !shown.contains("Q9Z8"),
            "{shown}"
        );
        // The command is the key's own text, and naming it is what the Run page is for.
        assert_eq!(
            scrub().check_text("cargo test -p x", TEXT_MAX),
            "cargo test -p x"
        );
    }

    #[test]
    fn a_stored_text_that_is_the_keys_own_words_is_withheld() {
        let mut scrub = scrub();
        scrub.needles = vec!["cargo test -p x".to_owned()];
        assert_eq!(scrub.text("cargo test -p x", TEXT_MAX), "Withheld.");
    }
}
