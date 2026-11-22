//! The typed turn result (M20) and the terminal proposal built on it (docs/terminal-input.md).
//!
//! A send may carry a JSON Schema. The engine is never trusted to follow it: the core adds a fixed Orbyn-written
//! instruction to the prompt, and when the run completes it reads the last fenced `json` block of the final assistant
//! message, checks it against the schema and records ONE `orbyn.turn_result` event: the value, or one of a few fixed
//! reasons, never agent text. A terminal proposal run supplies its own schema and, when the value also passes the
//! command rules, adds an `orbyn.proposal` event. Nothing here executes anything and nothing is fetched: the schema
//! compiles with no loader, so a remote `$ref` cannot cause any I/O.

use std::collections::{HashMap, HashSet};

use orbyn_contracts::{AgUiEvent, RunPurpose};
use serde_json::{Value, json};

use crate::error::{CoreError, Result};

/// The most a result schema may serialize to.
pub const MAX_SCHEMA_BYTES: usize = 16 * 1024;
/// The most a result value may be.
pub const MAX_RESULT_BYTES: usize = 256 * 1024;
const MAX_DEPTH: usize = 32;
/// The words of a refused schema. Fixed: the schema's own text is never echoed.
pub const SCHEMA_REFUSED: &str = "The result schema is not valid.";
/// The run event of a typed result.
pub const RESULT_EVENT: &str = "orbyn.turn_result";
/// The run event of a terminal proposal.
pub const PROPOSAL_EVENT: &str = "orbyn.proposal";
const MAX_COMMAND_CHARS: usize = 4096;
const MAX_NOTE_CHARS: usize = 1024;
const SCHEMA_LOC: &str = "orbyn:result-schema";

/// Why a result could not be read, as the fixed codes clients see.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NoJson,
    NotValidJson,
    SchemaMismatch,
    TooLarge,
}

impl Reason {
    pub fn code(self) -> &'static str {
        match self {
            Self::NoJson => "no_json",
            Self::NotValidJson => "not_valid_json",
            Self::SchemaMismatch => "schema_mismatch",
            Self::TooLarge => "too_large",
        }
    }
}

/// What a run was asked to return.
#[derive(Clone, Debug)]
pub struct ResultRequest {
    schema: Value,
    purpose: Option<RunPurpose>,
}

struct NoLoader;

impl boon::UrlLoader for NoLoader {
    fn load(&self, _url: &str) -> std::result::Result<Value, Box<dyn std::error::Error>> {
        Err("a result schema is one self-contained document".into())
    }
}

fn refused() -> CoreError {
    CoreError::Invalid(SCHEMA_REFUSED.into())
}

fn terminal_proposal_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": { "type": "string" },
            "note": { "type": "string" },
        },
        "required": ["command"],
        "additionalProperties": false,
    })
}

/// Deepest nesting, and whether a `$ref` or `$dynamicRef` points anywhere but into the document.
fn inspect(value: &Value, depth: usize) -> std::result::Result<(), ()> {
    if depth > MAX_DEPTH {
        return Err(());
    }
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                if key == "$dynamicRef" || key == "$recursiveRef" {
                    return Err(());
                }
                if key == "$ref" && !item.as_str().is_some_and(|text| text.starts_with('#')) {
                    return Err(());
                }
                inspect(item, depth + 1)?;
            }
            Ok(())
        }
        Value::Array(items) => items.iter().try_for_each(|item| inspect(item, depth + 1)),
        _ => Ok(()),
    }
}

fn compile(schema: &Value) -> std::result::Result<(boon::Schemas, boon::SchemaIndex), ()> {
    let mut compiler = boon::Compiler::new();
    compiler.set_default_draft(boon::Draft::V2020_12);
    compiler.use_loader(Box::new(NoLoader));
    compiler
        .add_resource(SCHEMA_LOC, schema.clone())
        .map_err(|_| ())?;
    let mut schemas = boon::Schemas::new();
    let index = compiler.compile(SCHEMA_LOC, &mut schemas).map_err(|_| ())?;
    Ok((schemas, index))
}

impl ResultRequest {
    /// The request a send's body makes, or none when it makes none. A schema that breaks a rule is refused before the run
    /// is admitted.
    pub fn from_body(schema: Option<Value>, purpose: Option<RunPurpose>) -> Result<Option<Self>> {
        let schema = match (schema, purpose) {
            (None, None) => return Ok(None),
            (Some(_), Some(_)) => {
                return Err(CoreError::Invalid(
                    "Send a result schema or a purpose, not both.".into(),
                ));
            }
            (None, Some(RunPurpose::TerminalProposal)) => terminal_proposal_schema(),
            (Some(schema), None) => {
                let bytes = serde_json::to_vec(&schema).map_err(|_| refused())?;
                if bytes.len() > MAX_SCHEMA_BYTES || !schema.is_object() {
                    return Err(refused());
                }
                inspect(&schema, 0).map_err(|()| refused())?;
                compile(&schema).map_err(|()| refused())?;
                schema
            }
        };
        Ok(Some(Self { schema, purpose }))
    }

    /// The prompt the engine gets: the person's text unchanged, then Orbyn's fixed instruction.
    pub fn instructed(&self, prompt: &str) -> String {
        let schema = serde_json::to_string(&self.schema).unwrap_or_default();
        format!(
            "{prompt}\n\n---\nWhen you finish, end your final message with exactly one fenced ```json block holding the result as JSON that matches the JSON Schema below. Put nothing after the block.\nJSON Schema:\n{schema}"
        )
    }

    /// The value in a final assistant message, checked against the schema.
    pub fn read(&self, text: &str) -> std::result::Result<Value, Reason> {
        let body = extract(text)?;
        if body.len() > MAX_RESULT_BYTES {
            return Err(Reason::TooLarge);
        }
        let value: Value = serde_json::from_str(body).map_err(|_| Reason::NotValidJson)?;
        let (schemas, index) = compile(&self.schema).map_err(|()| Reason::SchemaMismatch)?;
        schemas
            .validate(&value, index)
            .map_err(|_| Reason::SchemaMismatch)?;
        Ok(value)
    }

    /// The events a completed run records for `text`, its final assistant message: the typed result, and for a terminal
    /// proposal run the proposal when the value passes the command rules.
    pub fn events(&self, text: &str) -> Vec<AgUiEvent> {
        let custom = |name: &str, value: Value| AgUiEvent::Custom {
            name: name.into(),
            value,
        };
        match self.read(text) {
            Ok(value) => {
                let mut events = vec![custom(
                    RESULT_EVENT,
                    json!({ "ok": true, "value": value.clone() }),
                )];
                if self.purpose == Some(RunPurpose::TerminalProposal)
                    && let Some(proposal) = proposal(&value)
                {
                    events.push(custom(PROPOSAL_EVENT, proposal));
                }
                events
            }
            Err(reason) => vec![custom(
                RESULT_EVENT,
                json!({ "ok": false, "reason": reason.code() }),
            )],
        }
    }
}

/// The text of the last fenced `json` block, else the whole trimmed message when it reads as JSON.
fn extract(text: &str) -> std::result::Result<&str, Reason> {
    if let Some(start) = text.rfind("```json") {
        let after = &text[start + "```json".len()..];
        let after = after.strip_prefix(['\r', '\n']).unwrap_or(after);
        let end = after.find("```").unwrap_or(after.len());
        return Ok(after[..end].trim());
    }
    let whole = text.trim();
    if whole.starts_with(['{', '[']) {
        Ok(whole)
    } else {
        Err(Reason::NoJson)
    }
}

/// A terminal proposal by the rules of docs/terminal-input.md, or nothing. A value that breaks a rule is dropped whole.
fn proposal(value: &Value) -> Option<Value> {
    let command = value.get("command")?.as_str()?.trim();
    if command.is_empty()
        || command.chars().count() > MAX_COMMAND_CHARS
        || command
            .chars()
            .any(|c| c == '\n' || c == '\r' || (c.is_control() && c != '\t'))
    {
        return None;
    }
    let mut out = json!({ "command": command });
    if let Some(note) = value.get("note") {
        let note = note.as_str()?;
        if note.chars().count() > MAX_NOTE_CHARS {
            return None;
        }
        out["note"] = json!(note);
    }
    Some(out)
}

/// The final assistant message of a turn, kept as the engine's events go by. A user echo is never part of it.
#[derive(Default)]
pub struct AnswerText {
    users: HashSet<String>,
    open: HashMap<String, String>,
    last: Option<String>,
}

impl AnswerText {
    pub fn observe(&mut self, event: &AgUiEvent) {
        match event {
            AgUiEvent::TextMessageStart { message_id, role } => {
                if role == "user" {
                    self.users.insert(message_id.clone());
                } else {
                    self.open.entry(message_id.clone()).or_default();
                }
            }
            AgUiEvent::TextMessageContent { message_id, delta } => {
                if !self.users.contains(message_id) {
                    self.open
                        .entry(message_id.clone())
                        .or_default()
                        .push_str(delta);
                }
            }
            AgUiEvent::TextMessageEnd { message_id } => {
                if let Some(text) = self.open.remove(message_id)
                    && !text.trim().is_empty()
                {
                    self.last = Some(text);
                }
            }
            _ => {}
        }
    }

    /// The last finished assistant message.
    pub fn finish(self) -> Option<String> {
        self.last
    }
}

#[cfg(test)]
mod tests;
