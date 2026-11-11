//! Allowlist-only apply: what a merged record changes on this device. Only the 13
//! allowlisted keys and keybindings are applied, each through the same value check a
//! local write gets and the per-key rules in [`super::policy`]. Every other field is
//! left in the record, which keeps it for the next upload; this module never reads it.
//! The plan is a pure function of the record, so applying twice changes nothing.

use std::collections::{BTreeMap, BTreeSet};

use orbyn_contracts::KeybindingRule;
use serde_json::Value;

use super::merge::Record;
use super::policy;
use super::settings_record as names;

/// Added and removed elements of one set key.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SetEdit {
    pub add: BTreeSet<String>,
    pub remove: BTreeSet<String>,
}

#[derive(Debug, Default, PartialEq)]
pub struct ApplyPlan {
    /// A whole value per key; `None` is a reset to the default.
    pub values: BTreeMap<String, Option<Value>>,
    pub sets: BTreeMap<String, SetEdit>,
    pub keybindings: Vec<KeybindingRule>,
    /// Allowlisted fields refused by a rule: kept and passed on, never applied.
    pub refused: usize,
}

/// What `record` asks this device to apply. `record` holds only fields from the account
/// that were not applied yet, so what the person changed here since is never undone.
pub fn plan(record: &Record) -> ApplyPlan {
    let mut plan = ApplyPlan::default();
    for (name, field) in record {
        let value = &field.value;
        if names::is_keybinding_name(name) {
            match names::keybinding_of(name, value) {
                Some(rule) if policy::keybinding_allowed(&rule) => plan.keybindings.push(rule),
                _ => plan.refused += 1,
            }
        } else if names::set_key_of(name).is_some() {
            match names::element_of(name, value) {
                Some((key, id, removed)) => {
                    let edit = plan.sets.entry(key.to_owned()).or_default();
                    let target = if removed {
                        &mut edit.remove
                    } else {
                        &mut edit.add
                    };
                    target.insert(id);
                }
                None => plan.refused += 1,
            }
        } else if names::is_synced_key(name) && !names::is_set_key(name) {
            match accepted(name, value) {
                Some(kept) => {
                    plan.values.insert(name.clone(), kept);
                }
                None => plan.refused += 1,
            }
        }
        // Anything else is not ours to apply: kept in the record, passed on unchanged.
    }
    plan
}

/// The value to store for a plain key: `Some(None)` is a reset, `None` a refusal.
fn accepted(key: &str, value: &Value) -> Option<Option<Value>> {
    if value.is_null() {
        return Some(None);
    }
    crate::settings::preferences::check_synced(key, value).ok()?;
    let kept = match key {
        "branch_name_prefix" => {
            policy::branch_prefix_is_ref_part(value.as_str()?).then(|| value.clone())?
        }
        _ => value.clone(),
    };
    Some(Some(kept))
}

/// `current` with a set edit applied: removals first, then additions at the end, up to
/// the list limit. Additions past it are not applied.
pub fn edited_list(current: Option<&Value>, edit: &SetEdit) -> Vec<Value> {
    let mut list: Vec<Value> = current
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    list.retain(|item| item.as_str().is_none_or(|id| !edit.remove.contains(id)));
    for id in &edit.add {
        if list.len() >= names::MAX_LIST {
            break;
        }
        if !list.iter().any(|item| item.as_str() == Some(id)) {
            list.push(Value::String(id.clone()));
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::merge::{Field, Stamp};
    use super::*;

    fn field(value: Value) -> Field {
        Field {
            value,
            stamp: Stamp {
                wall_ms: 1,
                counter: 0,
                device: "peer".into(),
            },
        }
    }

    fn record(fields: Vec<(String, Value)>) -> Record {
        fields
            .into_iter()
            .map(|(name, value)| (name, field(value)))
            .collect()
    }

    #[test]
    fn an_allowlisted_value_is_planned_and_a_bad_one_is_refused() {
        let plan = plan(&record(vec![
            ("preview_appearance".into(), json!("dark")),
            ("response_streaming_mode".into(), json!("nonsense")),
            ("preview_zoom".into(), json!(1.5)),
            ("default_theme".into(), Value::Null),
        ]));
        assert_eq!(plan.values["preview_appearance"], Some(json!("dark")));
        assert_eq!(plan.values["preview_zoom"], Some(json!(1.5)));
        assert_eq!(plan.values["default_theme"], None);
        assert!(!plan.values.contains_key("response_streaming_mode"));
        assert_eq!(plan.refused, 1);
    }

    #[test]
    fn a_branch_prefix_that_is_not_a_ref_part_is_refused() {
        for bad in ["--upload-pack=x", "a b", "a..b"] {
            let plan = plan(&record(vec![("branch_name_prefix".into(), json!(bad))]));
            assert!(plan.values.is_empty(), "{bad}");
        }
        let plan = plan(&record(vec![(
            "branch_name_prefix".into(),
            json!("team/feat"),
        )]));
        assert_eq!(plan.values["branch_name_prefix"], Some(json!("team/feat")));
    }

    #[test]
    fn set_elements_and_keybindings_are_planned_from_their_own_identity() {
        let mut fields = Vec::new();
        for (id, removed) in [("a", false), ("b", true)] {
            let value = if removed {
                json!({"id": id, "removed": true})
            } else {
                json!({"id": id})
            };
            fields.push((names::element_name("favorite_models", id), value));
        }
        // A value filed under a name that is not its own hash is refused.
        fields.push((
            names::element_name("favorite_models", "x"),
            json!({"id": "y"}),
        ));
        let rule = KeybindingRule {
            key: "mod+y".into(),
            command: "sidebar.toggle".into(),
            when: None,
            remove: false,
        };
        let names::Staged::Field { name, value } = names::stage_keybinding(&rule) else {
            panic!()
        };
        fields.push((name, value));
        let plan = plan(&record(fields));
        assert_eq!(
            plan.sets["favorite_models"].add,
            BTreeSet::from(["a".to_owned()])
        );
        assert_eq!(
            plan.sets["favorite_models"].remove,
            BTreeSet::from(["b".to_owned()])
        );
        assert_eq!(plan.keybindings, vec![rule]);
        assert_eq!(plan.refused, 1);
        let list = edited_list(Some(&json!(["b", "c"])), &plan.sets["favorite_models"]);
        assert_eq!(list, vec![json!("c"), json!("a")]);
        // A peer cannot grow a list past the limit.
        let many: Vec<Value> = (0..names::MAX_LIST)
            .map(|n| json!(format!("m{n}")))
            .collect();
        let more = SetEdit {
            add: BTreeSet::from(["extra".to_owned()]),
            remove: BTreeSet::new(),
        };
        assert_eq!(
            edited_list(Some(&Value::Array(many)), &more).len(),
            names::MAX_LIST
        );
    }

    #[test]
    fn a_keybinding_to_an_approval_command_or_a_dialog_key_is_not_applied() {
        let mut fields = Vec::new();
        for (key, command) in [
            ("mod+y", "approval.approve"),
            ("mod+n", "permissions.setMode"),
            ("enter", "sidebar.toggle"),
            ("escape", "navigation.back"),
            ("mod+u", "composer.sendAlternate"),
        ] {
            let rule = KeybindingRule {
                key: key.into(),
                command: command.into(),
                when: None,
                remove: false,
            };
            let names::Staged::Field { name, value } = names::stage_keybinding(&rule) else {
                panic!()
            };
            fields.push((name, value));
        }
        let plan = plan(&record(fields));
        assert!(plan.keybindings.is_empty());
        assert_eq!(plan.refused, 5);
    }

    #[test]
    fn every_field_outside_the_allowlist_is_ignored_by_the_plan() {
        let mut fields = Vec::new();
        for key in crate::settings::preferences::catalog_keys() {
            if !names::is_synced_key(key) {
                fields.push((key.to_owned(), json!("anything")));
            }
        }
        assert_eq!(fields.len(), 51);
        fields.push(("a_future_key".into(), json!(1)));
        let plan = plan(&record(fields));
        assert_eq!(plan, ApplyPlan::default());
    }
}
