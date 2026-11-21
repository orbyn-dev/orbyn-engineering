//! Transcript export and import over HTTP (docs/attachments-transcripts.md): the export is redacted and carries no path, key or
//! value; the import is a `Person` route that reads an untrusted file with limits, drops and reports bad entries, writes nothing
//! outside the new thread and marks the thread.

use std::sync::Arc;

use orbyn_contracts::{
    AgUiEvent, PatternKind, PatternScope, RedactionPatternInput, TRANSCRIPT_FILE_ENDING,
    TRANSCRIPT_MEDIA_TYPE,
};
use orbyn_core::store::{NewProject, NewThread};
use orbyn_core::{Core, CoreConfig};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::router;
use crate::http::AppState;
use crate::http::access::Access;

const KEY: &str = "the-person-key-5521";

struct Served {
    port: u16,
    core: Arc<Core>,
    project: String,
    thread: String,
    work: std::path::PathBuf,
    dir: std::path::PathBuf,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn github_token() -> String {
    format!("ghp_{}", "aB3dE5gH7jK9mN1pQ3sT5vW7yZ9bC1dE3fG5")
}

async fn serve(name: &str) -> Served {
    let dir = std::env::temp_dir().join(format!("orbynd-transcript-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let core = Arc::new(
        Core::open(CoreConfig::new(
            dir.join("data"),
            "http://127.0.0.1:1".into(),
        ))
        .unwrap(),
    );
    let project = core
        .store
        .add_project(NewProject {
            path: &work.to_string_lossy(),
            name: "p",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "user").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id.clone(),
            title: "Redirect loop".into(),
            engine: "claude".into(),
            model: Some("opus".into()),
            cwd: work.to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let app = router(AppState {
        core: core.clone(),
        access: Arc::new(Access::new("token", vec![], None).with_person_key(Some(KEY))),
        remote: false,
        ui_dir: None,
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    Served {
        port,
        core,
        project: project.id,
        thread: thread.id,
        work,
        dir,
    }
}

impl Served {
    fn say(&self, thread: &str, role: &str, text: &str) {
        let id = format!(
            "m{}",
            self.core.store.thread_messages(thread, 1000).unwrap().len()
        );
        for event in [
            AgUiEvent::TextMessageStart {
                message_id: id.clone(),
                role: role.into(),
            },
            AgUiEvent::TextMessageContent {
                message_id: id.clone(),
                delta: text.into(),
            },
            AgUiEvent::TextMessageEnd {
                message_id: id.clone(),
            },
        ] {
            self.core.store.append_thread_event(thread, &event).unwrap();
        }
    }

    async fn get(&self, path: &str) -> (u16, Value) {
        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{}{path}", self.port))
            .bearer_auth("token")
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn import_raw(&self, query: &str, body: Vec<u8>, person: bool) -> (u16, Value) {
        let mut request = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/api/threads/import?project_id={}{query}",
                self.port, self.project
            ))
            .bearer_auth("token")
            .header("content-type", TRANSCRIPT_MEDIA_TYPE)
            .body(body);
        if person {
            request = request.header("x-orbyn-person", KEY);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        (status, response.json().await.unwrap_or(Value::Null))
    }

    async fn import(&self, query: &str, document: &Value) -> (u16, Value) {
        self.import_raw(query, serde_json::to_vec(document).unwrap(), true)
            .await
    }

    fn threads(&self) -> usize {
        self.core
            .store
            .list_all_threads(&self.project)
            .unwrap()
            .len()
    }
}

fn document(messages: Value) -> Value {
    json!({
        "format": "orbyn-transcript", "version": 1, "exported_at": "2026-11-22T10:00:00Z",
        "thread": { "title": "From a file", "engine": "claude", "model": "opus", "created_at": "2026-11-20T10:00:00Z" },
        "messages": messages, "omitted_earlier": 0
    })
}

#[test]
fn the_file_ending_and_media_type_are_the_pinned_constants() {
    assert_eq!(TRANSCRIPT_FILE_ENDING, ".orbyn.json");
    assert_eq!(
        TRANSCRIPT_MEDIA_TYPE,
        "application/vnd.orbyn.transcript+json"
    );
}

#[tokio::test]
async fn the_export_hides_secrets_paths_and_the_person_key() {
    let s = serve("export").await;
    s.core
        .save_redaction_pattern(
            "user",
            None,
            &RedactionPatternInput {
                name: "Corp password".into(),
                pattern: "corp-pass-7731".into(),
                kind: PatternKind::Text,
                applies_to: PatternScope::Anywhere,
                accept_broad: None,
            },
        )
        .unwrap();
    let work = s.work.to_string_lossy().into_owned();
    s.say(
        &s.thread,
        "user",
        &format!(
            "token {} and corp-pass-7731, see {work}/src/main.rs and /Users/someone/.ssh/config",
            github_token()
        ),
    );
    s.say(&s.thread, "assistant", "Looked at it.");
    // A tool line and a reasoning message are not exported.
    s.say(&s.thread, "developer", "internal");
    let (status, doc) = s.get(&format!("/api/threads/{}/export", s.thread)).await;
    assert_eq!(status, 200, "{doc}");
    assert_eq!(doc["format"], "orbyn-transcript");
    assert_eq!(doc["version"], 1);
    assert_eq!(doc["thread"]["title"], "Redirect loop");
    assert_eq!(doc["thread"]["engine"], "claude");
    assert_eq!(doc["messages"].as_array().unwrap().len(), 2);
    let text = doc.to_string();
    for leak in [
        github_token().as_str(),
        "corp-pass-7731",
        work.as_str(),
        "/Users/someone",
        KEY,
        "engine_session",
    ] {
        assert!(!text.contains(leak), "the export holds {leak}: {text}");
    }
    let first = doc["messages"][0]["text"].as_str().unwrap();
    assert!(first.contains("[hidden: GitHub token]"), "{first}");
    assert!(first.contains("[hidden: Corp password]"), "{first}");
    assert!(first.contains("<project>/src/main.rs"), "{first}");
}

#[tokio::test]
async fn the_export_hides_secrets_even_when_the_surface_switches_are_off() {
    let s = serve("surfaces").await;
    s.core
        .change_redaction_surfaces(
            "user",
            &orbyn_contracts::RedactionSurfaceChange {
                terminal: Some(false),
                agents: Some(false),
            },
        )
        .unwrap();
    s.say(&s.thread, "user", &format!("token {}", github_token()));
    let (_, doc) = s.get(&format!("/api/threads/{}/export", s.thread)).await;
    assert!(!doc.to_string().contains(&github_token()), "{doc}");
}

#[tokio::test]
async fn an_export_imports_and_exports_the_same_messages_and_marks_the_thread() {
    let s = serve("round").await;
    s.say(&s.thread, "user", "Why does logout loop?");
    s.say(
        &s.thread,
        "assistant",
        "The cookie is set twice.\nSecond line.",
    );
    let (_, first) = s.get(&format!("/api/threads/{}/export", s.thread)).await;
    let before = s.threads();
    let (status, answer) = s.import("", &first).await;
    assert_eq!(status, 201, "{answer}");
    assert_eq!(s.threads(), before + 1);
    assert_eq!(answer["message_count"], 2);
    assert_eq!(answer["dropped"], 0);
    let thread = &answer["thread"];
    assert_eq!(thread["title"], "Redirect loop");
    assert_eq!(thread["source"], "orbyn-transcript");
    assert!(thread["imported_at"].as_str().unwrap().ends_with('Z'));
    // A thread made here carries neither marker.
    let (_, own) = s.get(&format!("/api/threads/{}", s.thread)).await;
    assert!(
        own.get("source").is_none() && own.get("imported_at").is_none(),
        "{own}"
    );
    // Read-only history: no run, no model carried over from the file.
    assert!(thread["active_run"].is_null() && thread["last_run_state"].is_null());
    assert!(thread.get("model").is_none_or(Value::is_null));
    let id = thread["id"].as_str().unwrap();
    let (_, second) = s.get(&format!("/api/threads/{id}/export")).await;
    assert_eq!(first["messages"], second["messages"]);
    assert_eq!(first["thread"]["title"], second["thread"]["title"]);
}

#[tokio::test]
async fn a_service_token_alone_gets_403_on_import_and_nothing_is_made() {
    let s = serve("token").await;
    let before = s.threads();
    let body = serde_json::to_vec(&document(
        json!([{ "role": "user", "ts": "2026-11-20T10:00:00Z", "text": "hi" }]),
    ))
    .unwrap();
    let (status, answer) = s.import_raw("", body, false).await;
    assert_eq!(status, 403, "{answer}");
    assert_eq!(answer["code"], "person_required");
    assert_eq!(s.threads(), before);
}

#[tokio::test]
async fn a_file_over_two_mebibytes_is_refused_413() {
    let s = serve("size").await;
    let (status, answer) = s
        .import_raw("", vec![b' '; 2 * 1024 * 1024 + 1], true)
        .await;
    assert_eq!(status, 413, "{answer}");
    assert_eq!(answer["code"], "file_too_large");
    assert_eq!(s.threads(), 1);
}

#[tokio::test]
async fn an_unknown_format_or_version_is_refused_and_a_newer_one_says_so() {
    let s = serve("version").await;
    for (patch, code) in [
        (
            json!({ "format": "orbyn-library" }),
            "transcript_unreadable",
        ),
        (json!({ "version": 0 }), "transcript_unreadable"),
        (json!({ "version": "1" }), "transcript_unreadable"),
        (json!({ "version": 2 }), "transcript_too_new"),
    ] {
        let mut doc = document(json!([]));
        doc.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        let (status, answer) = s.import("", &doc).await;
        assert_eq!(status, 422, "{answer}");
        assert_eq!(answer["code"], code, "{answer}");
    }
    for body in ["not json", "[]", "{}", "null"] {
        let (status, answer) = s.import_raw("", body.as_bytes().to_vec(), true).await;
        assert_eq!(status, 422, "{body}: {answer}");
        assert_eq!(answer["code"], "transcript_unreadable");
    }
    assert_eq!(s.threads(), 1);
}

#[tokio::test]
async fn a_deeply_nested_file_is_refused_and_nothing_is_made() {
    let s = serve("depth").await;
    let mut deep = json!("x");
    for _ in 0..20 {
        deep = json!({ "a": deep });
    }
    let mut doc = document(json!([]));
    doc["extra"] = deep;
    let (status, answer) = s.import("", &doc).await;
    assert_eq!(status, 422, "{answer}");
    assert_eq!(answer["code"], "transcript_unreadable");
    // Past the parser's own limit as well.
    let nested = format!("{}1{}", "[".repeat(5000), "]".repeat(5000));
    let (status, _) = s.import_raw("", nested.into_bytes(), true).await;
    assert_eq!(status, 422);
    assert_eq!(s.threads(), 1);
}

#[tokio::test]
async fn bad_entries_are_dropped_with_a_reason_and_unknown_fields_never_land() {
    let s = serve("drop").await;
    let huge = "x".repeat(200 * 1024);
    let mut doc = document(json!([
        { "role": "user", "ts": "2026-11-20T10:00:00Z", "text": "ok", "tool": "rm -rf", "path": "/etc/passwd" },
        { "role": "system", "ts": "2026-11-20T10:00:01Z", "text": "nope" },
        { "role": "assistant", "ts": "not a time", "text": "late\u{0}\u{7}\u{202E}text\u{1b}[31m" },
        { "role": "user", "text": 5 },
        "a string",
        { "role": "assistant", "ts": "2026-11-20T10:00:02Z", "text": huge },
        { "role": "user", "ts": "2026-11-20T10:00:03Z", "text": "   " }
    ]));
    doc["thread"]["title"] = json!("Title\u{0}with\u{7}controls\u{202E}");
    doc["thread"]["engine_session"] = json!("session-abc");
    doc["env"] = json!({ "SECRET": "value" });
    doc["thread"]["cwd"] = json!("/etc");
    let (status, answer) = s.import("", &doc).await;
    assert_eq!(status, 201, "{answer}");
    assert_eq!(answer["message_count"], 3);
    assert_eq!(answer["dropped"], 4);
    assert_eq!(answer["title"], "Titlewithcontrols");
    let codes: Vec<String> = answer["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            format!(
                "{}:{}:{}",
                p["kind"].as_str().unwrap(),
                p["index"],
                p["code"].as_str().unwrap()
            )
        })
        .collect();
    for expected in [
        "document:null:unknown_fields_dropped",
        "thread:null:unknown_fields_dropped",
        "message:0:unknown_fields_dropped",
        "message:1:unknown_role",
        "message:2:time_replaced",
        "message:3:text_missing",
        "message:4:not_an_object",
        "message:5:text_cut",
        "message:6:text_empty",
    ] {
        assert!(
            codes.iter().any(|c| c == expected),
            "{expected} in {codes:?}"
        );
    }
    let id = answer["thread"]["id"].as_str().unwrap();
    let messages = s.core.store.thread_messages(id, 100).unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1].text, "latetext[31m");
    assert!(messages[2].text.len() <= 32 * 1024);
    // The thread is in the project's own folder, whatever the file said.
    let thread = s.core.store.thread(id).unwrap();
    assert_eq!(thread.cwd, s.work.to_string_lossy());
    let log = serde_json::to_string(&s.core.store.events_after(id, 0, 100).unwrap()).unwrap();
    assert!(
        !log.contains("session-abc") && !log.contains("/etc/passwd") && !log.contains("rm -rf")
    );
}

#[tokio::test]
async fn too_many_messages_drop_the_rest_and_a_dry_run_makes_nothing() {
    let s = serve("dry").await;
    let many: Vec<Value> = (0..2005)
        .map(|n| json!({ "role": "user", "ts": "2026-11-20T10:00:00Z", "text": format!("m{n}") }))
        .collect();
    let audits = s.core.store.list_audit(None, 1000).unwrap().len();
    let (status, answer) = s.import("&dry_run=true", &document(json!(many))).await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["dry_run"], true);
    assert!(answer["thread"].is_null());
    assert_eq!(answer["message_count"], 2000);
    assert_eq!(answer["dropped"], 5);
    assert_eq!(s.threads(), 1);
    assert_eq!(s.core.store.list_audit(None, 1000).unwrap().len(), audits);
    let (status, answer) = s.import("", &document(json!(many))).await;
    assert_eq!(status, 201, "{answer}");
    assert_eq!(answer["message_count"], 2000);
}

#[tokio::test]
async fn an_import_writes_no_file_and_one_audit_row_with_counts_only() {
    let s = serve("audit").await;
    let listing = |dir: &std::path::Path| -> Vec<String> {
        let mut names: Vec<String> = walk(dir);
        names.retain(|name| {
            !name.ends_with(".sqlite3-wal")
                && !name.ends_with(".sqlite3-shm")
                && !name.ends_with(".sqlite3")
        });
        names.sort();
        names
    };
    let work_before = listing(&s.work);
    let data_before = listing(&s.dir.join("data"));
    let (status, answer) = s
        .import(
            "",
            &document(
                json!([{ "role": "user", "ts": "2026-11-20T10:00:00Z", "text": "PRIVATE-BODY" }]),
            ),
        )
        .await;
    assert_eq!(status, 201, "{answer}");
    assert_eq!(listing(&s.work), work_before);
    assert_eq!(listing(&s.dir.join("data")), data_before);
    let rows: Vec<_> = s
        .core
        .store
        .list_audit(None, 50)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == "thread.imported")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].detail,
        json!({ "source": "orbyn-transcript", "messages": 1, "dropped": 0 })
    );
    assert!(
        !serde_json::to_string(&rows[0])
            .unwrap()
            .contains("PRIVATE-BODY")
    );
}

#[tokio::test]
async fn the_engine_must_be_one_this_install_has() {
    let s = serve("engine").await;
    let mut doc = document(json!([]));
    doc["thread"]["engine"] = json!("no-such-engine");
    let (status, _) = s.import("", &doc).await;
    assert_eq!(status, 404);
    let (status, answer) = s.import("&engine=claude", &doc).await;
    assert_eq!(status, 201, "{answer}");
    assert_eq!(answer["thread"]["engine"], "claude");
}

fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            out.push(path.to_string_lossy().into_owned());
            if path.is_dir() {
                out.extend(walk(&path));
            }
        }
    }
    out
}
