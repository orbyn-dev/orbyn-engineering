//! What a Flow screen may read of a stored text: redacted, the answer key's text withheld, the person's own redaction
//! patterns applied, and cut to a length. A text that still holds the key is replaced, never shown (H1, H2).

use std::sync::Arc;

use serde_json::json;

use super::super::guard::{key_needles, redact, refuse_leak, withhold};
use crate::app::Core;
use crate::error::Result;
use crate::redaction::Redactor;

/// Longest name, claim or reason a screen is given, in characters.
pub(super) const TEXT_MAX: usize = 300;

pub(super) struct Scrub {
    needles: Vec<String>,
    person: Arc<Redactor>,
}

impl Scrub {
    /// The scrub for one task: its key's text is withheld. Fails when the redaction settings cannot be read, so a row
    /// is not shown without the patterns the person asked to hide.
    pub(super) fn of(core: &Core, person: &Arc<Redactor>, task_id: &str) -> Result<Self> {
        let needles = core
            .store
            .crew_answer_key(task_id)?
            .map(|key| key_needles(&key))
            .unwrap_or_default();
        Ok(Self {
            needles,
            person: Arc::clone(person),
        })
    }

    pub(super) fn text(&self, text: &str, max: usize) -> String {
        let safe = withhold(&redact(text), &self.needles);
        let cut: String = safe.trim().chars().take(max).collect();
        if refuse_leak(&json!(cut), &self.needles).is_err() {
            return "Withheld.".to_owned();
        }
        self.person.labelled(&cut)
    }

    pub(super) fn texts(&self, items: &[String], max: usize) -> Vec<String> {
        items.iter().map(|t| self.text(t, max)).collect()
    }
}
