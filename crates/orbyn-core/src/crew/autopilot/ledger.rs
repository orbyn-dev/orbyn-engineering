//! The approval ledger, written by the core only (docs/crew.md: "Every approval and denial is in
//! the ledger with its reason"). A row holds the action's kind, target and paths, who decided,
//! the rule or the reasons and the tier. Before a row is stored, every string is redacted for
//! secrets and has the answer key's text withheld, and the row is scanned again; a row that still
//! fails is stored without its free text rather than not at all. A ledger that cannot be written
//! never allows an action: the caller denies.

use serde_json::{Value, json};

use crate::app::Core;
use crate::crew::{redact, refuse_leak, refuse_secret, withhold};
use crate::error::Result;
use crate::store::CrewTask;

const LONGEST: usize = 400;

fn clean(value: &Value, needles: &[String]) -> Value {
    match value {
        Value::String(text) => {
            let text = withhold(&redact(text), needles);
            Value::String(text.chars().take(LONGEST).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| clean(v, needles)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), clean(item, needles)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The row as it is stored: cleaned, then scanned; free text is dropped if the scan still objects.
pub(super) fn safe(body: &Value, needles: &[String]) -> Value {
    let cleaned = clean(body, needles);
    if refuse_secret(&cleaned).is_ok() && refuse_leak(&cleaned, needles).is_ok() {
        cleaned
    } else {
        json!({ "withheld": "the row failed the secret or answer-key check" })
    }
}

/// The words the requester reads: redacted, the key's text withheld, and never empty.
pub(super) fn shown(reason: &str, needles: &[String]) -> String {
    let text = withhold(&redact(reason), needles);
    let text: String = text.trim().chars().take(300).collect();
    if text.is_empty() || refuse_leak(&json!(text), needles).is_err() {
        "No reason can be shown.".to_owned()
    } else {
        text
    }
}

impl Core {
    /// Appends one row. An error is the caller's to turn into a denial.
    pub(super) fn autopilot_record(
        &self,
        task: &CrewTask,
        kind: &str,
        body: &Value,
        needles: &[String],
    ) -> Result<i64> {
        self.store
            .add_crew_autopilot_event(&task.id, &task.crew_id, kind, &safe(body, needles))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_loses_a_secret_and_the_key_text_before_it_is_stored() {
        let needles = vec!["orbyn-key-marker-77".to_owned()];
        let body = json!({
            "reason": "token = abcdefgh12345678 and orbyn-key-marker-77 ran",
            "nested": [format!("ghp_{}", "a1".repeat(19))],
        });
        let stored = safe(&body, &needles).to_string();
        assert!(!stored.contains("abcdefgh12345678"));
        assert!(!stored.contains("orbyn-key-marker-77"));
        assert!(!stored.contains("ghp_"));
        assert_eq!(shown("", &needles), "No reason can be shown.");
    }
}
