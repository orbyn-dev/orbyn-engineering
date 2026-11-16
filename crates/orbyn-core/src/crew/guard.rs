//! The two checks at the edge where an object enters or leaves the core (SPEC.md
//! section 8): no secret in any string or key (H2), and no answer-key text in anything
//! that is shown (H1). The secret formats are the recognised formats of `crate::redaction`,
//! all of them and always: `Redactor::locked` reads no setting, so nothing a person changes
//! in Settings › Secrets reaches Crew. The reference model in `orbyn-contracts/tests/crew_protocol`
//! states the same patterns; both must keep the minimum classes of the spec.

use orbyn_contracts::{AnswerKey, CrewError, ErrorCode};
use serde_json::Value;

use crate::redaction::Redactor;

fn strings(value: &Value, visit: &mut dyn FnMut(&str)) {
    match value {
        Value::String(text) => visit(text),
        Value::Array(items) => items.iter().for_each(|item| strings(item, visit)),
        Value::Object(map) => {
            for (key, item) in map {
                visit(key);
                if let Value::String(text) = item {
                    // A field named like a credential and holding a long value reads as an assignment.
                    visit(&format!("{key} = {text}"));
                }
                strings(item, visit);
            }
        }
        _ => {}
    }
}

/// The class of the first secret in any string or key, if there is one.
pub fn find_secret(value: &Value) -> Option<&'static str> {
    let mut found = None;
    strings(value, &mut |text| {
        if found.is_none() {
            found = Redactor::locked().class_of(text);
        }
    });
    found
}

/// `secret_detected` when the value holds a secret. The refusal names the class, never the match.
pub fn refuse_secret(value: &Value) -> Result<(), CrewError> {
    match find_secret(value) {
        Some(class) => Err(CrewError::new(
            ErrorCode::SecretDetected,
            format!("a {class} was found in a field; objects hold no secrets"),
        )),
        None => Ok(()),
    }
}

/// What the key must never share: its id and every check command of 8 or more characters.
pub fn key_needles(key: &AnswerKey) -> Vec<String> {
    std::iter::once(key.id.as_str().to_owned())
        .chain(check_needles(&key.checks))
        .collect()
}

/// Commands and the core's internal fix digest must not reach a worker.
pub fn check_needles(checks: &[orbyn_contracts::CrewCheck]) -> Vec<String> {
    let mut needles: Vec<String> = checks
        .iter()
        .filter_map(orbyn_contracts::CrewCheck::command)
        .filter(|run| run.len() >= 8)
        .collect();
    needles.extend(checks.iter().filter_map(|check| {
        (check.params["name"] == "crew-fix-touched")
            .then(|| check.params["note"].as_str().map(str::to_owned))
            .flatten()
            .filter(|digest| !digest.is_empty())
    }));
    needles
}

/// A needle and what it is, for a refusal that names the check and never the text.
pub struct Labeled {
    needle: String,
    what: String,
}

/// The key's needles with their owners: the key's id, and the command of each check by its id.
pub fn key_labeled(key: &AnswerKey) -> Vec<Labeled> {
    std::iter::once(Labeled {
        needle: key.id.as_str().to_owned(),
        what: "the id of the answer key".to_owned(),
    })
    .chain(key.checks.iter().flat_map(|check| {
        check_needles(std::slice::from_ref(check))
            .into_iter()
            .map(|needle| Labeled {
                needle,
                what: format!("the command of check `{}`", check.id.as_str()),
            })
    }))
    .collect()
}

/// Every string with the JSON path it sits at, `root` first (`brief.stop_when[0]`).
/// A key that holds a needle is written `<key>` in the path, so the path never repeats it.
fn strings_at(
    value: &Value,
    path: &str,
    hide: &dyn Fn(&str) -> bool,
    visit: &mut dyn FnMut(&str, &str),
) {
    match value {
        Value::String(text) => visit(path, text),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .for_each(|(i, item)| strings_at(item, &format!("{path}[{i}]"), hide, visit)),
        Value::Object(map) => {
            for (key, item) in map {
                let name = if hide(key) { "<key>" } else { key.as_str() };
                let here = if path.is_empty() {
                    name.to_owned()
                } else {
                    format!("{path}.{name}")
                };
                visit(&here, key);
                strings_at(item, &here, hide, visit);
            }
        }
        _ => {}
    }
}

/// `answer_key_forbidden` when any string of the value holds a needle. The refusal names the field
/// path under `root` and what the text repeats (a check's id), never the text itself: it goes to
/// the lead, and what it quoted could reach a worker.
pub fn refuse_leak_in(value: &Value, root: &str, needles: &[Labeled]) -> Result<(), CrewError> {
    let mut hit: Option<(String, String)> = None;
    let hide = |key: &str| needles.iter().any(|n| key.contains(n.needle.as_str()));
    strings_at(value, root, &hide, &mut |path, text| {
        if hit.is_none()
            && let Some(found) = needles.iter().find(|n| text.contains(n.needle.as_str()))
        {
            hit = Some((path.to_owned(), found.what.clone()));
        }
    });
    match hit {
        Some((path, what)) => Err(CrewError::new(
            ErrorCode::AnswerKeyForbidden,
            format!("{path} repeats {what}; keep check commands out of the brief"),
        )),
        None => Ok(()),
    }
}

/// `answer_key_forbidden` when any string of the value holds a needle; the path is named, the owner is not known.
pub fn refuse_leak(value: &Value, needles: &[String]) -> Result<(), CrewError> {
    let labeled: Vec<Labeled> = needles
        .iter()
        .map(|needle| Labeled {
            needle: needle.clone(),
            what: "answer-key text".to_owned(),
        })
        .collect();
    refuse_leak_in(value, "", &labeled)
}

/// A check's output with every secret of the spec's classes replaced by `[redacted]`. The core
/// does this to an Evidence excerpt before the H2 scan (SPEC.md section 8, "Redaction"); no
/// other field is redacted, so a secret anywhere else is still refused.
pub fn redact(text: &str) -> String {
    Redactor::locked().replace_each(text, "[redacted]")
}

/// `text` with every needle replaced by `[check]`, for output that goes back to a worker: a
/// failing test may print the command that ran it, and the command is part of the key (H1).
pub fn withhold(text: &str, needles: &[String]) -> String {
    needles.iter().fold(text.to_owned(), |text, needle| {
        if needle.is_empty() {
            text
        } else {
            text.replace(needle.as_str(), "[check]")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn labeled(needle: &str, what: &str) -> Vec<Labeled> {
        vec![Labeled {
            needle: needle.to_owned(),
            what: what.to_owned(),
        }]
    }

    #[test]
    fn a_leak_names_the_field_and_the_check_and_never_the_command() {
        let needles = labeled("cargo test -p demo", "the command of check `test-script`");
        let value = json!({ "stop_when": ["done", "when cargo test -p demo passes"] });
        let error = refuse_leak_in(&value, "brief", &needles).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("brief.stop_when[1]"), "{text}");
        assert!(text.contains("check `test-script`"), "{text}");
        assert!(!text.contains("cargo test"), "{text}");
        assert_eq!(error.code, ErrorCode::AnswerKeyForbidden);
    }

    #[test]
    fn a_needle_in_a_key_is_never_repeated_in_the_path() {
        let needles = labeled("cargo test -p demo", "the command of check `t`");
        for value in [
            json!({ "facts": [{}, {}, { "value": { "cargo test -p demo": 1 } }] }),
            json!({ "facts": [{ "value": { "cargo test -p demo": { "deep": "x" } } }] }),
            json!({ "cargo test -p demo": { "cargo test -p demo again": ["y"] } }),
        ] {
            let text = refuse_leak_in(&value, "brief", &needles)
                .unwrap_err()
                .to_string();
            assert!(!text.contains("cargo test"), "{text}");
            assert!(text.contains("<key>") || text.contains("brief."), "{text}");
        }
        let text = refuse_leak_in(
            &json!({ "facts": [{ "value": { "cargo test -p demo": 1 } }] }),
            "brief",
            &needles,
        )
        .unwrap_err()
        .to_string();
        assert!(text.contains("brief.facts[0].value.<key>"), "{text}");
    }

    #[test]
    fn a_clean_value_passes_and_the_plain_form_still_names_the_path() {
        let needles = vec!["cargo test -p demo".to_owned()];
        assert!(refuse_leak(&json!({ "a": ["fine"] }), &needles).is_ok());
        let error = refuse_leak(&json!({ "a": { "b": "x cargo test -p demo" } }), &needles)
            .unwrap_err()
            .to_string();
        assert!(error.contains("a.b"), "{error}");
        assert!(!error.contains("cargo test"), "{error}");
    }
}
