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

/// The command of every check of 8 or more characters: what a repro check, kept apart from any key, must never share
/// with a worker either.
pub fn check_needles(checks: &[orbyn_contracts::CrewCheck]) -> Vec<String> {
    checks
        .iter()
        .filter_map(orbyn_contracts::CrewCheck::command)
        .filter(|run| run.len() >= 8)
        .collect()
}

/// `answer_key_forbidden` when any string of the value holds a needle.
pub fn refuse_leak(value: &Value, needles: &[String]) -> Result<(), CrewError> {
    let mut leaked = false;
    strings(value, &mut |text| {
        leaked = leaked || needles.iter().any(|needle| text.contains(needle.as_str()));
    });
    if leaked {
        return Err(CrewError::new(
            ErrorCode::AnswerKeyForbidden,
            "the answer key is never part of a route",
        ));
    }
    Ok(())
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
