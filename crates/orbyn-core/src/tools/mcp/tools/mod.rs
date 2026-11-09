//! Orbyn's own tools for a running agent, under Orbyn names. Tools reach only
//! the caller's project except the explicitly approved project add/remove surface.
//! Claude's permission prompt passes these tools without
//! asking, so each one with an effect goes through the gate here, as a `tool` action:
//! it needs a person unless the thread is in full access or a rule allows it.

use std::path::Path;

use orbyn_contracts::{
    ActionKind, CreateThread, DelegateTask, TaskSchedule, Thread, UpdateThread, UpsertTask,
};
use serde_json::{Value, json};

use crate::app::Core;
use crate::browser::preview::{
    DEFAULT_SCROLL, MAX_SCROLL, MAX_SELECTOR_CHARS, MAX_TYPE_CHARS, MAX_WAIT_TEXT_CHARS,
    PreviewAct, PreviewAction,
};
use crate::error::{CoreError, Result};
use crate::governance::approvals::{ActionAsk, Verdict};
use crate::store::AuditRecord;
use crate::tools::mcp::McpBinding;

/// Under the 45 seconds a tool has to answer, so the caller hears `ended: false`.
const MAX_WAIT_SECONDS: u64 = 40;
const MAX_SEARCH_CHARS: usize = 200_000;
const MAX_HITS: usize = 50;
const MAX_BATCH: usize = 16;
const MAX_PROJECT_PATH_BYTES: usize = 4096;
const MAX_PROJECT_ID_BYTES: usize = 128;
/// A published page's frame height. Nothing measures the page yet (C23), so a call
/// picks the height and these are the bounds it is held to.
const DEFAULT_RENDER_HEIGHT: u64 = 480;
const MIN_RENDER_HEIGHT: u64 = 120;
const MAX_RENDER_HEIGHT: u64 = 2000;
const MAX_RENDER_TITLE_CHARS: usize = 200;
/// A review guide's bounds. The stored stops stay well under the 256 KiB reply cap,
/// so reading a guide back always fits.
const MAX_GUIDE_STOPS: usize = 30;
const MAX_STOP_TITLE_CHARS: usize = 200;
const MAX_STOP_SUMMARY_CHARS: usize = 4000;
const MAX_STOP_FILES: usize = 50;
const MAX_GUIDE_PATH_BYTES: usize = 1024;
const MAX_FILE_NOTE_CHARS: usize = 1000;
const MAX_STOP_QUESTIONS: usize = 10;
const MAX_QUESTION_CHARS: usize = 500;
const MAX_GUIDE_BYTES: usize = 128 * 1024;

struct Tool {
    name: &'static str,
    description: &'static str,
    /// Has an effect, so it goes through the gate.
    effect: bool,
    /// `(name, type)` of each argument.
    args: &'static [(&'static str, &'static str)],
    required: &'static [&'static str],
}

const fn read(
    name: &'static str,
    description: &'static str,
    args: &'static [(&'static str, &'static str)],
    required: &'static [&'static str],
) -> Tool {
    Tool {
        name,
        description,
        effect: false,
        args,
        required,
    }
}

const fn act(
    name: &'static str,
    description: &'static str,
    args: &'static [(&'static str, &'static str)],
    required: &'static [&'static str],
) -> Tool {
    Tool {
        name,
        description,
        effect: true,
        args,
        required,
    }
}

const TOOLS: &[Tool] = &[
    act(
        "orbyn_project_create",
        "Add an existing absolute folder in place as an untrusted project, through the tool approval policy. Does not create, clone, move or trust a folder.",
        &[("path", "string")],
        &["path"],
    ),
    act(
        "orbyn_project_delete",
        "Remove an approved project's registration and idle thread metadata, keeping every file. Refuses own, library, scratch, busy or cross-project thread families.",
        &[("project_id", "string")],
        &["project_id"],
    ),
    read(
        "orbyn_project_list",
        "Projects visible to this caller: only its own project.",
        &[],
        &[],
    ),
    read(
        "orbyn_environment_read",
        "This service's version, platform and this thread's folder.",
        &[],
        &[],
    ),
    read(
        "orbyn_project_read",
        "This thread's project, with its project file.",
        &[],
        &[],
    ),
    read(
        "orbyn_worktree_list",
        "Worktrees of this project.",
        &[],
        &[],
    ),
    read("orbyn_thread_list", "Threads in this project.", &[], &[]),
    read(
        "orbyn_thread_read",
        "A thread and its recent text.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    read(
        "orbyn_thread_search",
        "Threads in this project matching every word.",
        &[("query", "string")],
        &["query"],
    ),
    read(
        "orbyn_thread_wait",
        "Wait for a thread's run to end, up to forty seconds; ended is false at the deadline.",
        &[("thread_id", "string"), ("seconds", "number")],
        &["thread_id"],
    ),
    read(
        "orbyn_queue_list",
        "A thread's queued messages.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    read(
        "orbyn_pending_requests",
        "Approvals and questions open in a thread.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    read(
        "orbyn_pull_list",
        "Pull requests linked to this thread, and those it watches.",
        &[],
        &[],
    ),
    read(
        "orbyn_pull_guide_read",
        "This project's review guide for a pull request; guide is null when none was written.",
        &[("number", "number")],
        &["number"],
    ),
    read(
        "orbyn_schedule_list",
        "Scheduled tasks in this project.",
        &[],
        &[],
    ),
    read(
        "orbyn_task_status",
        "A delegated thread's state.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    read(
        "orbyn_worktree_status",
        "This checkout's branch, whether it is a worktree, and its uncommitted files.",
        &[],
        &[],
    ),
    read(
        "orbyn_orchestrator_capabilities",
        "What this thread can start: engines, and whether it may delegate.",
        &[],
        &[],
    ),
    read(
        "orbyn_preview_list",
        "This thread's preview tabs.",
        &[],
        &[],
    ),
    read(
        "orbyn_preview_status",
        "One preview tab: where it is, its size, view and state.",
        &[("tab_id", "string")],
        &["tab_id"],
    ),
    read(
        "orbyn_preview_snapshot",
        "The visible controls and a sample of the text of a preview tab's page, \
         capped in size.",
        &[("tab_id", "string"), ("max_elements", "number")],
        &["tab_id"],
    ),
    read(
        "orbyn_preview_wait_for",
        "Wait for a selector or text to show up in a preview tab, up to twenty seconds.",
        &[
            ("tab_id", "string"),
            ("selector", "string"),
            ("text", "string"),
            ("timeout_ms", "number"),
        ],
        &["tab_id"],
    ),
    read(
        "orbyn_track_files",
        "Before writing files no tool reports, such as from a shell script, call this with \
         the checkout paths; call it again with done true afterwards.",
        &[("paths", "array"), ("done", "boolean")],
        &["paths"],
    ),
    act(
        "orbyn_thread_launch",
        "Start a new thread in this project with a first message.",
        &[
            ("prompt", "string"),
            ("engine", "string"),
            ("title", "string"),
        ],
        &["prompt"],
    ),
    act(
        "orbyn_threads_create",
        "Start several threads in this project, each with a first message. One failure \
         does not stop the rest.",
        &[("threads", "array")],
        &["threads"],
    ),
    act(
        "orbyn_worktree_handoff",
        "Continue in a new worktree: a new thread there, with a first message.",
        &[
            ("purpose", "string"),
            ("prompt", "string"),
            ("base_branch", "string"),
            ("title", "string"),
        ],
        &["purpose", "prompt"],
    ),
    act(
        "orbyn_environment_preferences_update",
        "Change this project's preferences. null goes back to the default.",
        &[("values", "object")],
        &["values"],
    ),
    act(
        "orbyn_project_update",
        "Rename this project.",
        &[("name", "string")],
        &["name"],
    ),
    act(
        "orbyn_thread_send",
        "Queue a message on a thread, with attachments from orbyn_attachment_add.",
        &[
            ("thread_id", "string"),
            ("prompt", "string"),
            ("attachment_ids", "array"),
        ],
        &["thread_id", "prompt"],
    ),
    act(
        "orbyn_attachment_add",
        "Attach a file from this checkout to a thread, for orbyn_thread_send.",
        &[("thread_id", "string"), ("path", "string")],
        &["thread_id", "path"],
    ),
    act(
        "orbyn_attachment_discard",
        "Remove an attachment that was not sent.",
        &[("thread_id", "string"), ("attachment_id", "string")],
        &["thread_id", "attachment_id"],
    ),
    act(
        "orbyn_thread_organize",
        "Pin, settle, snooze, archive or mark a thread unread.",
        &[
            ("thread_id", "string"),
            ("pinned", "boolean"),
            ("settled", "boolean"),
            ("archived", "boolean"),
            ("unread", "boolean"),
            ("snoozed_until", "string"),
        ],
        &["thread_id"],
    ),
    act(
        "orbyn_thread_configure",
        "Set a thread's model, effort, permission mode or plan mode.",
        &[
            ("thread_id", "string"),
            ("model", "string"),
            ("effort", "string"),
            ("permission_mode", "string"),
            ("plan_mode", "boolean"),
        ],
        &["thread_id"],
    ),
    act(
        "orbyn_preview_open",
        "Open a web page in a preview tab for this thread.",
        &[("url", "string"), ("width", "number"), ("height", "number")],
        &["url"],
    ),
    act(
        "orbyn_preview_navigate",
        "Go to a page, or back or forward, in one of this thread's preview tabs.",
        &[("tab_id", "string"), ("url", "string"), ("back", "boolean")],
        &["tab_id"],
    ),
    act(
        "orbyn_preview_resize",
        "Set a preview tab's size.",
        &[
            ("tab_id", "string"),
            ("width", "number"),
            ("height", "number"),
        ],
        &["tab_id", "width", "height"],
    ),
    act(
        "orbyn_preview_set_appearance",
        "Set a preview tab's color scheme: system, light or dark.",
        &[("tab_id", "string"), ("appearance", "string")],
        &["tab_id", "appearance"],
    ),
    act(
        "orbyn_preview_act",
        "Click, type into or scroll a preview tab's page.",
        &[
            ("tab_id", "string"),
            ("action", "string"),
            ("selector", "string"),
            ("text", "string"),
            ("dy", "number"),
        ],
        &["tab_id", "action"],
    ),
    act(
        "orbyn_preview_evaluate",
        "Run an expression in a preview tab's page and return its value, or its size \
         when that is over 20000 bytes.",
        &[("tab_id", "string"), ("expression", "string")],
        &["tab_id", "expression"],
    ),
    act(
        "orbyn_preview_dialog",
        "Answer the alert, confirm or prompt a page opened. `handled` is false if none was.",
        &[
            ("tab_id", "string"),
            ("accept", "boolean"),
            ("text", "string"),
        ],
        &["tab_id", "accept"],
    ),
    act(
        "orbyn_preview_close",
        "Close one of this thread's preview tabs.",
        &[("tab_id", "string")],
        &["tab_id"],
    ),
    act(
        "orbyn_preview_recording_start",
        "Start recording a preview tab's frames.",
        &[("tab_id", "string")],
        &["tab_id"],
    ),
    act(
        "orbyn_preview_recording_stop",
        "Stop recording, and return where the frames are.",
        &[("tab_id", "string")],
        &["tab_id"],
    ),
    act(
        "orbyn_html_render",
        "Publish an HTML page in this thread's timeline, as a card with a frame. The page \
         runs sandboxed: no network, no API and no storage of its own. `html` is the whole \
         page. `height` is the frame's height in CSS pixels; nothing measures the page yet, \
         so a page that needs more room needs a taller height.",
        &[
            ("html", "string"),
            ("title", "string"),
            ("height", "number"),
        ],
        &["html"],
    ),
    act(
        "orbyn_thread_update",
        "Rename, pin or settle a thread.",
        &[
            ("thread_id", "string"),
            ("title", "string"),
            ("pinned", "boolean"),
            ("settled", "boolean"),
        ],
        &["thread_id"],
    ),
    act(
        "orbyn_thread_interrupt",
        "Stop a thread's running turn.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    act(
        "orbyn_pull_link",
        "Link a pull request to this thread.",
        &[("number", "number")],
        &["number"],
    ),
    act(
        "orbyn_pull_unlink",
        "Unlink a pull request from this thread.",
        &[("number", "number")],
        &["number"],
    ),
    act(
        "orbyn_pull_watch",
        "Wake this thread when a pull request changes.",
        &[("number", "number")],
        &["number"],
    ),
    act(
        "orbyn_pull_unwatch",
        "Stop watching a pull request.",
        &[("number", "number")],
        &["number"],
    ),
    act(
        "orbyn_pull_guide_write",
        "Write this project's review guide for a pull request, replacing any earlier one: \
         up to 30 stops in reading order, each {title, summary, files: [{path, note}], \
         questions: [text]}. head_sha names the commit it describes. Fails while review \
         guides are off for this project.",
        &[
            ("number", "number"),
            ("head_sha", "string"),
            ("stops", "array"),
        ],
        &["number", "stops"],
    ),
    act(
        "orbyn_delegate_task",
        "Hand work to a child thread, which reports back when it ends.",
        &[
            ("prompt", "string"),
            ("title", "string"),
            ("engine", "string"),
        ],
        &["prompt"],
    ),
    act(
        "orbyn_task_cancel",
        "Stop a delegated thread's run.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    act(
        "orbyn_workflow_stop",
        "Request a stop for this caller's delegated thread and its running delegated children. An idle thread is unchanged.",
        &[("thread_id", "string")],
        &["thread_id"],
    ),
    act(
        "orbyn_schedule_task",
        "Run a prompt in this project every so many minutes.",
        &[
            ("name", "string"),
            ("prompt", "string"),
            ("every_minutes", "number"),
        ],
        &["name", "prompt", "every_minutes"],
    ),
    act(
        "orbyn_schedule_delete",
        "Delete a scheduled task in this project.",
        &[("task_id", "string")],
        &["task_id"],
    ),
    act(
        "orbyn_schedule_run",
        "Run a scheduled task in this project now.",
        &[("task_id", "string")],
        &["task_id"],
    ),
];

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

fn bounded_project_text<'a>(args: &'a Value, key: &str, max_bytes: usize) -> Result<&'a str> {
    let value = text(args, key)?;
    if value.len() > max_bytes || value.contains('\0') {
        return Err(CoreError::Invalid(format!(
            "{key} is at most {max_bytes} bytes and cannot contain NUL."
        )));
    }
    Ok(value)
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

/// An optional commit id: 7 to 64 hexadecimal characters, lowercased.
fn guide_head(args: &Value) -> Result<Option<String>> {
    match args.get("head_sha") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(sha))
            if (7..=64).contains(&sha.trim().len())
                && sha.trim().chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            Ok(Some(sha.trim().to_ascii_lowercase()))
        }
        Some(_) => Err(CoreError::Invalid(
            "head_sha is a commit id of 7 to 64 hexadecimal characters.".into(),
        )),
    }
}

fn guide_text(
    value: Option<&Value>,
    what: &str,
    max_chars: usize,
    required: bool,
) -> Result<String> {
    let text = match value {
        None | Some(Value::Null) if !required => "",
        Some(Value::String(text)) => text.trim(),
        _ => return Err(CoreError::Invalid(format!("{what} must be text."))),
    };
    if required && text.is_empty() {
        return Err(CoreError::Invalid(format!("{what} is required.")));
    }
    if text.chars().count() > max_chars || text.contains('\0') {
        return Err(CoreError::Invalid(format!(
            "{what} is at most {max_chars} characters and cannot contain NUL."
        )));
    }
    Ok(text.to_owned())
}

fn guide_list<'a>(value: Option<&'a Value>, what: &str, max: usize) -> Result<&'a [Value]> {
    match value {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(items)) if items.len() <= max => Ok(items),
        Some(Value::Array(_)) => Err(CoreError::Invalid(format!("{what} holds at most {max}."))),
        Some(_) => Err(CoreError::Invalid(format!("{what} must be a list."))),
    }
}

/// `orbyn_pull_guide_write`'s stops, checked and rebuilt with only the known
/// fields, so what is stored is exactly what the guide shape allows. A file path
/// is display text for the reader; nothing opens it.
/// Answers the stored JSON and the number of stops.
fn guide_stops(args: &Value) -> Result<(String, usize)> {
    let stops = match args.get("stops") {
        Some(Value::Array(stops)) if !stops.is_empty() => stops,
        _ => {
            return Err(CoreError::Invalid(
                "stops must list at least one stop.".into(),
            ));
        }
    };
    if stops.len() > MAX_GUIDE_STOPS {
        return Err(CoreError::Invalid(format!(
            "A guide holds at most {MAX_GUIDE_STOPS} stops."
        )));
    }
    let mut built = Vec::with_capacity(stops.len());
    for stop in stops {
        if !stop.is_object() {
            return Err(CoreError::Invalid("Each stop must be an object.".into()));
        }
        let mut files = Vec::new();
        for file in guide_list(stop.get("files"), "files", MAX_STOP_FILES)? {
            let path = guide_text(
                file.get("path"),
                "A file's path",
                MAX_GUIDE_PATH_BYTES,
                true,
            )?;
            if path.len() > MAX_GUIDE_PATH_BYTES {
                return Err(CoreError::Invalid(format!(
                    "A file's path is at most {MAX_GUIDE_PATH_BYTES} bytes."
                )));
            }
            let note = guide_text(
                file.get("note"),
                "A file's note",
                MAX_FILE_NOTE_CHARS,
                false,
            )?;
            files.push(json!({ "path": path, "note": note }));
        }
        let mut questions = Vec::new();
        for question in guide_list(stop.get("questions"), "questions", MAX_STOP_QUESTIONS)? {
            questions.push(guide_text(
                Some(question),
                "A question",
                MAX_QUESTION_CHARS,
                true,
            )?);
        }
        built.push(json!({
            "title": guide_text(stop.get("title"), "A stop's title", MAX_STOP_TITLE_CHARS, true)?,
            "summary": guide_text(stop.get("summary"), "A stop's summary", MAX_STOP_SUMMARY_CHARS, false)?,
            "files": files,
            "questions": questions,
        }));
    }
    let count = built.len();
    let stored = Value::Array(built).to_string();
    if stored.len() > MAX_GUIDE_BYTES {
        return Err(CoreError::Invalid(format!(
            "A guide is at most {MAX_GUIDE_BYTES} bytes."
        )));
    }
    Ok((stored, count))
}

/// `orbyn_preview_act`'s arguments, checked before the page is touched: one known
/// action, a selector where it needs one, and text only where typing uses it.
fn page_act(args: &Value) -> Result<PreviewAct> {
    let asked = text(args, "action")?;
    let action = match asked {
        "click" => PreviewAction::Click,
        "type" => PreviewAction::Type,
        "scroll" => PreviewAction::Scroll,
        other => {
            return Err(CoreError::Invalid(format!(
                "{other} is not an action. Use click, type or scroll."
            )));
        }
    };
    let selector = args
        .get("selector")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let typed = args
        .get("text")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if action != PreviewAction::Scroll && selector.is_none() {
        return Err(CoreError::Invalid(
            "Name the selector of the element to act on.".into(),
        ));
    }
    if action == PreviewAction::Type {
        let Some(typed) = typed.as_ref() else {
            return Err(CoreError::Invalid("Say what text to type.".into()));
        };
        if typed.chars().count() > MAX_TYPE_CHARS {
            return Err(CoreError::Invalid(format!(
                "Text to type is at most {MAX_TYPE_CHARS} characters."
            )));
        }
    }
    let dy = args
        .get("dy")
        .and_then(Value::as_f64)
        .filter(|dy| dy.is_finite())
        .unwrap_or(DEFAULT_SCROLL)
        .clamp(-MAX_SCROLL, MAX_SCROLL);
    Ok(PreviewAct {
        action,
        selector,
        text: typed,
        dy,
    })
}

/// `orbyn_preview_wait_for` waits on exactly one of a selector or a text, and the
/// text it looks for is short enough to check on every poll.
fn wait_target(args: &Value) -> Result<(Option<String>, Option<String>)> {
    let field = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
    };
    match (field("selector"), field("text")) {
        (Some(selector), None) => {
            if selector.chars().count() > MAX_SELECTOR_CHARS {
                return Err(CoreError::Invalid(format!(
                    "A selector is at most {MAX_SELECTOR_CHARS} characters."
                )));
            }
            Ok((Some(selector), None))
        }
        (None, Some(wanted)) => {
            if wanted.chars().count() > MAX_WAIT_TEXT_CHARS {
                return Err(CoreError::Invalid(format!(
                    "Text to wait for is at most {MAX_WAIT_TEXT_CHARS} characters."
                )));
            }
            Ok((None, Some(wanted)))
        }
        (Some(_), Some(_)) => Err(CoreError::Invalid(
            "Name a selector or text to wait for, not both.".into(),
        )),
        (None, None) => Err(CoreError::Invalid(
            "Name a selector or text to wait for.".into(),
        )),
    }
}

/// All of the query's words, in any order, ignoring case.
pub(crate) fn matches_words(haystack: &str, query: &str) -> bool {
    let haystack = haystack.to_lowercase();
    let mut words = query.split_whitespace().peekable();
    words.peek().is_some() && words.all(|word| haystack.contains(&word.to_lowercase()))
}

/// The frame height a published page is shown at. A caller's number stands, held to a
/// size a card can show; without one the page gets the default. Nothing measures the
/// page yet (C23), so this is the whole of what decides it.
fn render_height(args: &Value) -> u64 {
    args.get("height")
        .and_then(Value::as_f64)
        .map_or(DEFAULT_RENDER_HEIGHT, |height| {
            let bounded = height.clamp(MIN_RENDER_HEIGHT as f64, MAX_RENDER_HEIGHT as f64);
            // The bound is not negative, so the cast is exact after rounding.
            bounded.round() as u64
        })
}

/// The card's title: trimmed, cut to a length a row can show, and `HTML` when the
/// call named none.
fn render_title(args: &Value) -> String {
    args.get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(|title| title.chars().take(MAX_RENDER_TITLE_CHARS).collect())
        .unwrap_or_else(|| "HTML".to_owned())
}

impl Core {
    /// Only project add/remove may reach another project. Resolve exactly what
    /// the approval describes; callers cannot supply the internal preview fields.
    async fn project_tool_preview(
        &self,
        binding: &McpBinding,
        name: &str,
        args: &Value,
    ) -> Result<Option<Value>> {
        if binding.scope.plan && matches!(name, "orbyn_project_create" | "orbyn_project_delete") {
            return Err(CoreError::Invalid(
                "Plan mode does not add or remove projects.".into(),
            ));
        }
        match name {
            "orbyn_project_create" => {
                let path = bounded_project_text(args, "path", MAX_PROJECT_PATH_BYTES)?;
                if !Path::new(path).is_absolute() {
                    return Err(CoreError::Invalid(
                        "path must be an absolute existing folder.".into(),
                    ));
                }
                let root = crate::project::workspace::resolve_project(
                    &self.workspace.root()?,
                    Path::new(path),
                )
                .await?;
                let path = root.path.to_string_lossy().into_owned();
                bounded_project_text(&json!({ "path": path }), "path", MAX_PROJECT_PATH_BYTES)?;
                Ok(Some(
                    json!({ "path": path, "trusted": false, "files_created": false }),
                ))
            }
            "orbyn_project_delete" => {
                let id = bounded_project_text(args, "project_id", MAX_PROJECT_ID_BYTES)?;
                let project = self.store.project(id)?;
                bounded_project_text(
                    &json!({ "path": project.path }),
                    "path",
                    MAX_PROJECT_PATH_BYTES,
                )?;
                bounded_project_text(&json!({ "name": project.name }), "name", 4096)?;
                let own = self.store.thread(&binding.scope.thread_id)?;
                if project.id == own.project_id
                    || project.scratch
                    || Path::new(&project.path) == self.workspace.root()?
                {
                    return Err(CoreError::Invalid("The caller's own, library and scratch projects cannot be removed by a tool.".into()));
                }
                let threads = self.store.agent_project_threads(id)?;
                Ok(Some(json!({
                    "project_id": id, "path": project.path, "name": project.name,
                    "thread_ids": threads, "files_removed": false,
                })))
            }
            _ => Ok(None),
        }
    }

    /// Runs one of Orbyn's tools for `binding`'s run. A tool with an effect is
    /// refused unless the gate allows it, and the gate's reason comes back.
    pub async fn call_orbyn_tool(
        &self,
        binding: &McpBinding,
        name: &str,
        args: &Value,
    ) -> Result<Value> {
        let tool = TOOLS
            .iter()
            .find(|tool| tool.name == name)
            .ok_or_else(|| CoreError::NotFound(format!("tool {name}")))?;
        let preview = self.project_tool_preview(binding, name, args).await?;
        let args = preview.as_ref().unwrap_or(args);
        if tool.effect {
            let summary = match name {
                "orbyn_project_create" => format!(
                    "Add existing folder {} to Orbyn as untrusted; keep all files",
                    text(args, "path")?
                ),
                "orbyn_project_delete" => format!(
                    "Remove project {} at {} from Orbyn; keep all files",
                    text(args, "project_id")?,
                    text(args, "path")?
                ),
                _ => format!("Use {name}"),
            };
            let ask = ActionAsk {
                kind: ActionKind::Tool,
                target: name.to_owned(),
                summary,
                detail: args.clone(),
                paths: Vec::new(),
            };
            if let Verdict::Deny(reason) = self.gate.request(&binding.scope, ask).await {
                return Err(CoreError::PolicyDenied(reason));
            }
        }
        self.run_orbyn_tool(binding, name, args).await
    }

    /// A thread in the caller's project. Anything else reads as not found, so a
    /// tool cannot be used to learn what other projects hold.
    fn project_thread(&self, binding: &McpBinding, id: &str) -> Result<Thread> {
        let own = self.store.thread(&binding.scope.thread_id)?;
        let thread = self.store.thread(id)?;
        if thread.project_id != own.project_id {
            return Err(CoreError::NotFound("thread".into()));
        }
        Ok(thread)
    }

    fn project_task(&self, binding: &McpBinding, id: &str) -> Result<()> {
        let own = self.store.thread(&binding.scope.thread_id)?;
        if self.store.task(id)?.project_id != own.project_id {
            return Err(CoreError::NotFound("scheduled task".into()));
        }
        Ok(())
    }

    /// A new thread in the caller's project with a first message.
    async fn launch(&self, own: &Thread, spec: &Value, worktree: Option<String>) -> Result<Value> {
        let string = |key: &str| spec.get(key).and_then(Value::as_str).map(str::to_owned);
        let thread = self
            .create_thread(CreateThread {
                project_id: Some(own.project_id.clone()),
                engine: string("engine").unwrap_or_else(|| own.engine.clone()),
                model: None,
                title: string("title"),
                worktree,
                skill_profile: own.skill_profile.clone(),
                base_branch: string("base_branch"),
                effort: None,
                options: std::collections::BTreeMap::new(),
            })
            .await?;
        self.runs.enqueue(&thread.id, text(spec, "prompt")?, &[])?;
        Ok(json!({ "thread_id": thread.id }))
    }

    /// A checkout file as an attachment. Trust, `..`, links and `.git` are the same
    /// checks as any file route, and a thread can only draw on its own checkout.
    fn attach_from_checkout(
        &self,
        own: &Thread,
        target: &str,
        path: &str,
    ) -> Result<orbyn_contracts::Attachment> {
        let bytes = self.open_raw_file(&own.id, path, true)?;
        let mut body = Vec::new();
        std::io::Read::read_to_end(&mut &bytes.file, &mut body)?;
        self.add_attachment(target, &bytes.name, "application/octet-stream", &body)
    }

    fn own_preview(&self, own: &Thread, args: &Value) -> Result<String> {
        let tab = text(args, "tab_id")?.to_owned();
        if !self
            .previews
            .list(&own.id)
            .iter()
            .any(|view| view.id == tab)
        {
            return Err(CoreError::NotFound("preview".into()));
        }
        Ok(tab)
    }

    #[allow(clippy::too_many_lines)]
    async fn run_orbyn_tool(
        &self,
        binding: &McpBinding,
        name: &str,
        args: &Value,
    ) -> Result<Value> {
        let actor = format!("engine:{}", binding.scope.engine);
        let own = self.store.thread(&binding.scope.thread_id)?;
        let thread_arg = || self.project_thread(binding, text(args, "thread_id")?);
        if name.starts_with("orbyn_preview")
            && self.preference("enable_agent_browser_access", Some(&own.project_id))?
                == json!(false)
        {
            return Err(CoreError::Invalid(
                "Agents can't use the browser here. It's turned off in Settings.".into(),
            ));
        }
        Ok(match name {
            "orbyn_environment_read" => json!({
                "version": env!("CARGO_PKG_VERSION"),
                "platform": std::env::consts::OS,
                "thread_id": own.id,
                "project_id": own.project_id,
                "checkout": own.cwd,
                "engine": own.engine,
            }),
            "orbyn_project_read" => json!({
                "project": self.store.project(&own.project_id)?,
                "file": self.project_file(&own.id)?,
            }),
            "orbyn_project_list" => json!([self.store.project(&own.project_id)?]),
            "orbyn_project_create" => {
                let approved = text(args, "path")?;
                let root = crate::project::workspace::resolve_project(
                    &self.workspace.root()?,
                    Path::new(approved),
                )
                .await?;
                if root.path != Path::new(approved) {
                    return Err(CoreError::Invalid(
                        "The approved folder changed; ask again.".into(),
                    ));
                }
                let name = root.path.file_name().map_or_else(
                    || "Project".to_owned(),
                    |name| name.to_string_lossy().into_owned(),
                );
                json!(self.store.insert_agent_project(
                    approved,
                    &name,
                    root.is_git,
                    &binding.scope
                )?)
            }
            "orbyn_project_delete" => {
                let id = text(args, "project_id")?;
                let threads: Vec<String> = serde_json::from_value(args["thread_ids"].clone())?;
                let removed = self.store.remove_agent_project(
                    id,
                    text(args, "path")?,
                    &threads,
                    &self.workspace.root()?.to_string_lossy(),
                    &binding.scope,
                )?;
                for thread in removed {
                    self.setups.forget(&thread);
                    self.terminals.close(&thread);
                }
                json!({ "project_id": id, "removed": true, "files_removed": false, "kept_path": args["path"] })
            }
            "orbyn_worktree_list" => {
                let project = self.store.project(&own.project_id)?;
                let found =
                    crate::source_control::git::linked_worktrees(Path::new(&project.path)).await;
                let listed: Vec<Value> = found
                    .unwrap_or_default()
                    .into_iter()
                    .map(|worktree| json!({ "path": worktree.path, "branch": worktree.branch }))
                    .collect();
                json!(listed)
            }
            "orbyn_thread_list" => json!(self.store.list_threads(Some(&own.project_id), false)?),
            "orbyn_thread_read" => {
                let thread = thread_arg()?;
                let text = self.store.recovery_handoff(&thread.id)?;
                json!({ "thread": thread, "text": text })
            }
            "orbyn_thread_search" => {
                let query = text(args, "query")?;
                let mut hits = Vec::new();
                for thread in self.store.list_threads(Some(&own.project_id), false)? {
                    let body = self.store.transcript(&thread.id, MAX_SEARCH_CHARS)?;
                    if matches_words(&format!("{} {body}", thread.title), query) {
                        hits.push(json!({ "id": thread.id, "title": thread.title }));
                        if hits.len() == MAX_HITS {
                            break;
                        }
                    }
                }
                json!(hits)
            }
            "orbyn_thread_wait" => {
                let thread = thread_arg()?;
                if thread.id == own.id {
                    return Err(CoreError::Invalid(
                        "A thread cannot wait for itself.".into(),
                    ));
                }
                let seconds = wait_seconds(args)?;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
                loop {
                    let thread = self.store.thread(&thread.id)?;
                    let ended = thread.active_run.is_none();
                    if ended || std::time::Instant::now() >= deadline {
                        break json!({
                            "ended": ended,
                            "last_run_state": thread.last_run_state,
                        });
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
            "orbyn_queue_list" => json!(self.store.list_queue(&thread_arg()?.id)?),
            "orbyn_pending_requests" => {
                let thread = thread_arg()?;
                json!({
                    "approvals": self.store.pending_for_thread(&thread.id)?,
                    "questions": self.store.list_questions(
                        Some(&thread.id),
                        Some(orbyn_contracts::QuestionState::Pending),
                    )?,
                })
            }
            "orbyn_pull_list" => json!({
                "linked": self.store.linked_pulls(&own.id)?,
                "watched": self.thread_watches(&own.id)?,
            }),
            "orbyn_schedule_list" => {
                let tasks = self.store.tasks()?;
                let tasks: Vec<_> = tasks
                    .into_iter()
                    .filter(|task| task.project_id == own.project_id)
                    .collect();
                json!(tasks)
            }
            "orbyn_task_status" => {
                let thread = thread_arg()?;
                json!({
                    "active": thread.active_run.is_some(),
                    "last_run_state": thread.last_run_state,
                })
            }
            "orbyn_track_files" => {
                let paths: Vec<String> = args
                    .get("paths")
                    .and_then(Value::as_array)
                    .ok_or_else(|| CoreError::Invalid("paths is required.".into()))?
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_owned))
                    .take(200)
                    .collect();
                let done = args.get("done").and_then(Value::as_bool).unwrap_or(false);
                crate::conversation::file_changes::record(
                    &self.store,
                    &binding.scope,
                    &paths,
                    done,
                );
                json!({ "tracked": paths.len() })
            }
            "orbyn_thread_launch" => self.launch(&own, args, None).await?,
            "orbyn_threads_create" => {
                let specs = args
                    .get("threads")
                    .and_then(Value::as_array)
                    .filter(|specs| specs.len() <= MAX_BATCH)
                    .ok_or_else(|| {
                        CoreError::Invalid(format!("threads is a list of up to {MAX_BATCH}."))
                    })?;
                let mut results = Vec::with_capacity(specs.len());
                for spec in specs {
                    results.push(match self.launch(&own, spec, None).await {
                        Ok(started) => started,
                        Err(error) => json!({ "error": error.public_message() }),
                    });
                }
                json!(results)
            }
            "orbyn_worktree_handoff" => {
                let purpose = text(args, "purpose")?.to_owned();
                self.launch(&own, args, Some(purpose)).await?
            }
            "orbyn_worktree_status" => {
                let checkout = Path::new(&own.cwd);
                let status =
                    crate::source_control::git::git(checkout, &["status", "--porcelain", "-b"])
                        .await?;
                let status = String::from_utf8_lossy(&status);
                let mut lines = status.lines();
                let branch = lines.next().and_then(|line| line.strip_prefix("## "));
                let project = self.store.project(&own.project_id)?;
                json!({
                    "checkout": own.cwd,
                    "branch": branch,
                    "worktree": Path::new(&project.path) != checkout,
                    "changed": lines.map(|line| line.get(3..).unwrap_or(line)).collect::<Vec<_>>(),
                })
            }
            "orbyn_orchestrator_capabilities" => {
                let nested =
                    own.lifecycle.as_ref().and_then(|l| l.relation.as_deref()) == Some("subagent");
                let engines: Vec<Value> = self
                    .engines
                    .statuses(false)
                    .await?
                    .into_iter()
                    .map(|engine| json!({ "id": engine.id, "name": engine.name }))
                    .collect();
                json!({
                    "engines": engines,
                    "can_delegate": !nested,
                    "max_batch": MAX_BATCH,
                })
            }
            "orbyn_environment_preferences_update" => {
                let values = args
                    .get("values")
                    .and_then(Value::as_object)
                    .ok_or_else(|| CoreError::Invalid("values is required.".into()))?
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                self.set_preferences(Some(&own.project_id), &values)?;
                json!({ "updated": values.len() })
            }
            "orbyn_project_update" => {
                json!(self.rename_project(&own.project_id, text(args, "name")?)?)
            }
            "orbyn_thread_send" => {
                let thread = thread_arg()?;
                let attachments: Vec<String> = args
                    .get("attachment_ids")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|id| id.as_str().map(str::to_owned))
                    .collect();
                json!(
                    self.runs
                        .enqueue(&thread.id, text(args, "prompt")?, &attachments)?
                )
            }
            "orbyn_attachment_add" => {
                let thread = thread_arg()?;
                json!(self.attach_from_checkout(&own, &thread.id, text(args, "path")?)?)
            }
            "orbyn_attachment_discard" => {
                let thread = thread_arg()?;
                let id = text(args, "attachment_id")?;
                let held = self.store.list_attachments(&thread.id)?;
                if !held.iter().any(|(attachment, _)| attachment.id == id) {
                    return Err(CoreError::NotFound("attachment".into()));
                }
                self.remove_attachment(id)?;
                json!({ "discarded": true })
            }
            "orbyn_thread_organize" => {
                let thread = thread_arg()?;
                let flag = |key: &str| args.get(key).and_then(Value::as_bool);
                let patch = UpdateThread {
                    pinned: flag("pinned"),
                    settled: flag("settled"),
                    archived: flag("archived"),
                    unread: flag("unread"),
                    snoozed_until: args
                        .get("snoozed_until")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    ..UpdateThread::default()
                };
                json!(self.update_thread(&thread.id, &patch, &actor)?)
            }
            "orbyn_thread_configure" => {
                let thread = thread_arg()?;
                let string = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_owned);
                let mode = args
                    .get("permission_mode")
                    .filter(|value| !value.is_null())
                    .map(|value| serde_json::from_value(value.clone()))
                    .transpose()
                    .map_err(|_| CoreError::Invalid("That is not a permission mode.".into()))?;
                let patch = UpdateThread {
                    model: string("model"),
                    effort: string("effort"),
                    permission_mode: mode,
                    plan_mode: args.get("plan_mode").and_then(Value::as_bool),
                    ..UpdateThread::default()
                };
                json!(self.update_thread(&thread.id, &patch, &actor)?)
            }
            "orbyn_preview_list" => json!(self.previews.list(&own.id)),
            "orbyn_preview_open" => {
                let size = |key: &str| {
                    args.get(key)
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                };
                let url = text(args, "url")?;
                let opened = self.open_preview(
                    &own.id,
                    url,
                    (size("width"), size("height")),
                    &actor,
                    crate::browser::preview::OpenHow::default(),
                );
                json!(opened.await?)
            }
            "orbyn_preview_navigate" => {
                let tab = self.own_preview(&own, args)?;
                if let Some(url) = args.get("url").and_then(Value::as_str) {
                    json!(self.previews.navigate(&own.id, &tab, url).await?)
                } else {
                    let back = args.get("back").and_then(Value::as_bool);
                    let back = back.ok_or_else(|| {
                        CoreError::Invalid("Name a url, or back true or false.".into())
                    })?;
                    json!(self.previews.step(&own.id, &tab, back).await?)
                }
            }
            "orbyn_preview_resize" => {
                let tab = self.own_preview(&own, args)?;
                let side = |key: &str| {
                    u32::try_from(number(args, key)?)
                        .map_err(|_| CoreError::Invalid(format!("{key} is too large.")))
                };
                let (width, height) = (side("width")?, side("height")?);
                json!(self.previews.resize(&own.id, &tab, width, height).await?)
            }
            "orbyn_preview_status" => {
                let tab = self.own_preview(&own, args)?;
                let tabs = self.previews.list(&own.id);
                json!(tabs.into_iter().find(|view| view.id == tab))
            }
            "orbyn_preview_snapshot" => {
                let tab = self.own_preview(&own, args)?;
                let max_elements = args.get("max_elements").and_then(Value::as_u64);
                json!(self.previews.snapshot(&own.id, &tab, max_elements).await?)
            }
            "orbyn_preview_wait_for" => {
                let tab = self.own_preview(&own, args)?;
                let (selector, wanted) = wait_target(args)?;
                let timeout = args.get("timeout_ms").and_then(Value::as_u64);
                json!(
                    self.previews
                        .wait_for(
                            &own.id,
                            &tab,
                            selector.as_deref(),
                            wanted.as_deref(),
                            timeout
                        )
                        .await?
                )
            }
            "orbyn_preview_act" => {
                let tab = self.own_preview(&own, args)?;
                let step = page_act(args)?;
                json!(self.previews.act(&own.id, &tab, &step).await?)
            }
            "orbyn_preview_evaluate" => {
                let tab = self.own_preview(&own, args)?;
                let expression = text(args, "expression")?;
                json!(self.previews.evaluate(&own.id, &tab, expression).await?)
            }
            "orbyn_preview_set_appearance" => {
                let tab = self.own_preview(&own, args)?;
                let view = orbyn_contracts::PreviewView {
                    zoom: None,
                    appearance: Some(text(args, "appearance")?.to_owned()),
                    mobile: None,
                };
                json!(self.previews.set_view(&own.id, &tab, &view).await?)
            }
            "orbyn_preview_dialog" => {
                let tab = self.own_preview(&own, args)?;
                let accept = args
                    .get("accept")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| CoreError::Invalid("Say whether to accept.".into()))?;
                let answer = args.get("text").and_then(Value::as_str);
                let handled = self.previews.answer_dialog(&own.id, &tab, accept, answer);
                json!({ "handled": handled.await? })
            }
            "orbyn_preview_close" => {
                let tab = self.own_preview(&own, args)?;
                self.previews.close(&own.id, &tab)?;
                json!({ "closed": true })
            }
            "orbyn_preview_recording_start" => {
                let tab = self.own_preview(&own, args)?;
                self.start_preview_recording(&own.id, &tab, Default::default())?;
                json!({ "recording": true })
            }
            "orbyn_preview_recording_stop" => {
                let tab = self.own_preview(&own, args)?;
                json!(self.previews.stop_recording(&own.id, &tab).await?)
            }
            "orbyn_html_render" => {
                // The caller's own thread, so a page lands in the timeline that asked
                // for it. The route stays a path: the client adds `?render=<token>`.
                let render = self.publish_render(&own.id, text(args, "html")?)?;
                json!({
                    "id": render.id,
                    "url": render.url,
                    "frame_token": render.frame_token,
                    "title": render_title(args),
                    "images": render.images,
                    "bytes": render.bytes,
                    "height": render_height(args),
                })
            }
            "orbyn_thread_update" => {
                let thread = thread_arg()?;
                let patch = UpdateThread {
                    title: args.get("title").and_then(Value::as_str).map(str::to_owned),
                    pinned: args.get("pinned").and_then(Value::as_bool),
                    settled: args.get("settled").and_then(Value::as_bool),
                    ..UpdateThread::default()
                };
                json!(self.update_thread(&thread.id, &patch, &actor)?)
            }
            "orbyn_workflow_stop" => {
                let id = text(args, "thread_id")?;
                if id.len() > 128 {
                    return Err(CoreError::Invalid("thread_id is at most 128 bytes.".into()));
                }
                let thread = self.project_thread(binding, id)?;
                if !thread.lifecycle.as_ref().is_some_and(|lifecycle| {
                    lifecycle.relation.as_deref() == Some("subagent")
                        && lifecycle.parent_thread_id.as_deref() == Some(own.id.as_str())
                }) {
                    return Err(CoreError::NotFound("delegated thread".into()));
                }
                let stopped = if let Some(run) = thread.active_run {
                    self.runs.cancel(&run.id)?;
                    true
                } else {
                    false
                };
                json!({ "stopped": stopped })
            }
            "orbyn_thread_interrupt" | "orbyn_task_cancel" => {
                let thread = thread_arg()?;
                if thread.id == own.id {
                    return Err(CoreError::Invalid("A thread cannot stop itself.".into()));
                }
                let stopped = thread.active_run.map(|run| self.runs.cancel(&run.id));
                json!({ "stopped": matches!(stopped, Some(Ok(()))) })
            }
            "orbyn_pull_link" => json!(self.link_pull(&own.id, number(args, "number")?).await?),
            "orbyn_pull_unlink" => {
                self.store.unlink_pull(&own.id, number(args, "number")?)?;
                json!({ "unlinked": true })
            }
            "orbyn_pull_watch" => json!(self.watch_pull(&own.id, number(args, "number")?)?),
            "orbyn_pull_unwatch" => {
                self.store.unwatch_pull(&own.id, number(args, "number")?)?;
                json!({ "unwatched": true })
            }
            "orbyn_pull_guide_read" => {
                let number = pull_number(args)?;
                json!({
                    "number": number,
                    "guide": self.store.pull_guide(&own.project_id, number)?,
                })
            }
            "orbyn_pull_guide_write" => {
                let number = pull_number(args)?;
                let head_sha = guide_head(args)?;
                let (stops, count) = guide_stops(args)?;
                if self.preference("review_guides_enabled", Some(&own.project_id))? != json!(true) {
                    return Err(CoreError::Invalid(
                        "Review guides are off for this project.".into(),
                    ));
                }
                let guide = crate::store::PullGuideWrite {
                    head_sha: head_sha.as_deref(),
                    stops: &stops,
                    thread_id: &own.id,
                };
                let written_at = self
                    .store
                    .write_pull_guide(&own.project_id, number, &guide)?;
                json!({
                    "number": number,
                    "head_sha": head_sha,
                    "stops": count,
                    "written_at": written_at,
                })
            }
            "orbyn_delegate_task" => {
                let nested =
                    own.lifecycle.as_ref().and_then(|l| l.relation.as_deref()) == Some("subagent");
                if nested {
                    return Err(CoreError::Invalid(
                        "A delegated thread cannot delegate further.".into(),
                    ));
                }
                let task = DelegateTask {
                    prompt: text(args, "prompt")?.to_owned(),
                    title: args.get("title").and_then(Value::as_str).map(str::to_owned),
                    engine: args
                        .get("engine")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    model: None,
                    effort: None,
                    options: std::collections::BTreeMap::new(),
                };
                let child = self.delegate(&own.id, &task, &actor)?;
                json!({ "thread_id": child.id })
            }
            "orbyn_schedule_task" => {
                let minutes = u32::try_from(number(args, "every_minutes")?)
                    .map_err(|_| CoreError::Invalid("That interval is too long.".into()))?;
                let task = UpsertTask {
                    id: None,
                    project_id: own.project_id.clone(),
                    thread_id: None,
                    name: text(args, "name")?.to_owned(),
                    prompt: text(args, "prompt")?.to_owned(),
                    engine: own.engine.clone(),
                    schedule: TaskSchedule {
                        kind: "interval".into(),
                        every_minutes: Some(minutes),
                        time: None,
                        weekdays: Vec::new(),
                        utc_offset_minutes: 0,
                        after_thread: None,
                        only_on_success: false,
                    },
                    signature: None,
                    secret: None,
                    filters: Vec::new(),
                    dedupe: None,
                    max_age_seconds: None,
                    timestamp_path: None,
                };
                json!(self.save_task(task, &actor)?)
            }
            "orbyn_schedule_delete" => {
                let id = text(args, "task_id")?;
                self.project_task(binding, id)?;
                self.delete_task(id, &actor)?;
                json!({ "deleted": true })
            }
            "orbyn_schedule_run" => {
                let id = text(args, "task_id")?;
                self.project_task(binding, id)?;
                json!({ "thread_id": self.run_task(id).await? })
            }
            other => return Err(CoreError::NotFound(format!("tool {other}"))),
        })
    }

    /// Records a call before it happens, whichever way the gate then goes.
    pub(crate) fn note_tool(&self, binding: &McpBinding, name: &str) -> Result<()> {
        self.store.append_audit(&AuditRecord {
            actor: format!("engine:{}", binding.scope.engine),
            action: "tool.call".into(),
            target: name.to_owned(),
            decision: None,
            source: "engine".into(),
            thread_id: Some(binding.scope.thread_id.clone()),
            run_id: Some(binding.scope.run_id.clone()),
            detail: Value::Null,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
