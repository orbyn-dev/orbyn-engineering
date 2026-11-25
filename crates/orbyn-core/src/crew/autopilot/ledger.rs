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
use crate::redaction::{Redactor, cut_safely};
use crate::store::CrewTask;

const LONGEST: usize = 400;

fn clean(value: &Value, needles: &[String], person: &Redactor) -> Value {
    match value {
        Value::String(text) => {
            // The person's patterns see the whole text; the cut never leaves part of a value behind.
            let text = person.labelled(&withhold(&redact(text), needles));
            Value::String(cut_safely(&text, LONGEST))
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| clean(v, needles, person)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), clean(item, needles, person)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The row as it is stored: cleaned, then scanned; free text is dropped if the scan still objects.
/// The stored row is never masked by the person's own patterns (see `safe_masked`).
pub(in crate::crew) fn safe(body: &Value, needles: &[String]) -> Value {
    safe_masked(body, needles, &Redactor::default())
}

/// `safe` with the person's patterns applied to the text before it is cut, for a row that is shown.
pub(super) fn safe_masked(body: &Value, needles: &[String], person: &Redactor) -> Value {
    let cleaned = clean(body, needles, person);
    if refuse_secret(&cleaned).is_ok() && refuse_leak(&cleaned, needles).is_ok() {
        cleaned
    } else {
        json!({ "withheld": "the row failed the secret or answer-key check" })
    }
}

/// The words the requester reads: redacted, the key's text withheld, and never empty.
pub(super) fn shown(reason: &str, needles: &[String]) -> String {
    shown_masked(reason, needles, &Redactor::default())
}

/// `shown` with the person's patterns applied before the text is cut.
pub(in crate::crew) fn shown_masked(reason: &str, needles: &[String], person: &Redactor) -> String {
    let text = person.labelled(withhold(&redact(reason), needles).trim());
    let text = cut_safely(&text, 300);
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
        // Every row names the work it was for, so a reader is not left with a task id: the lead thread's title as
        // it stood at the write. It goes in before `safe` like any other value, so it is redacted and cut with the
        // rest, and a title that cannot be read leaves the row without one: it decides nothing.
        let mut body = body.clone();
        if let Some(title) = self.autopilot_task_title(task) {
            body["title"] = Value::String(title);
        }
        self.store
            .add_crew_autopilot_event(&task.id, &task.crew_id, kind, &safe(&body, needles))
    }

    /// The task's title as the person sees it: the lead thread's title for the run the task belongs to. A failed
    /// read is `None`, never an error: the ledger row matters more than its title.
    fn autopilot_task_title(&self, task: &CrewTask) -> Option<String> {
        let run = self.store.crew_run(&task.id).ok().flatten()?;
        let title = self.store.thread(&run.lead_thread_id).ok()?.title;
        let title = title.trim().to_owned();
        (!title.is_empty()).then_some(title)
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

    #[test]
    fn a_cut_never_leaves_the_front_of_a_value_behind() {
        let secret = format!("ACME-{}", "Q9Z8".repeat(8));
        // The secret straddles the 400 (stored) and 300 (shown) cuts, either side of the edge.
        for lead in [372, 390, 398, 400] {
            let text = format!("{}{secret} tail", "a ".repeat(lead / 2));
            let stored = safe(&json!({ "reason": text }), &[]).to_string();
            assert!(
                !stored.contains("ACME") && !stored.contains("Q9Z8"),
                "{lead}"
            );
        }
        for lead in [272, 290, 298, 300] {
            let text = format!("{}{secret} tail", "a ".repeat(lead / 2));
            let shown = shown(&text, &[]);
            assert!(!shown.contains("ACME") && !shown.contains("Q9Z8"), "{lead}");
        }
    }

    #[test]
    fn a_shown_row_has_the_persons_patterns_applied_before_it_is_cut() {
        use crate::redaction::Custom;
        let person = Redactor::only(vec![Custom {
            name: "Acme key",
            regex: regex::Regex::new("ACME-[A-Z0-9]{32}").unwrap(),
            applies_to: orbyn_contracts::PatternScope::Anywhere,
        }]);
        let secret = format!("ACME-{}", "Q9Z8".repeat(8));
        for lead in [272, 280, 290, 298] {
            let text = format!("{}{secret} tail", "a ".repeat(lead / 2));
            let reason = shown_masked(&text, &[], &person);
            assert!(
                !reason.contains("ACME") && !reason.contains("Q9Z8"),
                "{lead}"
            );
            assert_eq!(
                reason.matches("[hidden: ").count(),
                reason.matches(']').count()
            );
        }
        let text = format!("{}{secret} tail", "a ".repeat(190));
        let row = safe_masked(&json!({ "reason": text }), &[], &person).to_string();
        assert!(!row.contains("ACME") && !row.contains("Q9Z8"));
        assert!(row.contains("[hidden: Acme key]"));
    }

    /// A person's value longer than the whole cut: the cut lands inside the match in the
    /// stored text, so a row or a shown reason must carry the marker, never the value's head.
    fn assert_no_value_fragment(cut: &str, cap: usize, value: &str) {
        assert!(!cut.contains("ACME"), "{cut}");
        let chars: Vec<char> = value.chars().collect();
        for window in chars.windows(16) {
            let window: String = window.iter().collect();
            assert!(!cut.contains(&window), "{window} survived in {cut}");
        }
        assert!(cut.chars().count() <= cap, "{cut}");
        assert_eq!(
            cut.matches("[hidden: ").count(),
            cut.matches(']').count(),
            "a marker was split: {cut}"
        );
    }

    #[test]
    fn a_person_value_longer_than_the_cut_never_shows_its_head() {
        use crate::redaction::Custom;
        let person = Redactor::only(vec![Custom {
            name: "Acme key",
            regex: regex::Regex::new("ACME-[A-Z0-9]{400}").unwrap(),
            applies_to: orbyn_contracts::PatternScope::Anywhere,
        }]);
        let secret = format!("ACME-{}", "Q9Z8".repeat(100));
        let other = format!("ACME-{}", "X7W6".repeat(100));
        assert!(secret.chars().count() > LONGEST);
        // The stored row's 400 cut, then the shown reason's 300 cut. The match starts at,
        // before and just before each cut and runs past it; a second value sits wholly
        // after. At 390/290 the cut falls in the marker the match became.
        for (cap, lead, stored, marker_shown) in [
            (LONGEST, 0usize, true, true),
            (LONGEST, 100, true, true),
            (LONGEST, 390, true, false),
            (300usize, 0, false, true),
            (300, 100, false, true),
            (300, 290, false, false),
        ] {
            let text = format!(
                "{}{secret} {} {other} tail",
                "a ".repeat(lead / 2),
                "b".repeat(cap + 100)
            );
            let cut = if stored {
                safe_masked(&json!({ "reason": text }), &[], &person)["reason"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            } else {
                shown_masked(&text, &[], &person)
            };
            assert_no_value_fragment(&cut, cap, &secret);
            assert_no_value_fragment(&cut, cap, &other);
            assert_eq!(
                cut.contains("[hidden: Acme key]"),
                marker_shown,
                "{cap}/{lead}: {cut}"
            );
        }

        // Dropping the run a cut lands in does not drop a word before it: a value whose
        // match crosses a space only stays out of the row if it was hidden before the cut.
        let spaced = format!("ACME KEY-{}", "Q9Z8".repeat(100));
        let spaced_person = Redactor::only(vec![Custom {
            name: "Acme key",
            regex: regex::Regex::new("ACME KEY-[A-Z0-9]{400}").unwrap(),
            applies_to: orbyn_contracts::PatternScope::Anywhere,
        }]);
        for (cap, lead, stored, marker_shown) in [
            (LONGEST, 0usize, true, true),
            (LONGEST, 390, true, false),
            (300usize, 0, false, true),
            (300, 290, false, false),
        ] {
            let text = format!(
                "{}{spaced} {} tail",
                "a ".repeat(lead / 2),
                "b".repeat(cap + 100)
            );
            let cut = if stored {
                safe_masked(&json!({ "reason": text }), &[], &spaced_person)["reason"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            } else {
                shown_masked(&text, &[], &spaced_person)
            };
            assert_no_value_fragment(&cut, cap, &spaced);
            assert_eq!(
                cut.contains("[hidden: Acme key]"),
                marker_shown,
                "{cap}/{lead}: {cut}"
            );
        }
    }
}
