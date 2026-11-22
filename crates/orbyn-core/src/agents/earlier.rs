//! Reading the messages an imported thread had before it was imported, from the session
//! file the engine wrote. See docs/attachments-transcripts.md, "Earlier messages of an
//! imported thread (K36)".

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use orbyn_contracts::{EarlierMessage, EarlierPage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::agents::history_import::find_session_file;
use crate::agents::usage::{Engine, history_root};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::redaction::Redactor;

/// Messages in one page.
const PAGE: usize = 50;
/// Bytes of one message, including the ellipsis a cut leaves.
const MAX_TEXT_BYTES: usize = 16 * 1024;
/// The largest session file read, matching the largest transcript read in this group.
const MAX_SESSION_BYTES: u64 = 256 * 1024 * 1024;
/// Lines read before the file is refused as too large for this read.
const MAX_SESSION_LINES: usize = 1_000_000;
/// Messages kept before the file is refused as too large for this read.
const MAX_MESSAGES: usize = 200_000;
/// A cursor is a few dozen bytes; anything longer was not written here.
const MAX_CURSOR_BYTES: usize = 512;
const CURSOR_VERSION: u8 = 1;
const ELLIPSIS: &str = "…";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mapped {
    pub role: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Cursor {
    v: u8,
    offset: usize,
    size: u64,
    mtime_ns: u64,
    check: String,
}

fn cursor_check(offset: usize, size: u64, mtime_ns: u64) -> String {
    let payload = format!("{CURSOR_VERSION}|{offset}|{size}|{mtime_ns}");
    crate::util::hex(&Sha256::digest(payload.as_bytes()))
}

fn unreadable() -> CoreError {
    CoreError::Refused {
        status: 422,
        code: "history_unreadable",
        message:
            "The agent's earlier history could not be read. The session file has moved or changed."
                .into(),
        detail: Value::Null,
    }
}

fn bad_cursor() -> CoreError {
    CoreError::Refused {
        status: 422,
        code: "bad_cursor",
        message: "The earlier-messages cursor is not one Orbyn wrote.".into(),
        detail: Value::Null,
    }
}

fn not_imported() -> CoreError {
    CoreError::Refused {
        status: 404,
        code: "not_imported",
        message: "This thread was not imported from an agent's history.".into(),
        detail: Value::Null,
    }
}

fn provider_engine(name: &str) -> Option<Engine> {
    match name {
        "claude" => Some(Engine::Claude),
        "codex" => Some(Engine::Codex),
        _ => None,
    }
}

fn source_name(engine: Engine) -> &'static str {
    match engine {
        Engine::Claude => "Claude Code",
        Engine::Codex => "Codex",
        // No Muse history is imported, so this read never answers for one.
        Engine::Muse => "Muse",
    }
}

fn mtime_ns(time: Option<SystemTime>) -> u64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
        })
}

/// The import made the thread at `imported_at`; a session file written later has changed
/// since the import, so its earlier messages are refused rather than guessed.
fn changed_since_import(imported_at: &str, modified: Option<SystemTime>) -> bool {
    let Some(modified) = modified else {
        return false;
    };
    let Ok(modified) = modified.duration_since(UNIX_EPOCH) else {
        return false;
    };
    let Ok(imported) =
        time::OffsetDateTime::parse(imported_at, &time::format_description::well_known::Rfc3339)
    else {
        return false;
    };
    i128::try_from(modified.as_nanos())
        .is_ok_and(|modified| modified > imported.unix_timestamp_nanos())
}

fn encode_cursor(offset: usize, size: u64, mtime_ns: u64) -> String {
    let cursor = Cursor {
        v: CURSOR_VERSION,
        offset,
        size,
        mtime_ns,
        check: cursor_check(offset, size, mtime_ns),
    };
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap_or_default())
}

fn decode_cursor(text: &str) -> Result<Cursor> {
    if text.len() > MAX_CURSOR_BYTES {
        return Err(bad_cursor());
    }
    let bytes = URL_SAFE_NO_PAD.decode(text).map_err(|_| bad_cursor())?;
    if bytes.len() > MAX_CURSOR_BYTES {
        return Err(bad_cursor());
    }
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| bad_cursor())?;
    if cursor.v != CURSOR_VERSION
        || cursor.check != cursor_check(cursor.offset, cursor.size, cursor.mtime_ns)
    {
        return Err(bad_cursor());
    }
    Ok(cursor)
}

/// The message text a client receives: redacted first, then cut at 16 KiB with an ellipsis.
fn wire_text(redactor: &Redactor, text: &str) -> String {
    let redacted = redactor.labelled(text);
    if redacted.len() <= MAX_TEXT_BYTES {
        return redacted;
    }
    let limit = MAX_TEXT_BYTES - ELLIPSIS.len();
    let mut at = limit;
    while !redacted.is_char_boundary(at) {
        at -= 1;
    }
    let mut head = &redacted[..at];
    // A cut inside a `[hidden: label]` marker would show a broken label, so drop that marker.
    if let Some(open) = head
        .rfind("[hidden: ")
        .filter(|open| !head[*open..].contains(']'))
    {
        head = &head[..open];
    }
    let mut cut = head.trim_end().to_owned();
    cut.push_str(ELLIPSIS);
    cut
}

fn meta_text(text: &str) -> bool {
    text.trim_start().starts_with('<')
}

fn flush_text(text: &mut String, out: &mut Vec<Mapped>, role: &str) {
    let joined = text.trim();
    if !joined.is_empty() {
        out.push(Mapped {
            role: role.to_owned(),
            text: joined.to_owned(),
        });
    }
    text.clear();
}

fn push_part(out: &mut Vec<Mapped>, role: &str, text: &mut String, part: &str) {
    flush_text(text, out, role);
    if !part.is_empty() {
        out.push(Mapped {
            role: role.to_owned(),
            text: part.to_owned(),
        });
    }
}

fn safe_tool_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-' | '.') {
                c
            } else {
                ' '
            }
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned.chars().take(40).collect()
}

fn tool_line(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let line = if lower.contains("web_search") || lower.contains("websearch") {
        "Searched the web"
    } else if lower.contains("webfetch") || lower.contains("web_fetch") || lower.contains("fetch") {
        "Fetched a page"
    } else if lower.contains("view_image") || lower.contains("image") {
        "Looked at an image"
    } else if lower.contains("todo") || lower.contains("update_plan") || lower.contains("plan") {
        "Updated the plan"
    } else if lower.contains("task") || lower.contains("subagent") || lower.contains("agent") {
        "Started an agent"
    } else if lower.contains("bash")
        || lower.contains("shell")
        || lower.contains("exec")
        || lower.contains("command")
        || lower.contains("terminal")
    {
        "Ran a command"
    } else if lower.contains("edit")
        || lower.contains("write")
        || lower.contains("patch")
        || lower.contains("notebook")
    {
        "Edited a file"
    } else if lower.contains("grep")
        || lower.contains("glob")
        || lower.contains("search")
        || lower.contains("list")
    {
        "Searched the files"
    } else if lower.contains("read")
        || lower.contains("view")
        || lower.contains("open")
        || lower.contains("cat")
    {
        "Read a file"
    } else {
        let safe = safe_tool_name(name);
        if safe.is_empty() {
            return "Used a tool".to_owned();
        }
        return format!("Used {safe}");
    };
    line.to_owned()
}

fn image_part(kind: &str) -> Option<&'static str> {
    if kind.contains("image") {
        Some("[image]")
    } else if kind.contains("file")
        || kind.contains("document")
        || kind.contains("audio")
        || kind.contains("video")
        || kind.contains("binary")
    {
        Some("[file]")
    } else {
        None
    }
}

fn version_major(text: &str) -> Option<u64> {
    let text = text.trim().trim_start_matches('v');
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn field_major(value: &Value, key: &str) -> Option<u64> {
    let value = value.get(key)?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(version_major))
}

fn declared_too_new(value: &Value) -> bool {
    [
        "format_version",
        "formatVersion",
        "schema_version",
        "schemaVersion",
    ]
    .iter()
    .any(|key| field_major(value, key).is_some_and(|major| major > 1))
}

fn claude_too_new(value: &Value) -> bool {
    declared_too_new(value) || field_major(value, "version").is_some_and(|major| major > 2)
}

fn codex_too_new(value: &Value) -> bool {
    if declared_too_new(value) {
        return true;
    }
    let payload = value.get("payload").unwrap_or(&Value::Null);
    if declared_too_new(payload) {
        return true;
    }
    field_major(payload, "cli_version").is_some_and(|major| major > 0)
        || field_major(payload, "version").is_some_and(|major| major > 1)
        || field_major(payload, "format_version").is_some_and(|major| major > 1)
}

struct Group {
    key: Option<String>,
    messages: Vec<Mapped>,
}

fn claude_content(role: &str, content: Option<&Value>, skip_meta: bool) -> Vec<Mapped> {
    let mut out = Vec::new();
    let Some(content) = content else {
        return out;
    };
    if let Some(text) = content.as_str() {
        let text = text.trim();
        if !text.is_empty() && !(skip_meta && meta_text(text)) {
            out.push(Mapped {
                role: role.to_owned(),
                text: text.to_owned(),
            });
        }
        return out;
    }
    let Some(parts) = content.as_array() else {
        return out;
    };
    let mut text = String::new();
    for part in parts {
        let kind = part.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "text" => {
                if let Some(value) = part.get("text").and_then(Value::as_str)
                    && !(skip_meta && meta_text(value))
                {
                    if !text.is_empty() {
                        text.push_str("\n\n");
                    }
                    text.push_str(value.trim());
                }
            }
            "tool_use" | "server_tool_use" | "mcp_tool_use" => {
                flush_text(&mut text, &mut out, role);
                let name = part.get("name").and_then(Value::as_str).unwrap_or("");
                out.push(Mapped {
                    role: "tool".into(),
                    text: tool_line(name),
                });
            }
            "thinking" | "redacted_thinking" | "tool_result" => {}
            other => {
                if let Some(placeholder) = image_part(other) {
                    push_part(&mut out, role, &mut text, placeholder);
                }
            }
        }
    }
    flush_text(&mut text, &mut out, role);
    out
}

fn parse_claude(reader: impl BufRead) -> Result<Vec<Mapped>> {
    let mut groups: Vec<Group> = Vec::new();
    let mut entries = 0usize;
    let mut recognized = 0usize;
    let mut count = 0usize;
    for (line_no, line) in reader.lines().enumerate() {
        if line_no >= MAX_SESSION_LINES {
            return Err(unreadable());
        }
        let line = line.map_err(|_| unreadable())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !value.is_object() {
            continue;
        }
        entries += 1;
        if claude_too_new(&value) {
            return Err(unreadable());
        }
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        let known = matches!(
            kind,
            "user"
                | "assistant"
                | "system"
                | "summary"
                | "queue-operation"
                | "progress"
                | "file-history-snapshot"
                | "custom-title"
                | "agent-name"
                | "permission-mode"
                | "last-prompt"
        );
        if known {
            recognized += 1;
        }
        if value.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let messages = match kind {
            "assistant" => {
                let key = value
                    .pointer("/message/id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let messages =
                    claude_content("assistant", value.pointer("/message/content"), false);
                if messages.is_empty() {
                    continue;
                }
                if let Some(key) = key.as_deref()
                    && let Some(group) = groups
                        .iter_mut()
                        .find(|group| group.key.as_deref() == Some(key))
                {
                    count = count.saturating_sub(group.messages.len());
                    count += messages.len();
                    group.messages = messages;
                    if count > MAX_MESSAGES {
                        return Err(unreadable());
                    }
                    continue;
                }
                Group { key, messages }
            }
            "user" => {
                let messages = claude_content("user", value.pointer("/message/content"), true);
                if messages.is_empty() {
                    continue;
                }
                Group {
                    key: None,
                    messages,
                }
            }
            _ => continue,
        };
        count += messages.messages.len();
        if count > MAX_MESSAGES {
            return Err(unreadable());
        }
        groups.push(messages);
    }
    if entries == 0 || recognized == 0 {
        return Err(unreadable());
    }
    let mut out = Vec::new();
    for group in groups {
        out.extend(group.messages);
    }
    Ok(out)
}

fn codex_content(role: &str, content: Option<&Value>, skip_meta: bool) -> Vec<Mapped> {
    let mut out = Vec::new();
    let Some(content) = content else {
        return out;
    };
    if let Some(text) = content.as_str() {
        let text = text.trim();
        if !text.is_empty() && !(skip_meta && meta_text(text)) {
            out.push(Mapped {
                role: role.to_owned(),
                text: text.to_owned(),
            });
        }
        return out;
    }
    let Some(parts) = content.as_array() else {
        return out;
    };
    let mut text = String::new();
    for part in parts {
        let kind = part.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "input_text" | "output_text" | "text" => {
                if let Some(value) = part.get("text").and_then(Value::as_str)
                    && !(skip_meta && meta_text(value))
                {
                    if !text.is_empty() {
                        text.push_str("\n\n");
                    }
                    text.push_str(value.trim());
                }
            }
            other => {
                if let Some(placeholder) = image_part(other) {
                    push_part(&mut out, role, &mut text, placeholder);
                }
            }
        }
    }
    flush_text(&mut text, &mut out, role);
    out
}

fn push_codex(out: &mut Vec<Mapped>, messages: Vec<Mapped>) -> Result<()> {
    for message in messages {
        if out
            .last()
            .is_some_and(|last| last.role == message.role && last.text == message.text)
        {
            continue;
        }
        out.push(message);
        if out.len() > MAX_MESSAGES {
            return Err(unreadable());
        }
    }
    Ok(())
}

fn parse_codex(reader: impl BufRead) -> Result<Vec<Mapped>> {
    let mut out = Vec::new();
    let mut entries = 0usize;
    let mut recognized = 0usize;
    for (line_no, line) in reader.lines().enumerate() {
        if line_no >= MAX_SESSION_LINES {
            return Err(unreadable());
        }
        let line = line.map_err(|_| unreadable())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if !value.is_object() {
            continue;
        }
        entries += 1;
        if codex_too_new(&value) {
            return Err(unreadable());
        }
        let payload = value.get("payload").unwrap_or(&Value::Null);
        match value.get("type").and_then(Value::as_str).unwrap_or("") {
            "session_meta" | "turn_context" | "compacted" | "world_state" | "event_msg"
            | "response_item" => recognized += 1,
            _ => continue,
        }
        match value.get("type").and_then(Value::as_str).unwrap_or("") {
            "event_msg" => {
                let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
                let messages = match kind {
                    "user_message" => {
                        let mut messages = Vec::new();
                        if let Some(text) = payload.get("message").and_then(Value::as_str)
                            && !text.trim().is_empty()
                            && !meta_text(text)
                        {
                            messages.push(Mapped {
                                role: "user".into(),
                                text: text.trim().to_owned(),
                            });
                        }
                        if messages.is_empty()
                            && payload
                                .get("images")
                                .and_then(Value::as_array)
                                .is_some_and(|images| !images.is_empty())
                        {
                            messages.push(Mapped {
                                role: "user".into(),
                                text: "[image]".into(),
                            });
                        }
                        messages
                    }
                    "agent_message" => payload
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .map(|text| {
                            vec![Mapped {
                                role: "assistant".into(),
                                text: text.to_owned(),
                            }]
                        })
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                push_codex(&mut out, messages)?;
            }
            "response_item" => {
                let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
                let messages = match kind {
                    "message" => {
                        let role = payload.get("role").and_then(Value::as_str).unwrap_or("");
                        match role {
                            "user" => codex_content("user", payload.get("content"), true),
                            "assistant" => {
                                codex_content("assistant", payload.get("content"), false)
                            }
                            _ => Vec::new(),
                        }
                    }
                    "function_call" | "custom_tool_call" => {
                        let name = payload
                            .get("name")
                            .or_else(|| payload.get("tool"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        vec![Mapped {
                            role: "tool".into(),
                            text: tool_line(name),
                        }]
                    }
                    "local_shell_call" => vec![Mapped {
                        role: "tool".into(),
                        text: "Ran a command".into(),
                    }],
                    "web_search_call" => vec![Mapped {
                        role: "tool".into(),
                        text: "Searched the web".into(),
                    }],
                    "mcp_tool_call" => {
                        let name = payload
                            .get("tool")
                            .or_else(|| payload.get("name"))
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        vec![Mapped {
                            role: "tool".into(),
                            text: tool_line(name),
                        }]
                    }
                    _ => Vec::new(),
                };
                push_codex(&mut out, messages)?;
            }
            _ => {}
        }
    }
    if entries == 0 || recognized == 0 {
        return Err(unreadable());
    }
    Ok(out)
}

pub(crate) fn parse_session(engine: Engine, reader: impl BufRead) -> Result<Vec<Mapped>> {
    match engine {
        Engine::Claude => parse_claude(reader),
        Engine::Codex => parse_codex(reader),
        Engine::Muse => Err(unreadable()),
    }
}

impl Core {
    /// The messages an imported thread had before it was imported, from the engine's own
    /// session file. Read only: it writes no row and nothing in the provider's folder.
    pub fn earlier_messages(&self, thread_id: &str, before: Option<&str>) -> Result<EarlierPage> {
        let thread = self.store.thread(thread_id)?;
        let Some(engine) = provider_engine(&thread.engine) else {
            return Err(not_imported());
        };
        if thread.source.is_some()
            || thread.imported_at.is_none()
            || self.store.engine_session(&thread.id)?.is_none()
        {
            return Err(not_imported());
        }
        let Some(root) = history_root(engine) else {
            return Err(unreadable());
        };
        self.earlier_messages_in(&thread.id, before, &root)
    }

    fn earlier_messages_in(
        &self,
        thread_id: &str,
        before: Option<&str>,
        root: &Path,
    ) -> Result<EarlierPage> {
        let thread = self.store.thread(thread_id)?;
        let Some(engine) = provider_engine(&thread.engine) else {
            return Err(not_imported());
        };
        let Some(imported_at) = thread.imported_at.clone() else {
            return Err(not_imported());
        };
        if thread.source.is_some() {
            return Err(not_imported());
        }
        let Some(session) = self.store.engine_session(&thread.id)? else {
            return Err(not_imported());
        };
        let Some(path) = find_session_file(engine, root, session.as_str()) else {
            return Err(unreadable());
        };
        let metadata = std::fs::metadata(&path).map_err(|_| unreadable())?;
        let size = metadata.len();
        let modified = metadata.modified().ok();
        let mtime = mtime_ns(modified);
        let requested_end = match before {
            None => None,
            Some(text) => {
                let cursor = decode_cursor(text)?;
                if cursor.size != size || cursor.mtime_ns != mtime {
                    return Err(unreadable());
                }
                Some(cursor.offset)
            }
        };
        if changed_since_import(imported_at.as_str(), modified) {
            return Err(unreadable());
        }
        if !metadata.is_file() || size > MAX_SESSION_BYTES {
            return Err(unreadable());
        }
        let file = std::fs::File::open(&path).map_err(|_| unreadable())?;
        let reader = BufReader::new(file.take(MAX_SESSION_BYTES + 1));
        let parsed = parse_session(engine, reader)?;
        let after = std::fs::metadata(&path).map_err(|_| unreadable())?;
        if after.len() != size || mtime_ns(after.modified().ok()) != mtime {
            return Err(unreadable());
        }
        let redactor = self.export_redactor().map_err(|_| unreadable())?;
        let mut messages = Vec::with_capacity(parsed.len());
        for message in parsed {
            let text = wire_text(&redactor, &message.text);
            if text.is_empty() {
                continue;
            }
            messages.push(EarlierMessage {
                role: message.role,
                text,
            });
        }
        let total = messages.len();
        let end = match requested_end {
            Some(offset) => {
                if offset > total || (total - offset) % PAGE != 0 {
                    return Err(bad_cursor());
                }
                offset
            }
            None => total,
        };
        let start = end.saturating_sub(PAGE);
        let page = messages[start..end].to_vec();
        let next_before = (start > 0).then(|| encode_cursor(start, size, mtime));
        tracing::debug!(
            thread_id = %thread.id,
            messages = page.len(),
            total,
            "read earlier messages"
        );
        Ok(EarlierPage {
            source: source_name(engine).to_owned(),
            total: u32::try_from(total).unwrap_or(u32::MAX),
            imported_at,
            next_before,
            messages: page,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::path::Path;

    use serde_json::{Value, json};

    use super::*;
    use crate::agents::usage::Engine;
    use crate::store::{NewProject, NewThread};
    use crate::{CoreConfig, testing::TempDir};

    fn core(dir: &TempDir) -> Core {
        let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        Core::open(config).unwrap()
    }

    fn project(core: &Core, dir: &TempDir) -> String {
        let path = dir.path().join("work");
        std::fs::create_dir_all(&path).unwrap();
        core.store
            .add_project(NewProject {
                path: path.to_str().unwrap(),
                name: "work",
                is_git: false,
                managed: false,
                scratch: false,
            })
            .unwrap()
            .id
    }

    fn imported(core: &Core, project_id: &str, cwd: &str, engine: &str, session: &str) -> String {
        let (thread, _) = core
            .store
            .import_native_thread(
                NewThread {
                    project_id: project_id.to_owned(),
                    title: "Imported".into(),
                    engine: engine.into(),
                    model: None,
                    cwd: cwd.into(),
                    worktree: None,
                    skill_profile: None,
                    parent_thread_id: None,
                    relation: None,
                    scratch: false,
                },
                session,
                &[],
            )
            .unwrap();
        thread.id
    }

    fn write(path: &Path, lines: &[Value]) -> Vec<u8> {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let text: Vec<String> = lines.iter().map(ToString::to_string).collect();
        let bytes = text.join("\n").into_bytes();
        std::fs::write(path, &bytes).unwrap();
        bytes
    }

    fn claude_user(cwd: &str, session: &str, text: &str) -> Value {
        json!({
            "type": "user", "sessionId": session, "cwd": cwd,
            "message": {"role": "user", "content": [{"type": "text", "text": text}]},
        })
    }

    fn parse(engine: Engine, text: &str) -> Result<Vec<Mapped>> {
        parse_session(engine, Cursor::new(text.as_bytes()))
    }

    #[test]
    fn claude_maps_text_thinking_tools_images_and_meta() {
        let text = [
            json!({"type": "summary", "summary": "ignored"}),
            json!({"type": "user", "isSidechain": true, "message": {"content": "SUBAGENT"}}),
            json!({"type": "user", "message": {"role": "user", "content": [
                {"type": "text", "text": "hello"},
                {"type": "tool_result", "tool_use_id": "t0", "content": "OUTPUT_SENTINEL"}
            ]}}),
            json!({"type": "assistant", "message": {"id": "a1", "role": "assistant", "content": [
                {"type": "thinking", "thinking": "THINKING_SENTINEL"},
                {"type": "text", "text": "answer"},
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/ARG_SENTINEL"}},
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}
            ]}}),
            // Claude writes an assistant message repeatedly while it streams; the last line stands.
            json!({"type": "assistant", "message": {"id": "a2", "role": "assistant", "content": [
                {"type": "text", "text": "streamed"}
            ]}}),
            json!({"type": "assistant", "message": {"id": "a2", "role": "assistant", "content": [
                {"type": "text", "text": "complete"}
            ]}}),
            json!({"type": "system", "content": "META_SENTINEL"}),
        ]
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        let messages = parse(Engine::Claude, &text).unwrap();
        let pairs: Vec<(&str, &str)> = messages
            .iter()
            .map(|message| (message.role.as_str(), message.text.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("user", "hello"),
                ("assistant", "answer"),
                ("tool", "Read a file"),
                ("assistant", "[image]"),
                ("assistant", "complete"),
            ]
        );
        assert!(!format!("{messages:?}").contains("SUBAGENT"));
        assert!(!format!("{messages:?}").contains("THINKING_SENTINEL"));
        assert!(!format!("{messages:?}").contains("ARG_SENTINEL"));
        assert!(!format!("{messages:?}").contains("OUTPUT_SENTINEL"));
        assert!(!format!("{messages:?}").contains("META_SENTINEL"));
    }

    #[test]
    fn codex_maps_event_messages_tools_thinking_and_duplicates() {
        let text = [
            json!({"type": "session_meta", "payload": {"id": "k1", "cwd": "/work", "cli_version": "0.166.0"}}),
            json!({"type": "turn_context", "payload": {"model": "gpt-5"}}),
            // The same prompt as a response item and an event; it appears once.
            json!({"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "run it"}]}}),
            json!({"type": "event_msg", "payload": {"type": "user_message", "message": "run it"}}),
            json!({"type": "response_item", "payload": {"type": "reasoning", "summary": [{"type": "summary_text", "text": "THINKING_SENTINEL"}]}}),
            json!({"type": "response_item", "payload": {"type": "function_call", "name": "shell", "arguments": "{\"command\":\"curl ARG_SENTINEL\"}", "call_id": "c1"}}),
            json!({"type": "response_item", "payload": {"type": "function_call_output", "call_id": "c1", "output": "OUTPUT_SENTINEL"}}),
            json!({"type": "event_msg", "payload": {"type": "agent_message", "message": "done"}}),
            json!({"type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "done"}]}}),
        ]
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        let messages = parse(Engine::Codex, &text).unwrap();
        let pairs: Vec<(&str, &str)> = messages
            .iter()
            .map(|message| (message.role.as_str(), message.text.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("user", "run it"),
                ("tool", "Ran a command"),
                ("assistant", "done"),
            ]
        );
        let debug = format!("{messages:?}");
        assert!(!debug.contains("THINKING_SENTINEL"));
        assert!(!debug.contains("ARG_SENTINEL"));
        assert!(!debug.contains("OUTPUT_SENTINEL"));
    }

    #[test]
    fn an_unreadable_or_newer_session_is_refused() {
        assert!(parse(Engine::Claude, "not json").is_err());
        assert!(parse(Engine::Claude, "").is_err());
        assert!(parse(Engine::Codex, "{\"no\":\"conversation\"}\n").is_err());
        let newer_claude = json!({"type": "user", "version": "3.0.0",
            "message": {"content": "hello"}})
        .to_string();
        assert!(parse(Engine::Claude, &newer_claude).is_err());
        let newer_codex =
            json!({"type": "session_meta", "payload": {"id": "k1", "cli_version": "1.0.0"}})
                .to_string();
        assert!(parse(Engine::Codex, &newer_codex).is_err());
        let newer_shape = json!({"type": "user", "format_version": 2,
            "message": {"content": "hello"}})
        .to_string();
        assert!(parse(Engine::Claude, &newer_shape).is_err());
    }

    #[test]
    fn a_page_is_the_last_fifty_or_the_one_before_the_cursor_and_the_cursor_is_bound_to_the_file() {
        let dir = TempDir::new();
        let core = core(&dir);
        let project_id = project(&core, &dir);
        let cwd = dir.path().join("work").to_str().unwrap().to_owned();
        let root = dir.path().join("history");
        let file = root.join("projects/-work/s-pages.jsonl");
        let lines: Vec<Value> = (0..120)
            .map(|at| claude_user(&cwd, "s-pages", &format!("message {at}")))
            .collect();
        let bytes = write(&file, &lines);
        let thread = imported(&core, &project_id, &cwd, "claude", "s-pages");

        let first = core.earlier_messages_in(&thread, None, &root).unwrap();
        assert_eq!(first.total, 120);
        assert_eq!(
            first.imported_at,
            core.store.thread(&thread).unwrap().imported_at.unwrap()
        );
        assert_eq!(first.messages.len(), 50);
        assert_eq!(first.messages[0].text, "message 70");
        assert_eq!(first.messages[49].text, "message 119");
        let cursor = first.next_before.clone().unwrap();

        let second = core
            .earlier_messages_in(&thread, Some(&cursor), &root)
            .unwrap();
        assert_eq!(second.messages[0].text, "message 20");
        assert_eq!(second.messages[49].text, "message 69");
        let cursor = second.next_before.clone().unwrap();

        let third = core
            .earlier_messages_in(&thread, Some(&cursor), &root)
            .unwrap();
        assert_eq!(third.messages.len(), 20);
        assert_eq!(third.messages[0].text, "message 0");
        assert!(third.next_before.is_none());
        assert_eq!(std::fs::read(&file).unwrap(), bytes);

        let changed = core.earlier_messages_in(&thread, Some("not-a-cursor"), &root);
        assert_eq!(
            changed.unwrap_err().code(),
            "bad_cursor",
            "a malformed cursor is its own refusal"
        );

        let mut appended = bytes.clone();
        appended.extend_from_slice(b"\n");
        appended.extend_from_slice(
            claude_user(&cwd, "s-pages", "appended")
                .to_string()
                .as_bytes(),
        );
        std::fs::write(&file, &appended).unwrap();
        let changed = core.earlier_messages_in(&thread, Some(&cursor), &root);
        assert_eq!(changed.unwrap_err().code(), "history_unreadable");
    }

    #[test]
    fn missing_oversized_and_unreadable_files_are_history_unreadable() {
        let dir = TempDir::new();
        let core = core(&dir);
        let project_id = project(&core, &dir);
        let cwd = dir.path().join("work").to_str().unwrap().to_owned();
        let root = dir.path().join("history");
        let file = root.join("projects/-work/s-bad.jsonl");
        write(&file, &[claude_user(&cwd, "s-bad", "hello")]);
        let thread = imported(&core, &project_id, &cwd, "claude", "s-bad");

        std::fs::rename(&file, file.with_extension("moved")).unwrap();
        assert_eq!(
            core.earlier_messages_in(&thread, None, &root)
                .unwrap_err()
                .code(),
            "history_unreadable"
        );

        let oversized = file.with_file_name("s-big.jsonl");
        write(&oversized, &[claude_user(&cwd, "s-big", "hello")]);
        let big = imported(&core, &project_id, &cwd, "claude", "s-big");
        let handle = std::fs::OpenOptions::new()
            .write(true)
            .open(&oversized)
            .unwrap();
        handle.set_len(MAX_SESSION_BYTES + 1).unwrap();
        drop(handle);
        assert_eq!(
            core.earlier_messages_in(&big, None, &root)
                .unwrap_err()
                .code(),
            "history_unreadable"
        );

        let malformed = file.with_file_name("s-malformed.jsonl");
        write(
            &malformed,
            &[
                json!({"type": "user", "sessionId": "s-malformed", "cwd": cwd,
                "message": {"content": "hello"}}),
            ],
        );
        let bad = imported(&core, &project_id, &cwd, "claude", "s-malformed");
        std::fs::write(&malformed, "not json at all").unwrap();
        // The head no longer names the session, so the file cannot be resolved either.
        assert_eq!(
            core.earlier_messages_in(&bad, None, &root)
                .unwrap_err()
                .code(),
            "history_unreadable"
        );
    }

    #[test]
    fn a_thread_without_an_import_record_is_not_imported_and_a_missing_thread_is_not_found() {
        let dir = TempDir::new();
        let core = core(&dir);
        let project_id = project(&core, &dir);
        let cwd = dir.path().join("work").to_str().unwrap().to_owned();
        let root = dir.path().join("history");
        let thread = core
            .store
            .import_transcript_thread(
                NewThread {
                    project_id,
                    title: "From a file".into(),
                    engine: "claude".into(),
                    model: None,
                    cwd,
                    worktree: None,
                    skill_profile: None,
                    parent_thread_id: None,
                    relation: None,
                    scratch: false,
                },
                &[],
                0,
            )
            .unwrap();
        assert_eq!(
            core.earlier_messages_in(&thread.id, None, &root)
                .unwrap_err()
                .code(),
            "not_imported"
        );
        assert!(matches!(
            core.earlier_messages_in("missing", None, &root),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn a_read_writes_no_row_and_redacts_and_cuts_every_text() {
        let dir = TempDir::new();
        let core = core(&dir);
        let project_id = project(&core, &dir);
        let cwd = dir.path().join("work").to_str().unwrap().to_owned();
        let root = dir.path().join("history");
        let file = root.join("projects/-work/s-text.jsonl");
        let secret = "sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let long = "x".repeat(20_000);
        let bytes = write(
            &file,
            &[
                claude_user(&cwd, "s-text", &format!("key {secret}")),
                claude_user(&cwd, "s-text", &long),
            ],
        );
        let thread = imported(&core, &project_id, &cwd, "claude", "s-text");
        let counts = || {
            let conn = core.store.reader();
            let count = |table: &str| -> i64 {
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap()
            };
            (
                count("threads"),
                count("thread_events"),
                count("thread_messages"),
                count("audit_events"),
            )
        };
        let before = counts();
        let page = core.earlier_messages_in(&thread, None, &root).unwrap();
        assert_eq!(counts(), before);
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
        // The redactor may fold the words before a key into its label; what matters is that the
        // key is gone and its marker stays.
        assert!(
            page.messages[0].text.ends_with("[hidden: API key]"),
            "{}",
            page.messages[0].text
        );
        assert!(page.messages[1].text.ends_with('…'));
        assert!(page.messages[1].text.len() <= 16 * 1024);
        assert!(!format!("{page:?}").contains(secret));
    }
}
