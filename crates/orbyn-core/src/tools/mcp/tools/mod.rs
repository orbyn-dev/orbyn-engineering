//! Orbyn's own tools for a running agent, under Orbyn names. Tools reach only
//! the caller's project except the explicitly approved project add/remove surface.
//! Claude's permission prompt passes these tools without
//! asking, so each one with an effect goes through the gate here, as a `tool` action:
//! it needs a person unless the thread is in full access or a rule allows it.

mod dispatch;

mod preview;

mod render;

mod project;

mod guide;

mod definitions;

use serde_json::{Value, json};

use crate::app::Core;
use crate::error::{CoreError, Result};
use definitions::TOOLS;

/// Under the 45 seconds a tool has to answer, so the caller hears `ended: false`.
const MAX_WAIT_SECONDS: u64 = 40;
const MAX_SEARCH_CHARS: usize = 200_000;
const MAX_HITS: usize = 50;
const MAX_BATCH: usize = 16;
/// The tool list for `tools/list`.
pub(crate) fn tool_specs() -> impl Iterator<Item = Value> {
    TOOLS.iter().map(|tool| {
        let properties: serde_json::Map<String, Value> = tool
            .args
            .iter()
            .map(|(name, kind)| ((*name).to_owned(), json!({ "type": kind })))
            .collect();
        json!({
            "name": tool.name,
            "description": tool.description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": tool.required,
            },
        })
    })
}

pub(crate) fn has_effect(name: &str) -> bool {
    TOOLS.iter().any(|tool| tool.name == name && tool.effect)
}

pub(crate) fn is_tool(name: &str) -> bool {
    TOOLS.iter().any(|tool| tool.name == name)
}

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CoreError::Invalid(format!("{key} is required.")))
}

fn number(args: &Value, key: &str) -> Result<u64> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| CoreError::Invalid(format!("{key} is required.")))
}

fn wait_seconds(args: &Value) -> Result<u64> {
    match args.get("seconds") {
        None => Ok(30),
        Some(value) => value
            .as_u64()
            .map(|seconds| seconds.min(MAX_WAIT_SECONDS))
            .ok_or_else(|| CoreError::Invalid("seconds must be a non-negative integer.".into())),
    }
}

/// A pull request number as stored: positive and small enough for SQLite.
fn pull_number(args: &Value) -> Result<u64> {
    let value = number(args, "number")?;
    if value == 0 || value > u64::from(u32::MAX) {
        return Err(CoreError::Invalid(
            "number must be a pull request number.".into(),
        ));
    }
    Ok(value)
}

/// All of the query's words, in any order, ignoring case.
pub(crate) fn matches_words(haystack: &str, query: &str) -> bool {
    let haystack = haystack.to_lowercase();
    let mut words = query.split_whitespace().peekable();
    words.peek().is_some() && words.all(|word| haystack.contains(&word.to_lowercase()))
}

impl Core {}

#[cfg(test)]
mod tests;
