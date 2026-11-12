//! The trip wires of Autopilot (docs/crew.md, "Autopilot"): 3 denials in a row or 20 in a run stop
//! the task, and text that looks like injected instructions stops it too.
//!
//! The injection detector is a short list of patterns over the text an agent's request carries: the
//! summary, the target and every string of the detail. It is a tripwire, not a shield: it
//! catches the blunt, English forms of an injection (an instruction to ignore the rules, a claim of
//! authority or pre-approval, a request to reveal the system prompt, hidden characters). It cannot
//! catch a paraphrase, another language, text split across fields or messages, encoded text, or an
//! instruction that is only implied. The always-denied list, not this detector, is what holds.

use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

/// Denials in a row that stop a task.
pub const IN_A_ROW: u32 = 3;
/// Denials in one run that stop a task.
pub const IN_A_RUN: u32 = 20;

/// The class of a pattern, for the ledger. The text that matched is never recorded.
const PATTERNS: [(&str, &str); 12] = [
    (
        "ignore instructions",
        r"(?i)\b(ignore|disregard|forget|override|bypass)\b[^.\n]{0,40}\b(previous|prior|above|earlier|all|any|your|the)\b[^.\n]{0,30}\b(instructions?|rules?|guidelines?|polic(?:y|ies)|restrictions?|safeguards?)\b",
    ),
    (
        "new instructions",
        r"(?i)\b(new|updated|real|actual)\s+(instructions?|system prompt|orders?)\s*[:\-]",
    ),
    (
        "role change",
        r"(?i)\byou are now\b|\bfrom now on,? you\b|\bact as (?:root|admin|the lead|an? administrator)\b|\bdeveloper mode\b|\bjailbreak",
    ),
    (
        "claimed approval",
        r"(?i)\b(?:the )?(?:lead|owner|user|person|admin|anthropic|openai|system)\s+(?:has|have)\s+(?:already\s+)?(?:approved|authori[sz]ed|allowed)\b|\bpre-?approved\b|\bapproval (?:is )?(?:granted|not needed|unnecessary)\b",
    ),
    (
        "reveal prompt",
        r"(?i)\b(reveal|print|show|repeat|output|leak|send)\b[^.\n]{0,30}\b(system prompt|your instructions|hidden instructions|the answer key|the key|credentials?|api keys?|secrets?)\b",
    ),
    (
        "hide from user",
        r"(?i)\bdo not (?:tell|inform|alert|notify|mention)\b[^.\n]{0,20}\b(user|person|owner|lead)\b|\bwithout (?:telling|informing|asking) the (?:user|person|owner|lead)\b",
    ),
    (
        "chat markup",
        r"(?i)<\|?(?:im_start|im_end|system|endoftext)\|?>|</?(?:system|assistant|instructions?)>|\[/?INST\]",
    ),
    (
        "exfiltrate",
        r"(?i)\bexfiltrat|\bsend (?:it|them|this|the \w+) to https?://",
    ),
    (
        "urgent override",
        r"(?i)\b(?:urgent|critical|emergency)\b[^.\n]{0,40}\b(?:skip|ignore|bypass|without)\b[^.\n]{0,30}\b(?:check|review|approval|verification|tests?)\b",
    ),
    (
        "disable safety",
        r"(?i)\b(?:disable|turn off|remove|skip)\b[^.\n]{0,20}\b(?:safety|guardrails?|sandbox|approvals?|permission checks?|the verifier)\b",
    ),
    (
        "instruction to the model",
        r"(?i)\b(?:assistant|claude|codex|gpt|ai agent|model)\s*[:,]\s*(?:you must|you should|please|now|ignore|run|execute|delete|send)\b",
    ),
    (
        "fake end of text",
        r"(?i)\b(?:end of (?:document|file|output|tool result)|---\s*end\s*---)\b[^\n]{0,40}\b(?:instructions?|now|new)\b",
    ),
];

fn compiled() -> &'static [(&'static str, Regex)] {
    static COMPILED: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        PATTERNS
            .iter()
            .map(|(name, pattern)| (*name, Regex::new(pattern).expect("a fixed pattern")))
            .collect()
    })
}

/// Characters that hide text from a person: zero-width, bidirectional controls, the tag block and other invisible format characters
/// (soft hyphen, combining grapheme joiner, Arabic letter mark, Hangul and Braille fillers, Mongolian free variation selectors,
/// the deprecated format controls and interlinear annotation marks). Emoji variation selectors are left alone: they are legitimate.
fn hidden(c: char) -> bool {
    matches!(c, '\u{00ad}' | '\u{034f}' | '\u{061c}' | '\u{115f}'..='\u{1160}' | '\u{17b4}'..='\u{17b5}' | '\u{180b}'..='\u{180f}'
        | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{2800}' | '\u{3164}' | '\u{feff}' | '\u{ffa0}'
        | '\u{fff9}'..='\u{fffb}' | '\u{e0000}'..='\u{e007f}')
}

/// Whether `text` holds a character that hides text from a person (see `hidden`). Text a person types into a setting is checked with
/// this before it is stored.
pub fn has_hidden(text: &str) -> bool {
    text.chars().any(hidden)
}

/// The class of the first injection pattern in `text`, if any.
pub fn injection_in(text: &str) -> Option<&'static str> {
    if has_hidden(text) {
        return Some("hidden characters");
    }
    compiled()
        .iter()
        .find(|(_, re)| re.is_match(text))
        .map(|(name, _)| *name)
}

/// The same over every string in a JSON value, keys included.
pub fn injection_in_value(value: &Value) -> Option<&'static str> {
    match value {
        Value::String(text) => injection_in(text),
        Value::Array(items) => items.iter().find_map(injection_in_value),
        Value::Object(map) => map
            .iter()
            .find_map(|(key, item)| injection_in(key).or_else(|| injection_in_value(item))),
        _ => None,
    }
}

/// Which trip wire the counts reached, if any.
pub fn tripped(in_a_row: u32, in_a_run: u32) -> bool {
    in_a_row >= IN_A_ROW || in_a_run >= IN_A_RUN
}
