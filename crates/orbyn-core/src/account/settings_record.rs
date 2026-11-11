//! The `engineering.settings` record: which preferences sync, how each becomes one or
//! more fields, and the field-name hashing for elements. The Engine's
//! `orbyn-engineering` crate is meant to own these names and limits
//! ([docs/account.md](../../../../docs/account.md#the-settings-record)); until it is
//! pinned, this module is the one place the app spells them, so swapping is one file.

use std::collections::BTreeSet;

use orbyn_contracts::KeybindingRule;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const ZONE: &str = "engineering.settings";

/// The 20 allowlisted catalog keys. Keybindings are the twenty-first group of fields.
pub const SYNCED_KEYS: [&str; 20] = [
    "default_theme",
    "response_streaming_mode",
    "preview_appearance",
    "preview_zoom",
    "open_web_links_in_app",
    "quit_confirmation",
    "default_engine",
    "default_model",
    "default_effort",
    "default_model_options",
    "fallback_model",
    "text_generation_model",
    "favorite_models",
    "hidden_models",
    "model_order",
    "default_thread_env_mode",
    "auto_thread_titles",
    "branch_naming_mode",
    "branch_name_prefix",
    "branch_name_instructions",
];

/// Keys with set semantics: one field per element, removal is a tombstone.
pub const SET_KEYS: [&str; 2] = ["favorite_models", "hidden_models"];

/// The field-name prefix of keybinding rules.
pub const KEYBINDINGS: &str = "keybindings";

pub const MAX_FIELD_NAME: usize = 64;
pub const MAX_VALUE_BYTES: usize = 8 * 1024;
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
pub const MAX_LIST: usize = 500;

/// A field ready to stage, or one over a limit that stays local.
#[derive(Clone, Debug, PartialEq)]
pub enum Staged {
    Field { name: String, value: Value },
    Skipped { name: String },
}

pub fn is_synced_key(key: &str) -> bool {
    SYNCED_KEYS.contains(&key)
}

pub fn is_set_key(key: &str) -> bool {
    SET_KEYS.contains(&key)
}

/// A field name is at most 64 characters from `a-z 0-9 _ . :`.
pub fn valid_field_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FIELD_NAME
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b':')
        })
}

/// The first 16 characters of the lowercase base32 of the SHA-256 of an identity, so
/// any model id or key chord fits the field alphabet.
fn hash16(identity: &str) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let digest = Sha256::digest(identity.as_bytes());
    let mut out = String::with_capacity(16);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in digest.iter() {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 && out.len() < 16 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buffer >> bits) & 31) as usize]));
        }
        if out.len() == 16 {
            break;
        }
    }
    out
}

/// The field of one element of a set key.
pub fn element_name(key: &str, id: &str) -> String {
    format!("{key}:{}", hash16(id))
}

/// The field of one keybinding rule: one per key and `when`.
pub fn keybinding_name(key: &str, when: Option<&str>) -> String {
    format!(
        "{KEYBINDINGS}:{}",
        hash16(&format!("{key}\u{1f}{}", when.unwrap_or_default()))
    )
}

/// The set key a field name belongs to, if it is an element field.
pub fn set_key_of(name: &str) -> Option<&'static str> {
    SET_KEYS.into_iter().find(|key| {
        name.strip_prefix(*key)
            .is_some_and(|rest| rest.starts_with(':'))
    })
}

pub fn is_keybinding_name(name: &str) -> bool {
    name.strip_prefix(KEYBINDINGS)
        .is_some_and(|rest| rest.starts_with(':'))
}

fn fits(name: String, value: Value) -> Staged {
    if valid_field_name(&name) && value.to_string().len() <= MAX_VALUE_BYTES {
        Staged::Field { name, value }
    } else {
        Staged::Skipped { name }
    }
}

fn strings(value: Option<&Value>) -> BTreeSet<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The fields one person-level preference change stages. `old` and `new` are the
/// stored values; `new` is `None` when the value went back to its default. A key
/// outside the allowlist stages nothing, so the four that must never sync cannot.
pub fn stage_preference(key: &str, old: Option<&Value>, new: Option<&Value>) -> Vec<Staged> {
    if !is_synced_key(key) {
        return Vec::new();
    }
    if is_set_key(key) {
        let after = strings(new);
        if after.len() > MAX_LIST {
            return vec![Staged::Skipped {
                name: key.to_owned(),
            }];
        }
        let before = strings(old);
        let added = after
            .difference(&before)
            .map(|id| (id, json!({ "id": id })));
        let removed = before
            .difference(&after)
            .map(|id| (id, json!({ "id": id, "removed": true })));
        return added
            .chain(removed)
            .map(|(id, value)| fits(element_name(key, id), value))
            .collect();
    }
    if old == new {
        return Vec::new();
    }
    if new
        .and_then(Value::as_array)
        .is_some_and(|list| list.len() > MAX_LIST)
    {
        return vec![Staged::Skipped {
            name: key.to_owned(),
        }];
    }
    vec![fits(key.to_owned(), new.cloned().unwrap_or(Value::Null))]
}

/// The field one keybinding rule stages.
pub fn stage_keybinding(rule: &KeybindingRule) -> Staged {
    let value = json!({
        "key": rule.key,
        "command": rule.command,
        "when": rule.when,
        "remove": rule.remove,
    });
    fits(keybinding_name(&rule.key, rule.when.as_deref()), value)
}

/// The rule a keybinding field holds, if its name is the hash of its own identity.
pub fn keybinding_of(name: &str, value: &Value) -> Option<KeybindingRule> {
    let rule = KeybindingRule {
        key: value.get("key")?.as_str()?.to_owned(),
        command: value.get("command")?.as_str()?.to_owned(),
        when: match value.get("when") {
            None | Some(Value::Null) => None,
            Some(when) => Some(when.as_str()?.to_owned()),
        },
        remove: value
            .get("remove")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    (keybinding_name(&rule.key, rule.when.as_deref()) == name).then_some(rule)
}

/// The element a set field holds and whether it was removed, if the name is the hash
/// of the element's own id.
pub fn element_of(name: &str, value: &Value) -> Option<(&'static str, String, bool)> {
    let key = set_key_of(name)?;
    let id = value.get("id")?.as_str()?;
    let removed = value
        .get("removed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (!id.is_empty() && id.len() <= 256 && element_name(key, id) == name)
        .then(|| (key, id.to_owned(), removed))
}

/// The size a record uploads as: every name and value.
pub fn record_bytes(record: &super::merge::Record) -> usize {
    record
        .iter()
        .map(|(name, field)| name.len() + field.value.to_string().len() + 48)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_twenty_unique_synced_keys_all_in_the_catalog() {
        let unique: BTreeSet<_> = SYNCED_KEYS.iter().collect();
        assert_eq!(unique.len(), 20);
        let catalog: BTreeSet<_> = crate::settings::preferences::catalog_keys().collect();
        assert_eq!(catalog.len(), 62);
        assert!(SYNCED_KEYS.iter().all(|key| catalog.contains(key)));
        // The four that escalate, and the stamp, are never staged.
        for never in [
            "default_permission_mode",
            "default_project_scripts",
            "enable_agent_browser_access",
            "enable_agent_device_access",
            "default_theme_set_at",
        ] {
            assert!(
                stage_preference(never, None, Some(&json!("x"))).is_empty(),
                "{never}"
            );
        }
    }

    #[test]
    fn an_element_name_is_16_hash_characters_inside_the_alphabet() {
        for id in [
            "gpt-5",
            "a model id with spaces and ünïcode and /slashes/ that is far longer than sixty-four characters in total",
            "",
        ] {
            let name = element_name("favorite_models", id);
            assert!(valid_field_name(&name), "{name}");
            assert_eq!(name.len(), "favorite_models:".len() + 16);
        }
        assert_eq!(
            element_name("favorite_models", "a"),
            element_name("favorite_models", "a")
        );
        assert_ne!(
            element_name("favorite_models", "a"),
            element_name("favorite_models", "b")
        );
        assert_ne!(
            element_name("favorite_models", "a"),
            element_name("hidden_models", "a")
        );
        // A known vector: the RFC 4648 base32 of SHA-256("a"), lowercased, cut to 16.
        assert_eq!(hash16("a"), "zklycewkdo64v6wc");
    }

    #[test]
    fn a_set_stages_one_field_per_added_element_and_a_tombstone_per_removed_one() {
        let old = json!(["a", "b"]);
        let new = json!(["b", "c"]);
        let staged = stage_preference("favorite_models", Some(&old), Some(&new));
        assert_eq!(staged.len(), 2);
        assert!(staged.contains(&Staged::Field {
            name: element_name("favorite_models", "c"),
            value: json!({ "id": "c" }),
        }));
        assert!(staged.contains(&Staged::Field {
            name: element_name("favorite_models", "a"),
            value: json!({ "id": "a", "removed": true }),
        }));
        // Nothing changed, nothing staged.
        assert!(stage_preference("favorite_models", Some(&new), Some(&new)).is_empty());
        // Resetting to the default removes every element.
        assert_eq!(stage_preference("hidden_models", Some(&old), None).len(), 2);
    }

    #[test]
    fn two_devices_adding_different_models_keep_both() {
        use super::super::merge::{Field, Record, Stamp, merge_records};
        let element = |name: String, device: &str| {
            let Staged::Field { name, value } =
                stage_preference("favorite_models", None, Some(&json!([name]))).remove(0)
            else {
                unreachable!()
            };
            (
                name,
                Field {
                    value,
                    stamp: Stamp {
                        wall_ms: 1,
                        counter: 0,
                        device: device.into(),
                    },
                },
            )
        };
        let a: Record = [element("m1".into(), "a")].into();
        let b: Record = [element("m2".into(), "b")].into();
        assert_eq!(merge_records(&a, &b).record.len(), 2);
    }

    #[test]
    fn a_plain_key_stages_its_value_and_null_when_reset() {
        assert_eq!(
            stage_preference("default_effort", None, Some(&json!("high"))),
            vec![Staged::Field {
                name: "default_effort".into(),
                value: json!("high")
            }]
        );
        assert_eq!(
            stage_preference("default_effort", Some(&json!("high")), None),
            vec![Staged::Field {
                name: "default_effort".into(),
                value: Value::Null
            }]
        );
    }

    #[test]
    fn limits_keep_a_setting_local() {
        let big = json!("x".repeat(MAX_VALUE_BYTES + 1));
        assert_eq!(
            stage_preference("branch_name_instructions", None, Some(&big)),
            vec![Staged::Skipped {
                name: "branch_name_instructions".into()
            }]
        );
        let many: Vec<String> = (0..=MAX_LIST).map(|n| format!("m{n}")).collect();
        assert!(matches!(
            stage_preference("favorite_models", None, Some(&json!(many)))[..],
            [Staged::Skipped { .. }]
        ));
        assert!(matches!(
            stage_preference("model_order", None, Some(&json!(many)))[..],
            [Staged::Skipped { .. }]
        ));
    }

    #[test]
    fn a_keybinding_has_one_field_per_key_and_when_and_names_its_own_hash() {
        let rule = KeybindingRule {
            key: "mod+y".into(),
            command: "sidebar.toggle".into(),
            when: Some("!terminalFocus".into()),
            remove: false,
        };
        let Staged::Field { name, value } = stage_keybinding(&rule) else {
            panic!()
        };
        assert_eq!(keybinding_of(&name, &value), Some(rule.clone()));
        let other_when = KeybindingRule {
            when: None,
            ..rule.clone()
        };
        assert_ne!(
            name,
            match stage_keybinding(&other_when) {
                Staged::Field { name, .. } => name,
                Staged::Skipped { name } => name,
            }
        );
        // A value filed under someone else's name is refused.
        assert_eq!(keybinding_of("keybindings:aaaaaaaaaaaaaaaa", &value), None);
    }
}
