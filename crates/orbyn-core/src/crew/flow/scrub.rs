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
        let safe = withhold(&redact(text), &self.needles);
        // The person's patterns run on the whole text: a cut first could leave the front of a value they no longer match.
        let hidden = self.person.labelled(safe.trim());
        let cut = cut_safely(&hidden, max);
        if refuse_leak(&json!(cut), &self.needles).is_err() {
            return "Withheld.".to_owned();
        }
        cut
    }

    pub(super) fn texts(&self, items: &[String], max: usize) -> Vec<String> {
        items.iter().map(|t| self.text(t, max)).collect()
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
}
