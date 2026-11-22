//! The earlier-messages read of an imported thread through the real service and its own
//! Claude and Codex history folders: paging, mapping, redaction and a read that writes
//! nothing.

mod support;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use support::{Options, Service, TempDir, setup};

fn write_session(path: &Path, lines: &[Value]) -> Vec<u8> {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text: Vec<String> = lines.iter().map(ToString::to_string).collect();
    let bytes = text.join("\n").into_bytes();
    std::fs::write(path, &bytes).unwrap();
    bytes
}

fn claude_user(project: &str, session: &str, text: &str) -> Value {
    json!({
        "type": "user",
        "sessionId": session,
        "cwd": project,
        "message": {"role": "user", "content": [{"type": "text", "text": text}]},
    })
}

async fn project_path(service: &Service) -> String {
    service
        .json("/api/projects")
        .await
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["scratch"] != true)
        .unwrap()["path"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn import(service: &Service, engine: &str, session: &str) -> String {
    let (status, body) = service
        .send(
            "/api/history/import",
            json!({"engine": engine, "session_id": session}),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    body["thread_id"].as_str().unwrap().to_owned()
}

async fn earlier(service: &Service, thread: &str, before: Option<&str>) -> (u16, Value) {
    let mut request = service.get(&format!("/api/threads/{thread}/earlier"));
    if let Some(before) = before {
        request = request.query(&[("before", before)]);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

fn counts(data: &Path) -> (i64, i64, i64, i64) {
    let conn = rusqlite::Connection::open(data.join("orbyn.sqlite3")).unwrap();
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
}

fn claude_home(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(name)
}

#[tokio::test]
async fn an_imported_claude_thread_pages_its_history_without_writing() {
    let dir = TempDir::new();
    let home = claude_home(&dir, "claude-home");
    let options = Options {
        env: vec![(
            "CLAUDE_CONFIG_DIR".into(),
            home.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    let (service, _ordinary) = setup(&dir, options, "claude").await;
    let project = project_path(&service).await;
    let file = home.join("projects/-project/s-120.jsonl");
    let lines: Vec<Value> = (0..120)
        .map(|at| claude_user(&project, "s-120", &format!("message {at}")))
        .collect();
    let before = write_session(&file, &lines);
    let thread = import(&service, "claude", "s-120").await;

    let data = dir.path().join("data");
    let rows_before = counts(&data);
    let (status, first) = earlier(&service, &thread, None).await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["source"], "Claude Code");
    assert_eq!(first["total"], 120);
    assert_eq!(first["messages"].as_array().unwrap().len(), 50);
    assert_eq!(first["messages"][0]["role"], "user");
    assert_eq!(first["messages"][0]["text"], "message 70");
    assert_eq!(first["messages"][49]["text"], "message 119");
    assert!(first["imported_at"].is_string());
    let thread_body = service.json(&format!("/api/threads/{thread}")).await;
    assert_eq!(first["imported_at"], thread_body["imported_at"]);
    let cursor = first["next_before"].as_str().unwrap().to_owned();

    let (status, second) = earlier(&service, &thread, Some(&cursor)).await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["messages"][0]["text"], "message 20");
    assert_eq!(second["messages"][49]["text"], "message 69");
    let cursor = second["next_before"].as_str().unwrap().to_owned();

    let (status, third) = earlier(&service, &thread, Some(&cursor)).await;
    assert_eq!(status, 200, "{third}");
    assert_eq!(third["messages"].as_array().unwrap().len(), 20);
    assert_eq!(third["messages"][0]["text"], "message 0");
    assert_eq!(third["messages"][19]["text"], "message 19");
    assert!(third["next_before"].is_null());

    assert_eq!(counts(&data), rows_before, "a read writes no row");
    assert_eq!(
        std::fs::read(&file).unwrap(),
        before,
        "the session file is unchanged"
    );

    // A changed file no longer matches the cursor the read wrote.
    let mut changed = before.clone();
    changed.extend_from_slice(b"\n");
    changed.extend_from_slice(
        claude_user(&project, "s-120", "appended")
            .to_string()
            .as_bytes(),
    );
    std::fs::write(&file, &changed).unwrap();
    let (status, body) = earlier(&service, &thread, Some(&cursor)).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["code"], "history_unreadable");

    // A cursor that is not one Orbyn wrote is refused on its own.
    let (status, body) = earlier(&service, &thread, Some("not-a-cursor")).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["code"], "bad_cursor");

    service.stop().await;
}

#[tokio::test]
async fn the_read_maps_roles_tools_thinking_images_and_redacts_text() {
    let dir = TempDir::new();
    let home = claude_home(&dir, "claude-home");
    let options = Options {
        env: vec![(
            "CLAUDE_CONFIG_DIR".into(),
            home.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    let (service, _ordinary) = setup(&dir, options, "claude").await;
    let project = project_path(&service).await;
    let secret = "sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let long = "x".repeat(20_000);
    let lines = [
        claude_user(&project, "s-map", &format!("my key is {secret}")),
        json!({
            "type": "assistant", "sessionId": "s-map", "cwd": project,
            "message": {"id": "a1", "role": "assistant", "content": [
                {"type": "thinking", "thinking": "THINKING_SENTINEL"},
                {"type": "text", "text": "the answer"},
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/ARG_SENTINEL"}}
            ]},
        }),
        json!({
            "type": "user", "sessionId": "s-map", "cwd": project,
            "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "OUTPUT_SENTINEL"}
            ]},
        }),
        json!({"type": "system", "sessionId": "s-map", "cwd": project, "content": "META_SENTINEL"}),
        json!({
            "type": "assistant", "sessionId": "s-map", "cwd": project,
            "message": {"id": "a2", "role": "assistant", "content": [
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}},
                {"type": "text", "text": "after the picture"}
            ]},
        }),
        claude_user(&project, "s-map", &long),
    ];
    write_session(&home.join("projects/-project/s-map.jsonl"), &lines);
    let thread = import(&service, "claude", "s-map").await;

    let (status, page) = earlier(&service, &thread, None).await;
    assert_eq!(status, 200, "{page}");
    let messages = page["messages"].as_array().unwrap();
    let texts: Vec<&str> = messages
        .iter()
        .map(|m| m["text"].as_str().unwrap())
        .collect();
    assert_eq!(messages.len(), 6, "{page}");
    assert_eq!(page["total"], 6);
    assert_eq!(texts[0], "my key is [hidden: API key]");
    assert_eq!(texts[1], "the answer");
    assert_eq!(texts[2], "Read a file");
    assert_eq!(texts[3], "[image]");
    assert_eq!(texts[4], "after the picture");
    assert!(texts[5].ends_with('…'));
    assert!(texts[5].len() <= 16 * 1024);
    for sentinel in [
        "THINKING_SENTINEL",
        "ARG_SENTINEL",
        "OUTPUT_SENTINEL",
        "META_SENTINEL",
        secret,
    ] {
        assert!(!page.to_string().contains(sentinel), "{page}");
    }

    service.stop().await;
}

#[tokio::test]
async fn a_codex_thread_reads_event_messages_and_tool_calls_without_their_arguments() {
    let dir = TempDir::new();
    let home = dir.path().join("codex-home");
    let options = Options {
        env: vec![("CODEX_HOME".into(), home.to_string_lossy().into_owned())],
        ..Options::default()
    };
    let (service, _ordinary) = setup(&dir, options, "codex").await;
    let project = project_path(&service).await;
    let secret = "sk-ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let lines = [
        json!({"type": "session_meta", "payload": {"id": "k-map", "cwd": project}}),
        json!({"type": "event_msg", "payload": {"type": "user_message", "message": "run the build"}}),
        json!({"type": "response_item", "payload": {
            "type": "function_call", "name": "shell",
            "arguments": json!({"command": format!("curl {secret}")}).to_string(),
            "call_id": "c1"
        }}),
        json!({"type": "response_item", "payload": {"type": "reasoning", "summary": [{"type": "summary_text", "text": "THINKING_SENTINEL"}]}}),
        json!({"type": "event_msg", "payload": {"type": "agent_message", "message": "cargo test passes"}}),
        json!({"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_image", "image_url": "data:image/png;base64,AAAA"}]}}),
    ];
    write_session(&home.join("sessions/2026/11/06/rollout-map.jsonl"), &lines);
    let thread = import(&service, "codex", "k-map").await;

    let (status, page) = earlier(&service, &thread, None).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["source"], "Codex");
    assert_eq!(page["total"], 4);
    let messages = page["messages"].as_array().unwrap();
    let texts: Vec<&str> = messages
        .iter()
        .map(|m| m["text"].as_str().unwrap())
        .collect();
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(texts[0], "run the build");
    assert_eq!(messages[1]["role"], "tool");
    assert_eq!(texts[1], "Ran a command");
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(texts[2], "cargo test passes");
    assert_eq!(texts[3], "[image]");
    for sentinel in ["THINKING_SENTINEL", secret, "curl"] {
        assert!(!page.to_string().contains(sentinel), "{page}");
    }

    service.stop().await;
}

#[tokio::test]
async fn a_missing_file_or_a_thread_with_no_import_record_is_refused() {
    let dir = TempDir::new();
    let home = claude_home(&dir, "claude-home");
    let options = Options {
        env: vec![(
            "CLAUDE_CONFIG_DIR".into(),
            home.to_string_lossy().into_owned(),
        )],
        ..Options::default()
    };
    let (service, ordinary) = setup(&dir, options, "claude").await;
    let project = project_path(&service).await;
    let file = home.join("projects/-project/s-gone.jsonl");
    write_session(&file, &[claude_user(&project, "s-gone", "hello")]);
    let thread = import(&service, "claude", "s-gone").await;

    let (status, body) = earlier(&service, &ordinary, None).await;
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["code"], "not_imported");

    std::fs::remove_file(&file).unwrap();
    let (status, body) = earlier(&service, &thread, None).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["code"], "history_unreadable");

    service.stop().await;
}
