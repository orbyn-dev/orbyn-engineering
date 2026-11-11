//! The two checks at the edge where an object enters or leaves the core (SPEC.md
//! section 8): no secret in any string or key (H2), and no answer-key text in anything
//! that is shown (H1). The reference model in `orbyn-contracts/tests/crew_protocol`
//! states the same patterns; both must keep the minimum classes of the spec.

use std::sync::OnceLock;

use orbyn_contracts::{AnswerKey, CrewError, ErrorCode};
use regex::Regex;
use serde_json::Value;

const PATTERNS: [(&str, &str); 9] = [
    ("private key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
    ("api key", r"\bsk-[A-Za-z0-9_-]{20,}"),
    ("github token", r"\bgh[pousr]_[A-Za-z0-9]{36,}"),
    ("github token", r"\bgithub_pat_[A-Za-z0-9_]{22,}"),
    ("aws key", r"\bAKIA[0-9A-Z]{16}\b"),
    ("slack token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
    ("bearer token", r"(?i)\bbearer\s+[A-Za-z0-9._~+/-]{20,}=*"),
    (
        "jwt",
        r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
    ),
    (
        "credential assignment",
        r#"(?i)\b(password|passwd|secret|api[_-]?key|token)["']?\s*[:=]\s*["']?[^\s"']{8,}"#,
    ),
];

fn patterns() -> &'static [(&'static str, Regex)] {
    static COMPILED: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        PATTERNS
            .iter()
            .map(|(name, pattern)| (*name, Regex::new(pattern).expect("a fixed pattern")))
            .collect()
    })
}

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
            found = patterns()
                .iter()
                .find(|(_, pattern)| pattern.is_match(text))
                .map(|(name, _)| *name);
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
    patterns()
        .iter()
        .fold(text.to_owned(), |text, (_, pattern)| {
            pattern.replace_all(&text, "[redacted]").into_owned()
        })
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
