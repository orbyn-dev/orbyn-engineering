//! One Claude conversation's messages, read from Claude's own history folder
//! (`CLAUDE_CONFIG_DIR`, else `~/.claude`, then `projects/<folder>/<session>.jsonl`).
//! Reading needs no sign-in and writes nothing. A caller names a session by id, and
//! optionally its working folder, never by path.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::agents::usage::{Engine, history_root};
use crate::error::{CoreError, Result};

/// The largest transcript read, in bytes.
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Project folders looked through when no working folder is given.
const MAX_FOLDERS: usize = 4000;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryOptions {
    /// The session's working folder; only that project's history is read.
    pub dir: Option<String>,
    #[serde(default)]
    pub include_system_messages: bool,
    /// Stop after the message with this id.
    pub up_to_message_id: Option<String>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Claude's folder name for a working folder: every character that is not an ASCII
/// letter or digit becomes `-`, so `C:\code\app` is `C--code-app`.
pub fn project_folder_name(dir: &str) -> String {
    dir.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn find_transcript(root: &Path, session: &str, dir: Option<&str>) -> Option<PathBuf> {
    let file = format!("{session}.jsonl");
    if let Some(dir) = dir {
        let path = root.join(project_folder_name(dir)).join(&file);
        return path.is_file().then_some(path);
    }
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .take(MAX_FOLDERS)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path().join(&file))
        .find(|path| path.is_file())
}

/// The session's user and assistant messages in file order, and system ones when
/// asked. Side-chain (subagent) entries are left out.
pub fn session_messages(session: &str, options: &HistoryOptions) -> Result<Vec<Value>> {
    let root = history_root(Engine::Claude)
        .ok_or_else(|| CoreError::Unsupported("No home folder, so no Claude history.".into()))?;
    session_messages_in(&root, session, options)
}

fn session_messages_in(root: &Path, session: &str, options: &HistoryOptions) -> Result<Vec<Value>> {
    if !valid_id(session) {
        return Err(CoreError::Invalid(
            "That is not a Claude session id.".into(),
        ));
    }
    let path = find_transcript(root, session, options.dir.as_deref())
        .ok_or_else(|| CoreError::NotFound("Claude session".into()))?;
    let file = std::fs::File::open(&path)?;
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(CoreError::Invalid(
            "This Claude session is too large to read here.".into(),
        ));
    }
    let mut messages = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        // A line cut off by a crash is skipped, as Claude itself does.
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
        let wanted = matches!(kind, "user" | "assistant")
            || (kind == "system" && options.include_system_messages);
        if !wanted || entry.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let uuid = entry.get("uuid").cloned().unwrap_or(Value::Null);
        let last = options
            .up_to_message_id
            .as_deref()
            .is_some_and(|wanted| uuid.as_str() == Some(wanted));
        messages.push(json!({
            "type": kind,
            "uuid": uuid,
            "session_id": entry.get("sessionId").cloned().unwrap_or_else(|| json!(session)),
            "message": entry.get("message").cloned().unwrap_or(Value::Null),
            "parent_tool_use_id": Value::Null,
        }));
        if last {
            break;
        }
    }
    Ok(messages)
}

/// A UUID in the shape the engine and the host accept. Matching against the file is
/// exact, as the host's is, so a rewritten value is never guessed at.
pub(crate) fn valid_uuid(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

/// A message a subagent wrote, or one answering a tool call, is not at the top level of
/// the conversation and can never be the point a rewind resumes at.
fn has_parent(entry: &Value) -> bool {
    match entry.get("parent_tool_use_id") {
        None | Some(Value::Null) => false,
        Some(Value::String(id)) => !id.is_empty(),
        Some(_) => true,
    }
}

fn top_level(entry: &Value) -> bool {
    !has_parent(entry) && entry.get("isSidechain").and_then(Value::as_bool) != Some(true)
}

/// The uuid to resume a Claude session at to undo the run whose prompt user message is
/// `prompt_id`: the last top-level assistant message before that user message.
pub(crate) fn resume_point(session: &str, prompt_id: &str, dir: Option<&Path>) -> Result<String> {
    let root = history_root(Engine::Claude)
        .ok_or_else(|| CoreError::Unsupported("No home folder, so no Claude history.".into()))?;
    resume_point_in(&root, session, prompt_id, dir)
}

pub(crate) fn resume_point_in(
    root: &Path,
    session: &str,
    prompt_id: &str,
    dir: Option<&Path>,
) -> Result<String> {
    if !valid_id(session) {
        return Err(CoreError::Invalid(
            "That is not a Claude session id.".into(),
        ));
    }
    if !valid_uuid(prompt_id) {
        return Err(CoreError::Invalid("That is not a Claude prompt id.".into()));
    }
    // The path comes from the stored session id, never from a request value. The thread's
    // folder is only the first hint: a native Claude thread starts in the library when its
    // session lives there, while the thread keeps the checkout folder. If the hint holds no
    // file, the session id is looked up across the history root, as the console's
    // `session_cwd` does. The host, sent the session and its own folder, validates the uuid
    // against the same file before it resumes.
    let dir = dir.map(|dir| dir.to_string_lossy());
    let path = match find_transcript(root, session, dir.as_deref()) {
        Some(path) => path,
        None if dir.is_some() => find_transcript(root, session, None)
            .ok_or_else(|| CoreError::NotFound("Claude session".into()))?,
        None => return Err(CoreError::NotFound("Claude session".into())),
    };
    let file = std::fs::File::open(&path)?;
    if file.metadata()?.len() > MAX_FILE_BYTES {
        return Err(CoreError::Invalid(
            "This Claude session is too large to read here.".into(),
        ));
    }
    let mut earlier: Option<String> = None;
    let mut found_prompt = false;
    for line in BufReader::new(file).lines() {
        let line = line?;
        // A line cut off by a crash is skipped, as Claude itself does. A file that lost
        // the prompt this way cannot name a point, and the rewind falls back.
        let Ok(entry) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if !top_level(&entry) {
            continue;
        }
        match entry.get("type").and_then(Value::as_str) {
            Some("user") if entry.get("uuid").and_then(Value::as_str) == Some(prompt_id) => {
                found_prompt = true;
                break;
            }
            Some("assistant") => {
                if let Some(uuid) = entry
                    .get("uuid")
                    .and_then(Value::as_str)
                    .filter(|uuid| valid_uuid(uuid))
                {
                    earlier = Some(uuid.to_owned());
                }
            }
            _ => {}
        }
    }
    if !found_prompt {
        return Err(CoreError::NotFound("the run's Claude prompt".into()));
    }
    earlier.ok_or_else(|| CoreError::NotFound("an earlier Claude assistant message".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn write(root: &Path, folder: &str, session: &str, lines: &[Value]) {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let mut text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        text.push_str("{\"type\":\"user\",\"cut");
        std::fs::write(dir.join(format!("{session}.jsonl")), text).unwrap();
    }

    fn write_whole(root: &Path, folder: &str, session: &str, lines: &[Value]) {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
        std::fs::write(dir.join(format!("{session}.jsonl")), text).unwrap();
    }

    fn write_raw(root: &Path, folder: &str, session: &str, text: &str) {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{session}.jsonl")), text).unwrap();
    }

    const SESSION: &str = "0f3c2b1a-1111-4222-8333-444455556666";
    const PROMPT: &str = "a1a1a1a1-1111-4222-8333-444455556666";
    const BEFORE: &str = "b2b2b2b2-1111-4222-8333-444455556666";

    fn user(uuid: &str) -> Value {
        json!({"type":"user","uuid":uuid,"sessionId":SESSION,"message":{"role":"user","content":"hi"}})
    }

    fn tool_result(uuid: &str) -> Value {
        json!({
            "type":"user",
            "uuid":uuid,
            "sessionId":SESSION,
            "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}]}
        })
    }

    fn assistant(uuid: &str) -> Value {
        json!({"type":"assistant","uuid":uuid,"sessionId":SESSION,"message":{"role":"assistant","content":"answer"}})
    }

    #[test]
    fn folder_names_follow_claude_on_unix_and_windows_paths() {
        assert_eq!(project_folder_name("/Users/a/my app"), "-Users-a-my-app");
        assert_eq!(
            project_folder_name(r"C:\Users\a\my_app"),
            "C--Users-a-my-app"
        );
    }

    #[test]
    fn reads_messages_in_order_up_to_an_id_and_only_where_asked() {
        let dir = TempDir::new();
        let root = dir.path();
        let id = "0f3c2b1a-1111-4222-8333-444455556666";
        write(
            root,
            &project_folder_name(r"C:\work\app"),
            id,
            &[
                json!({"type":"summary","summary":"x"}),
                json!({"type":"user","uuid":"u1","sessionId":id,"message":{"role":"user","content":"hi"}}),
                json!({"type":"system","uuid":"s1","sessionId":id,"content":"note"}),
                json!({"type":"assistant","uuid":"a1","sessionId":id,"isSidechain":true,"message":{}}),
                json!({"type":"assistant","uuid":"a2","sessionId":id,"message":{"role":"assistant","content":"hello"}}),
                json!({"type":"user","uuid":"u3","sessionId":id,"message":{"role":"user","content":"more"}}),
            ],
        );
        let all = session_messages_in(root, id, &HistoryOptions::default()).unwrap();
        let uuids: Vec<_> = all.iter().map(|m| m["uuid"].clone()).collect();
        assert_eq!(uuids, [json!("u1"), json!("a2"), json!("u3")]);
        assert_eq!(all[1]["message"]["content"], "hello");

        let options = HistoryOptions {
            dir: Some(r"C:\work\app".into()),
            include_system_messages: true,
            up_to_message_id: Some("a2".into()),
        };
        let some = session_messages_in(root, id, &options).unwrap();
        let uuids: Vec<_> = some.iter().map(|m| m["uuid"].clone()).collect();
        assert_eq!(uuids, [json!("u1"), json!("s1"), json!("a2")]);

        let elsewhere = HistoryOptions {
            dir: Some("/other".into()),
            ..HistoryOptions::default()
        };
        assert!(matches!(
            session_messages_in(root, id, &elsewhere),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn an_id_cannot_name_a_path() {
        let dir = TempDir::new();
        for id in ["../secret", r"..\secret", "", "a/b"] {
            assert!(matches!(
                session_messages_in(dir.path(), id, &HistoryOptions::default()),
                Err(CoreError::Invalid(_))
            ));
        }
    }

    #[test]
    fn a_rewind_point_is_the_last_top_level_assistant_before_the_prompt() {
        let dir = TempDir::new();
        let cwd = r"C:\work\app";
        let folder = project_folder_name(cwd);
        write_whole(
            dir.path(),
            &folder,
            SESSION,
            &[
                json!({"type":"summary","summary":"x"}),
                user("u0"),
                assistant("old"),
                json!({"type":"assistant","uuid":"sub-1","sessionId":SESSION,"isSidechain":true,"message":{}}),
                json!({"type":"assistant","uuid":"sub-2","sessionId":SESSION,"parent_tool_use_id":"toolu_1","message":{}}),
                tool_result("tool-1"),
                assistant(BEFORE),
                // A subagent that speaks after the last top-level message must not be the point.
                json!({"type":"assistant","uuid":"sub-3","sessionId":SESSION,"isSidechain":true,"message":{}}),
                json!({"type":"assistant","uuid":"sub-4","sessionId":SESSION,"parent_tool_use_id":"toolu_2","message":{}}),
                user(PROMPT),
                assistant("after"),
            ],
        );
        assert_eq!(
            resume_point_in(dir.path(), SESSION, PROMPT, Some(Path::new(cwd))).unwrap(),
            BEFORE
        );
    }

    #[test]
    fn a_wrong_folder_hint_still_finds_the_session_by_id() {
        let dir = TempDir::new();
        let actual = project_folder_name("/library");
        write_whole(
            dir.path(),
            &actual,
            SESSION,
            &[assistant(BEFORE), user(PROMPT)],
        );
        assert_eq!(
            resume_point_in(dir.path(), SESSION, PROMPT, Some(Path::new("/checkout"))).unwrap(),
            BEFORE,
            "a library thread keeps the checkout folder"
        );
    }

    #[test]
    fn a_rewind_point_needs_the_prompt_and_an_earlier_top_level_assistant() {
        let dir = TempDir::new();
        let folder = project_folder_name("/one");
        write_whole(dir.path(), &folder, SESSION, &[user(PROMPT)]);
        assert!(matches!(
            resume_point_in(dir.path(), SESSION, PROMPT, Some(Path::new("/one"))),
            Err(CoreError::NotFound(_))
        ));

        let dir = TempDir::new();
        write_whole(
            dir.path(),
            &folder,
            SESSION,
            &[
                json!({"type":"assistant","uuid":"sub","sessionId":SESSION,"isSidechain":true,"message":{}}),
                user(PROMPT),
            ],
        );
        assert!(matches!(
            resume_point_in(dir.path(), SESSION, PROMPT, Some(Path::new("/one"))),
            Err(CoreError::NotFound(_))
        ));

        let dir = TempDir::new();
        write_whole(
            dir.path(),
            &folder,
            SESSION,
            &[assistant(BEFORE), user("another-prompt")],
        );
        assert!(matches!(
            resume_point_in(dir.path(), SESSION, PROMPT, Some(Path::new("/one"))),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn a_rewind_point_refuses_an_invalid_session_or_prompt_id_and_a_missing_file() {
        let dir = TempDir::new();
        for session in ["../secret", r"..\secret", "", "a/b"] {
            assert!(matches!(
                resume_point_in(dir.path(), session, PROMPT, None),
                Err(CoreError::Invalid(_))
            ));
        }
        for prompt in ["not-a-uuid", "../secret", "", "a1a1a1a1-1111-4222-8333"] {
            assert!(matches!(
                resume_point_in(dir.path(), SESSION, prompt, None),
                Err(CoreError::Invalid(_))
            ));
        }
        assert!(matches!(
            resume_point_in(dir.path(), SESSION, PROMPT, None),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn a_truncated_or_rewritten_file_never_yields_a_guess() {
        let dir = TempDir::new();
        let folder = project_folder_name("/one");
        write_raw(
            dir.path(),
            &folder,
            SESSION,
            "{\"type\":\"assistant\",\"uuid\":\"cut",
        );
        assert!(matches!(
            resume_point_in(dir.path(), SESSION, PROMPT, Some(Path::new("/one"))),
            Err(CoreError::NotFound(_))
        ));

        // The prompt is gone, as after a rewrite: the assistant before it is not guessed at.
        let dir = TempDir::new();
        write_whole(
            dir.path(),
            &folder,
            SESSION,
            &[assistant(BEFORE), user("rewritten")],
        );
        assert!(matches!(
            resume_point_in(dir.path(), SESSION, PROMPT, Some(Path::new("/one"))),
            Err(CoreError::NotFound(_))
        ));
    }
}
