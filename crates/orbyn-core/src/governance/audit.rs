//! The audit log as a person reads it. Rows are written by many sources and stored as written; the
//! person's own redaction patterns are applied to a row's stored JSON detail when it is read, never
//! to what is stored (the same rule the Crew ledger follows). Settings that do not read fail the
//! read: a row is not shown without the patterns the person asked to hide.

use orbyn_contracts::AuditEvent;

use crate::app::Core;
use crate::error::Result;

impl Core {
    /// The newest rows. Only `detail` is masked: its values and its keys both go through the
    /// person's patterns (`Core::display_redactor`), because a secret can sit in either. The fields
    /// a screen reads — actor, action, target, decision, source, thread and run — are as stored.
    pub fn audit_events(&self, thread_id: Option<&str>, limit: u32) -> Result<Vec<AuditEvent>> {
        let person = self.display_redactor()?;
        let mut rows = self.store.list_audit(thread_id, limit)?;
        for row in &mut rows {
            row.detail = person.mask_json(&row.detail);
        }
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use orbyn_contracts::{PatternKind, PatternScope, RedactionPatternInput};
    use serde_json::json;

    use crate::store::AuditRecord;
    use crate::testing::TempDir;
    use crate::{Core, CoreConfig};

    const ACTOR: &str = "local";

    fn core(dir: &TempDir) -> Core {
        let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        Core::open(config).unwrap()
    }

    fn row(action: &str, detail: serde_json::Value) -> AuditRecord {
        AuditRecord {
            actor: ACTOR.into(),
            action: action.into(),
            target: "cargo test".into(),
            decision: None,
            source: "unknown".into(),
            thread_id: None,
            run_id: None,
            detail,
        }
    }

    fn pattern(core: &Core, name: &str, text: String) {
        core.save_redaction_pattern(
            ACTOR,
            None,
            &RedactionPatternInput {
                name: name.into(),
                pattern: text,
                kind: PatternKind::Text,
                applies_to: PatternScope::Anywhere,
                accept_broad: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn a_secret_in_a_stored_detail_is_masked_on_read_in_a_value_and_in_a_key() {
        let dir = TempDir::new();
        let core = core(&dir);
        // A synthetic canary, never a real secret.
        let canary = format!("{}-{}", "zebra", "canary");
        let mut nested = serde_json::Map::new();
        nested.insert(canary.clone(), json!(["a", 1, true, null]));
        core.store
            .append_audit(&row(
                "permission.ask",
                json!({
                    "note": format!("the {canary} field was read"),
                    "nested": serde_json::Value::Object(nested),
                }),
            ))
            .unwrap();
        // The row is stored as written: reading it back from the store shows the canary.
        let stored = |core: &Core| {
            core.store
                .list_audit(None, 50)
                .unwrap()
                .into_iter()
                .find(|event| event.action == "permission.ask")
                .unwrap()
                .detail
        };
        let before = stored(&core);
        assert!(before.to_string().contains(&canary));
        pattern(&core, "Canary", canary.clone());

        let shown = core.audit_events(None, 200).unwrap();
        let text = serde_json::to_string(&shown).unwrap();
        assert!(!text.contains(&canary), "a detail value or key was shown");
        assert!(text.contains("[hidden: Canary]"));
        // The fields a screen reads stay readable, and so do the ordinary keys around the canary.
        assert!(text.contains("\"action\":\"permission.ask\""));
        for key in ["note", "nested", "cargo test"] {
            assert!(text.contains(key), "the field {key} was over-masked");
        }
        // The read never rewrites what is stored.
        assert_eq!(stored(&core), before);
    }

    #[test]
    fn settings_that_do_not_read_fail_the_audit_read() {
        let dir = TempDir::new();
        let core = core(&dir);
        core.store
            .append_audit(&row("permission.ask", json!({ "note": "nothing" })))
            .unwrap();
        core.store.set_setting("redaction", "{ not json").unwrap();
        assert!(
            core.audit_events(None, 200).is_err(),
            "the rows were shown without the patterns the person asked to hide"
        );
    }
}
