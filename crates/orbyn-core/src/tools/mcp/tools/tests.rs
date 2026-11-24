use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orbyn_contracts::{
    ConnectTracker, IssueDetail, IssueState, IssueStateCategory, IssueSummary, PermissionMode,
    PullCheck, PullChecks, PullRequest, PullRequestDetail, TrackerKind,
};
use serde_json::{Value, json};

use crate::app::Core;
use crate::browser::preview::{
    DEFAULT_SCROLL, MAX_SCROLL, MAX_SELECTOR_CHARS, MAX_TYPE_CHARS, MAX_WAIT_TEXT_CHARS,
    PreviewAct, PreviewAction,
};
use crate::credentials::{Credentials, MemoryBackend};
use crate::error::CoreError;
use crate::governance::approvals::RunScope;
use crate::issues::{IssueQuery, IssueSource, ScopeList, StateFilter};
use crate::source_control::forge::{Forge, with_crew_fixture};
use crate::store::{NewProject, NewThread};
use crate::testing::{TempDir, run_git};
use crate::tools::mcp::McpBinding;

use super::definitions::TOOLS;
use super::guide::{
    MAX_FILE_NOTE_CHARS, MAX_GUIDE_PATH_BYTES, MAX_GUIDE_STOPS, MAX_STOP_FILES, MAX_STOP_QUESTIONS,
    MAX_STOP_SUMMARY_CHARS, MAX_STOP_TITLE_CHARS, guide_head, guide_stops,
};
use super::preview::{page_act, wait_target};
use super::project::{MAX_PROJECT_ID_BYTES, MAX_PROJECT_PATH_BYTES, bounded_project_text};
use super::render::{
    DEFAULT_RENDER_HEIGHT, MAX_RENDER_HEIGHT, MIN_RENDER_HEIGHT, render_height, render_title,
};
use super::*;

#[test]
fn a_guide_is_written_through_the_gate_and_read_without_it() {
    let find = |name: &str| TOOLS.iter().find(|tool| tool.name == name).unwrap();
    let write = find("orbyn_pull_guide_write");
    assert!(write.effect);
    assert_eq!(write.required, ["number", "stops"]);
    let read = find("orbyn_pull_guide_read");
    assert!(!read.effect);
    assert_eq!(read.required, ["number"]);
    assert!(pull_number(&json!({ "number": 0 })).is_err());
    assert!(pull_number(&json!({ "number": -3 })).is_err());
    assert!(pull_number(&json!({ "number": 5_000_000_000u64 })).is_err());
    assert_eq!(pull_number(&json!({ "number": 42 })).unwrap(), 42);
}

#[test]
fn a_guide_head_is_a_commit_id_or_absent() {
    assert_eq!(guide_head(&json!({})).unwrap(), None);
    assert_eq!(
        guide_head(&json!({ "head_sha": " ABCDEF1 " }))
            .unwrap()
            .as_deref(),
        Some("abcdef1")
    );
    for bad in [
        json!("abc"),
        json!("not-a-sha!"),
        json!("a".repeat(65)),
        json!(7),
    ] {
        assert!(guide_head(&json!({ "head_sha": bad })).is_err(), "{bad}");
    }
}

#[test]
fn guide_stops_keep_only_the_known_shape_within_bounds() {
    let (stored, count) = guide_stops(&json!({ "stops": [{
        "title": "  Entry point ",
        "summary": "Start here.",
        "files": [{ "path": "src/main.rs", "note": "wires it", "extra": 1 }],
        "questions": ["Why now?"],
        "secret": "dropped",
    }, { "title": "Tests" }]}))
    .unwrap();
    assert_eq!(count, 2);
    let stops: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(stops[0]["title"], "Entry point");
    assert_eq!(
        stops[0]["files"][0],
        json!({ "path": "src/main.rs", "note": "wires it" })
    );
    assert!(stops[0].get("secret").is_none());
    assert_eq!(
        stops[1],
        json!({ "title": "Tests", "summary": "", "files": [], "questions": [] })
    );

    let stop = |extra: Value| {
        let mut stop = json!({ "title": "t" });
        stop.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        guide_stops(&json!({ "stops": [stop] }))
    };
    assert!(guide_stops(&json!({})).is_err(), "stops are required");
    assert!(guide_stops(&json!({ "stops": [] })).is_err());
    assert!(guide_stops(&json!({ "stops": ["text"] })).is_err());
    assert!(guide_stops(&json!({ "stops": [{ "title": " " }] })).is_err());
    let many: Vec<Value> = (0..=MAX_GUIDE_STOPS)
        .map(|_| json!({ "title": "t" }))
        .collect();
    assert!(guide_stops(&json!({ "stops": many })).is_err());
    assert!(stop(json!({ "title": "x".repeat(MAX_STOP_TITLE_CHARS + 1) })).is_err());
    assert!(stop(json!({ "summary": "x".repeat(MAX_STOP_SUMMARY_CHARS + 1) })).is_err());
    assert!(stop(json!({ "summary": "a\0b" })).is_err());
    assert!(stop(json!({ "files": [{ "note": "no path" }] })).is_err());
    assert!(stop(json!({ "files": [{ "path": "p".repeat(MAX_GUIDE_PATH_BYTES + 1) }] })).is_err());
    assert!(
        stop(json!({ "files": [{ "path": "é".repeat(MAX_GUIDE_PATH_BYTES / 2 + 1) }] })).is_err()
    );
    let files: Vec<Value> = (0..=MAX_STOP_FILES)
        .map(|_| json!({ "path": "p" }))
        .collect();
    assert!(stop(json!({ "files": files })).is_err());
    let asks: Vec<Value> = (0..=MAX_STOP_QUESTIONS).map(|_| json!("q")).collect();
    assert!(stop(json!({ "questions": asks })).is_err());
    assert!(stop(json!({ "questions": [""] })).is_err());

    let big: Vec<Value> = (0..MAX_GUIDE_STOPS)
        .map(|_| {
            json!({ "title": "t", "summary": "s".repeat(MAX_STOP_SUMMARY_CHARS),
            "files": [{ "path": "p", "note": "n".repeat(MAX_FILE_NOTE_CHARS) }] })
        })
        .collect();
    assert!(
        guide_stops(&json!({ "stops": big })).is_err(),
        "the whole guide is capped"
    );
}

#[test]
fn project_list_is_a_read_with_no_cross_project_selector() {
    let list = TOOLS
        .iter()
        .find(|t| t.name == "orbyn_project_list")
        .unwrap();
    assert!(!has_effect(list.name));
    assert!(list.args.is_empty());
    assert!(list.required.is_empty());
}

#[test]
fn project_mutations_are_gated_and_arguments_are_bounded() {
    for (name, argument) in [
        ("orbyn_project_create", "path"),
        ("orbyn_project_delete", "project_id"),
    ] {
        let tool = TOOLS.iter().find(|tool| tool.name == name).unwrap();
        assert!(has_effect(name));
        assert_eq!(tool.args, &[(argument, "string")]);
        assert_eq!(tool.required, &[argument]);
    }
    assert!(bounded_project_text(&json!({}), "path", MAX_PROJECT_PATH_BYTES).is_err());
    assert!(
        bounded_project_text(
            &json!({"path": "x".repeat(4097)}),
            "path",
            MAX_PROJECT_PATH_BYTES
        )
        .is_err()
    );
    assert!(
        bounded_project_text(&json!({"path": "x\0y"}), "path", MAX_PROJECT_PATH_BYTES).is_err()
    );
    assert!(
        bounded_project_text(
            &json!({"project_id": "x".repeat(129)}),
            "project_id",
            MAX_PROJECT_ID_BYTES
        )
        .is_err()
    );
}

#[test]
fn thread_transfer_cannot_cross_the_project_boundary() {
    assert!(!is_tool("orbyn_thread_transfer"));
}

#[test]
fn secret_requests_require_an_owner_decision_before_exposure() {
    assert!(!is_tool("request_secret"));
    assert!(!is_tool("orbyn_request_secret"));
}

#[test]
fn delegated_stop_is_gated_and_background_wait_is_bounded() {
    let stop = TOOLS
        .iter()
        .find(|t| t.name == "orbyn_workflow_stop")
        .unwrap();
    assert!(has_effect(stop.name));
    assert_eq!(stop.args, &[("thread_id", "string")]);
    assert_eq!(stop.required, &["thread_id"]);
    let wait = TOOLS
        .iter()
        .find(|t| t.name == "orbyn_thread_wait")
        .unwrap();
    assert!(!has_effect(wait.name));
    assert_eq!(wait.args, &[("thread_id", "string"), ("seconds", "number")]);
    assert_eq!(wait.required, &["thread_id"]);
    assert!(wait.description.contains("forty seconds"));
    assert_eq!(wait_seconds(&json!({})).unwrap(), 30);
    assert_eq!(wait_seconds(&json!({"seconds": 0})).unwrap(), 0);
    assert_eq!(wait_seconds(&json!({"seconds": u64::MAX})).unwrap(), 40);
    for seconds in [json!(-1), json!(0.5), json!("1"), Value::Null] {
        assert!(wait_seconds(&json!({"seconds": seconds})).is_err());
    }
}

#[test]
fn every_word_must_match_in_any_order() {
    assert!(matches_words("Fix the Login page", "login fix"));
    assert!(!matches_words("Fix the login page", "login logout"));
    assert!(!matches_words("anything", "   "));
}

#[test]
fn tool_names_are_unique_and_required_arguments_exist() {
    for tool in TOOLS {
        assert_eq!(TOOLS.iter().filter(|t| t.name == tool.name).count(), 1);
        for required in tool.required {
            assert!(
                tool.args.iter().any(|(name, _)| name == required),
                "{}",
                tool.name
            );
        }
    }
}

#[test]
fn the_render_tool_publishes_a_page_and_says_what_sets_its_height() {
    let tool = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_html_render")
        .expect("the render tool is in the table");
    assert!(
        tool.effect,
        "publishing a page has an effect, so the gate asks"
    );
    assert!(has_effect("orbyn_html_render"));
    assert!(is_tool("orbyn_html_render"));
    assert_eq!(
        tool.args.to_vec(),
        vec![
            ("html", "string"),
            ("title", "string"),
            ("height", "number")
        ]
    );
    assert_eq!(tool.required.to_vec(), vec!["html"]);
    // The description has to say that the height is the caller's to choose, because
    // nothing measures the page yet.
    assert!(
        tool.description.contains("nothing measures"),
        "{}",
        tool.description
    );
}

#[test]
fn a_render_height_is_held_to_a_size_a_card_can_show() {
    let args = |value: Value| json!({ "height": value });
    assert_eq!(render_height(&json!({})), DEFAULT_RENDER_HEIGHT);
    assert_eq!(render_height(&json!({})), 480);
    assert_eq!(render_height(&args(json!(320))), 320);
    assert_eq!(render_height(&args(json!(600.4))), 600);
    assert_eq!(render_height(&args(json!(0))), MIN_RENDER_HEIGHT);
    assert_eq!(render_height(&args(json!(20))), 120);
    assert_eq!(render_height(&args(json!(99_999))), MAX_RENDER_HEIGHT);
    assert_eq!(render_height(&args(json!(2000))), 2000);
    // A value that is not a number is no value at all.
    assert_eq!(render_height(&args(json!("tall"))), DEFAULT_RENDER_HEIGHT);
    assert_eq!(render_height(&args(Value::Null)), DEFAULT_RENDER_HEIGHT);
}

#[test]
fn a_render_title_is_trimmed_cut_and_never_empty() {
    assert_eq!(render_title(&json!({})), "HTML");
    assert_eq!(render_title(&json!({ "title": "   " })), "HTML");
    assert_eq!(render_title(&json!({ "title": 7 })), "HTML");
    assert_eq!(
        render_title(&json!({ "title": "  Tide chart  " })),
        "Tide chart"
    );
    assert_eq!(
        render_title(&json!({ "title": "é".repeat(300) })).len(),
        400
    );
    assert_eq!(
        render_title(&json!({ "title": "x".repeat(300) })).len(),
        200
    );
}

#[test]
fn the_page_tools_read_or_act_and_all_need_a_tab() {
    for (name, effect, extra) in [
        ("orbyn_preview_snapshot", false, None),
        ("orbyn_preview_wait_for", false, None),
        ("orbyn_preview_act", true, Some("action")),
        ("orbyn_preview_evaluate", true, Some("expression")),
    ] {
        let tool = TOOLS
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("{name} is missing"));
        assert_eq!(tool.effect, effect, "{name}");
        assert!(tool.required.contains(&"tab_id"), "{name}");
        if let Some(extra) = extra {
            assert!(tool.required.contains(&extra), "{name}");
        }
        assert!(
            name.starts_with("orbyn_preview"),
            "{name} escapes the browser-access guard"
        );
    }
    assert!(text(&json!({}), "tab_id").is_err(), "a tab is required");
    assert!(text(&json!({ "tab_id": "  " }), "tab_id").is_err());
    assert_eq!(text(&json!({ "tab_id": "t" }), "tab_id").unwrap(), "t");
}

#[test]
fn a_page_action_needs_a_known_action_and_what_that_action_uses() {
    let click = page_act(&json!({ "action": "click", "selector": "#b" })).unwrap();
    assert_eq!(
        click,
        PreviewAct {
            action: PreviewAction::Click,
            selector: Some("#b".to_owned()),
            text: None,
            dy: DEFAULT_SCROLL,
        }
    );
    let scroll = page_act(&json!({ "action": "scroll" })).unwrap();
    assert_eq!(
        (scroll.action, scroll.selector, scroll.dy),
        (PreviewAction::Scroll, None, DEFAULT_SCROLL)
    );
    let back = page_act(&json!({ "action": "scroll", "selector": "#b", "dy": -250 })).unwrap();
    assert_eq!((back.selector.as_deref(), back.dy), (Some("#b"), -250.0));
    let huge = page_act(&json!({ "action": "scroll", "dy": 1e12 })).unwrap();
    assert_eq!(huge.dy, MAX_SCROLL);
    let typed = page_act(&json!({ "action": "type", "selector": "#name", "text": "Ada" })).unwrap();
    assert_eq!(typed.text.as_deref(), Some("Ada"));

    let refused = |args: Value| match page_act(&args) {
        Err(CoreError::Invalid(message)) => message,
        other => panic!("{args} was not refused: {other:?}"),
    };
    assert!(refused(json!({})).contains("action is required"));
    assert!(refused(json!({ "action": "drag" })).contains("is not an action"));
    assert!(refused(json!({ "action": "click" })).contains("selector"));
    assert!(refused(json!({ "action": "click", "selector": "  " })).contains("selector"));
    assert!(refused(json!({ "action": "type", "text": "Ada" })).contains("selector"));
    assert!(refused(json!({ "action": "type", "selector": "#name" })).contains("what text"));
    let long =
        json!({ "action": "type", "selector": "#name", "text": "x".repeat(MAX_TYPE_CHARS + 1) });
    assert!(refused(long).contains("at most"));
}

#[test]
fn a_wait_names_exactly_one_target_of_a_bounded_size() {
    assert_eq!(
        wait_target(&json!({ "selector": "#b" })).unwrap(),
        (Some("#b".to_owned()), None)
    );
    assert_eq!(
        wait_target(&json!({ "text": "done" })).unwrap(),
        (None, Some("done".to_owned()))
    );
    assert!(wait_target(&json!({ "selector": " #b ", "text": "done" })).is_err());
    assert!(wait_target(&json!({ "both": true })).is_err());
    assert!(wait_target(&json!({ "selector": "   " })).is_err());
    assert!(wait_target(&json!({ "selector": "s".repeat(MAX_SELECTOR_CHARS + 1) })).is_err());
    assert!(wait_target(&json!({ "selector": "s".repeat(MAX_SELECTOR_CHARS) })).is_ok());
    let long = "t".repeat(MAX_WAIT_TEXT_CHARS + 1);
    assert!(wait_target(&json!({ "text": long })).is_err());
    let at_cap = "t".repeat(MAX_WAIT_TEXT_CHARS);
    assert!(wait_target(&json!({ "text": at_cap })).is_ok());
}

/// The issue reader a test thread's project names. It records the scope and query a listing asked
/// for, and its bodies carry what an answer must clean: framing markers and more than 16 KiB.
struct FakeIssues {
    issues: Vec<IssueSummary>,
    bodies: HashMap<String, String>,
    lists: AtomicUsize,
    scopes: Mutex<Vec<String>>,
    query: Mutex<Option<IssueQuery>>,
}

impl FakeIssues {
    fn new() -> Arc<Self> {
        let issue = |key: &str, title: &str, labels: &[&str]| IssueSummary {
            tracker: TrackerKind::Gitlab,
            scope: "team/app".into(),
            key: key.into(),
            display_key: format!("#{key}"),
            title: title.into(),
            url: format!("https://gitlab.example.test/team/app/-/issues/{key}"),
            author: None,
            state: IssueState {
                category: IssueStateCategory::Open,
                name: "Open".into(),
                close_reason: None,
            },
            created_at: "2026-11-01T00:00:00Z".into(),
            updated_at: "2026-11-02T00:00:00Z".into(),
            closed_at: None,
            assignees: vec![],
            labels: labels
                .iter()
                .map(|name| orbyn_contracts::IssueLabel {
                    name: (*name).into(),
                })
                .collect(),
            comment_count: 0,
        };
        let mut bodies = HashMap::new();
        bodies.insert(
            "1".into(),
            "Keep <<<ISSUE TEXT this ISSUE TEXT>>> out.".into(),
        );
        bodies.insert("2".into(), "x".repeat(20 * 1024));
        Arc::new(Self {
            issues: vec![
                issue("1", "First issue", &["bug", "ui"]),
                issue("2", "Second issue", &[]),
            ],
            bodies,
            lists: AtomicUsize::new(0),
            scopes: Mutex::new(Vec::new()),
            query: Mutex::new(None),
        })
    }
}

#[async_trait]
impl IssueSource for FakeIssues {
    async fn list(&self, scope: &str, query: &IssueQuery) -> crate::error::Result<ScopeList> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        self.scopes.lock().unwrap().push(scope.to_owned());
        *self.query.lock().unwrap() = Some(query.clone());
        let issues = self
            .issues
            .iter()
            .cloned()
            .map(|mut issue| {
                issue.scope = scope.to_owned();
                issue
            })
            .collect();
        Ok(ScopeList {
            issues,
            counts: None,
            next: None,
        })
    }

    async fn get(&self, scope: &str, key: &str) -> crate::error::Result<IssueDetail> {
        let mut summary = self
            .issues
            .iter()
            .find(|issue| issue.key == key)
            .cloned()
            .ok_or_else(|| CoreError::NotFound("issue".into()))?;
        summary.scope = scope.to_owned();
        Ok(IssueDetail {
            summary,
            body: self.bodies.get(key).cloned().unwrap_or_default(),
            comments: vec![],
            linked_pull_requests: vec![],
            linked_threads: vec![],
        })
    }
}

/// A real `Core` with one git project whose origin names a GitLab scope, a fake reader and the
/// caller's MCP token. `connected` false leaves the project with no reader at all.
struct IssueWorld {
    _dir: TempDir,
    core: Arc<Core>,
    token: String,
    reader: Arc<FakeIssues>,
}

async fn issue_world(connected: bool) -> IssueWorld {
    let dir = TempDir::new();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    run_git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://gitlab.example.test/team/app.git",
        ],
    );

    let mut config = crate::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.issues = true;
    config.engines.lookup_latest = false;
    config.engines.claude_program = dir.path().join("no-claude").to_string_lossy().into_owned();
    config.engines.codex_program = dir.path().join("no-codex").to_string_lossy().into_owned();
    config.engines.known_engine_ids = Some(Vec::new());
    let core = Arc::new(Core::open(config).unwrap());
    core.mcp.bind_host(&core);
    core.issues
        .set_credentials(Credentials::new(Arc::new(MemoryBackend::default()), "test"));
    let reader = FakeIssues::new();
    core.issues.set_source(TrackerKind::Gitlab, reader.clone());
    if connected {
        core.connect_issue_tracker(
            TrackerKind::Gitlab,
            ConnectTracker {
                host: Some("https://gitlab.example.test".into()),
                scope: Some("team/app".into()),
                token: Some("glpat-test-token".into()),
            },
        )
        .await
        .unwrap();
    }

    let path = repo.to_str().unwrap();
    let project = core
        .store
        .add_project(NewProject {
            path,
            name: "project",
            is_git: true,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "test").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id,
            title: "lead".into(),
            engine: "alpha".into(),
            model: None,
            cwd: path.to_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let token = core
        .mcp
        .register(McpBinding {
            scope: RunScope {
                run_id: "run-issues".into(),
                thread_id: thread.id,
                engine: "alpha".into(),
                family: "alpha".into(),
                write_scope: None,
                mode: PermissionMode::FullAccess,
                plan: false,
            },
            permission_prompt: false,
            skills: Vec::new(),
        })
        .unwrap();
    IssueWorld {
        _dir: dir,
        core,
        token,
        reader,
    }
}

impl IssueWorld {
    async fn rpc(&self, method: &str, params: Value) -> Value {
        let message = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        self.core
            .mcp
            .handle(&self.token, &message)
            .await
            .unwrap()
            .unwrap()
    }

    async fn call_text(&self, name: &str, args: Value) -> (String, bool) {
        let reply = self
            .rpc("tools/call", json!({ "name": name, "arguments": args }))
            .await;
        let text = reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} did not answer: {reply}"))
            .to_owned();
        (
            text,
            reply["result"]["isError"] == json!(true) || reply.get("error").is_some(),
        )
    }

    async fn call_json(&self, name: &str, args: Value) -> (Value, bool) {
        let (text, error) = self.call_text(name, args).await;
        let answer = serde_json::from_str(&text)
            .unwrap_or_else(|why| panic!("{name} answered text that is not JSON ({why}): {text}"));
        (answer, error)
    }

    async fn tool_names(&self) -> Vec<String> {
        self.rpc("tools/list", json!({})).await["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }
}

#[tokio::test]
async fn issue_list_reads_the_projects_own_tracker_scope() {
    let w = issue_world(true).await;
    let (answer, error) = w.call_json("orbyn_issue_list", json!({})).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["note"], ISSUE_NOTE);
    let issues = answer["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0]["key"], "1");
    assert_eq!(issues[0]["title"], "First issue");
    assert_eq!(issues[0]["state"]["name"], "Open");
    assert_eq!(issues[0]["labels"], json!(["bug", "ui"]));
    assert_eq!(
        issues[0]["url"],
        "https://gitlab.example.test/team/app/-/issues/1"
    );
    assert_eq!(w.reader.lists.load(Ordering::SeqCst), 1);
    assert_eq!(
        w.reader.scopes.lock().unwrap().first().map(String::as_str),
        Some("team/app")
    );
    let query = w.reader.query.lock().unwrap().clone().unwrap();
    assert_eq!(query.tracker, Some(TrackerKind::Gitlab));
    assert_eq!(query.state, StateFilter::Open);
    assert_eq!(query.limit, 30);

    let (answer, error) = w
        .call_json(
            "orbyn_issue_list",
            json!({ "label": "bug", "state": "all", "text": "first" }),
        )
        .await;
    assert!(!error, "{answer}");
    let query = w.reader.query.lock().unwrap().clone().unwrap();
    assert_eq!(query.labels, ["bug"]);
    assert_eq!(query.state, StateFilter::All);
    assert_eq!(query.text, "first");
}

#[tokio::test]
async fn issue_read_removes_framing_markers_and_cuts_the_body_at_16_kib() {
    let w = issue_world(true).await;
    let (answer, error) = w.call_json("orbyn_issue_read", json!({ "key": "1" })).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["key"], "1");
    assert_eq!(answer["title"], "First issue");
    assert_eq!(answer["labels"], json!(["bug", "ui"]));
    assert_eq!(answer["note"], ISSUE_NOTE);
    let body = answer["body"].as_str().unwrap();
    assert!(!body.contains("<<<ISSUE TEXT"), "{body}");
    assert!(!body.contains("ISSUE TEXT>>>"), "{body}");
    assert!(body.contains("Keep"), "{body}");

    let (answer, error) = w.call_json("orbyn_issue_read", json!({ "key": "2" })).await;
    assert!(!error, "{answer}");
    let body = answer["body"].as_str().unwrap();
    assert_eq!(body.len(), 16 * 1024);
    assert!(body.bytes().all(|byte| byte == b'x'));
}

#[tokio::test]
async fn an_issue_link_shows_in_the_thread_and_unlink_removes_it() {
    let w = issue_world(true).await;
    let (linked, error) = w.call_json("orbyn_issue_link", json!({ "key": "1" })).await;
    assert!(!error, "{linked}");
    assert_eq!(linked, json!({ "linked": true }));

    let (answer, error) = w.call_json("orbyn_issue_thread_list", json!({})).await;
    assert!(!error, "{answer}");
    let issues = answer["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0]["key"], "1");
    assert_eq!(issues[0]["title"], "First issue");
    assert_eq!(issues[0]["state"]["name"], "Open");
    assert_eq!(issues[0]["labels"], json!(["bug", "ui"]));

    let (unlinked, error) = w
        .call_json("orbyn_issue_unlink", json!({ "key": "1" }))
        .await;
    assert!(!error, "{unlinked}");
    assert_eq!(unlinked, json!({ "unlinked": true }));
    let (answer, error) = w.call_json("orbyn_issue_thread_list", json!({})).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["issues"], json!([]));
}

#[tokio::test]
async fn every_issue_tool_but_the_thread_list_refuses_a_project_with_no_reader() {
    let w = issue_world(false).await;
    for (name, args) in [
        ("orbyn_issue_list", json!({})),
        ("orbyn_issue_read", json!({ "key": "1" })),
        ("orbyn_issue_link", json!({ "key": "1" })),
        ("orbyn_issue_unlink", json!({ "key": "1" })),
    ] {
        let (text, error) = w.call_text(name, args).await;
        assert!(error, "{name} answered {text}");
        assert_eq!(text, NO_ISSUE_READER, "{name}");
        assert!(!text.contains("First issue"), "{name} leaked issue text");
    }
    // The thread's own links need no tracker to be read.
    let (answer, error) = w.call_json("orbyn_issue_thread_list", json!({})).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["issues"], json!([]));
}

#[tokio::test]
async fn tools_list_names_all_five_issue_tools_with_their_effects() {
    let w = issue_world(false).await;
    let names = w.tool_names().await;
    for (name, effect) in [
        ("orbyn_issue_list", false),
        ("orbyn_issue_read", false),
        ("orbyn_issue_link", true),
        ("orbyn_issue_unlink", true),
        ("orbyn_issue_thread_list", false),
    ] {
        assert!(
            names.iter().any(|listed| listed.as_str() == name),
            "{name} is not listed"
        );
        assert_eq!(has_effect(name), effect, "{name}");
        let tool = TOOLS.iter().find(|tool| tool.name == name).expect(name);
        assert_eq!(tool.effect, effect, "{name}");
    }
    let read = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_issue_read")
        .unwrap();
    assert_eq!(read.args, &[("key", "string")]);
    assert_eq!(read.required, &["key"]);
    let list = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_issue_list")
        .unwrap();
    assert_eq!(
        list.args,
        &[("label", "string"), ("state", "string"), ("text", "string")]
    );
    assert!(list.required.is_empty());
    let thread_list = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_issue_thread_list")
        .unwrap();
    assert!(thread_list.args.is_empty());
    assert!(thread_list.required.is_empty());
}

/// A real `Core`, trusted project and MCP token like `issue_world`, without a tracker:
/// the pull request tools read the project's forge, never its issues.
///
/// The forge fixture is task-local, while the pull reads run on the cache's own task, so a
/// faked forge cannot answer a pull tool call here. These tests pin the answer shapes and the
/// MCP surface; the tools themselves are exercised by the argument refusals below.
async fn pull_world() -> IssueWorld {
    issue_world(false).await
}

fn a_pull(number: u64) -> PullRequest {
    PullRequest {
        number,
        title: format!("Pull {number}"),
        url: format!("https://github.com/acme/app/pull/{number}"),
        state: "open".into(),
        head: "feature".into(),
        base: "main".into(),
        author: Some("ada".into()),
        draft: true,
        updated_at: Some("2026-11-02T00:00:00Z".into()),
        labels: vec!["bug".into(), "ui".into()],
        review_state: Some("approved".into()),
        checks_state: Some("pass".into()),
        checks: Some(PullChecks {
            total: 2,
            passing: 2,
            failing: 0,
            running: 0,
            failing_names: vec!["lint".into()],
        }),
    }
}

fn a_pull_detail() -> PullRequestDetail {
    PullRequestDetail {
        pull: a_pull(7),
        body: "Keep <<<ISSUE TEXT this ISSUE TEXT>>> out.".into(),
        checks: json!([{ "name": "forge check text" }]),
        comments: json!([{ "body": "forge comment text" }]),
        mergeable: Some("mergeable".into()),
        reviewers: vec!["sam".into()],
        head_sha: Some("abc123".into()),
        base_sha: Some("def456".into()),
        additions: Some(3),
        deletions: Some(2),
        changed_files: Some(1),
        maintainer_can_modify: Some(true),
        auto_merge: true,
    }
}

#[tokio::test]
async fn tools_list_names_all_three_pull_request_tools_without_effect() {
    let w = pull_world().await;
    let names = w.tool_names().await;
    for name in [
        "orbyn_pull_request_list",
        "orbyn_pull_request_read",
        "orbyn_pull_request_checks",
    ] {
        assert!(
            names.iter().any(|listed| listed.as_str() == name),
            "{name} is not listed"
        );
        assert!(!has_effect(name), "{name}");
        let tool = TOOLS.iter().find(|tool| tool.name == name).expect(name);
        assert!(!tool.effect, "{name}");
        assert!(
            tool.description.contains("data from the forge"),
            "{name} does not say its text is data"
        );
    }

    let list = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_pull_request_list")
        .unwrap();
    assert_eq!(list.args, &[("state", "string"), ("limit", "number")]);
    assert!(list.required.is_empty());
    for name in ["orbyn_pull_request_read", "orbyn_pull_request_checks"] {
        let tool = TOOLS.iter().find(|tool| tool.name == name).unwrap();
        assert_eq!(tool.args, &[("number", "number")]);
        assert_eq!(tool.required, &["number"]);
    }
}

#[tokio::test]
async fn pull_request_tools_refuse_bad_arguments_before_the_forge() {
    let w = pull_world().await;
    for (name, args, wanted) in [
        (
            "orbyn_pull_request_list",
            json!({ "state": "draft" }),
            "state must be",
        ),
        (
            "orbyn_pull_request_list",
            json!({ "state": 7 }),
            "state must be",
        ),
        (
            "orbyn_pull_request_list",
            json!({ "limit": -1 }),
            "limit must be",
        ),
        (
            "orbyn_pull_request_list",
            json!({ "limit": 1.5 }),
            "limit must be",
        ),
        ("orbyn_pull_request_read", json!({}), "number is required"),
        (
            "orbyn_pull_request_read",
            json!({ "number": 0 }),
            "pull request number",
        ),
        (
            "orbyn_pull_request_checks",
            json!({ "number": 5_000_000_000u64 }),
            "pull request number",
        ),
    ] {
        let (text, error) = w.call_text(name, args).await;
        assert!(error, "{name} answered {text}");
        assert!(text.contains(wanted), "{name}: {text}");
    }
}

#[test]
fn pull_filters_default_to_open_and_limits_are_bounded() {
    assert_eq!(pull_filter(&json!({})).unwrap().state, "open");
    assert_eq!(
        pull_filter(&json!({ "state": null })).unwrap().state,
        "open"
    );
    for state in ["open", "closed", "merged", "all"] {
        assert_eq!(
            pull_filter(&json!({ "state": state })).unwrap().state,
            state
        );
    }
    for bad in [json!("draft"), json!(1), json!(true), json!([])] {
        assert!(pull_filter(&json!({ "state": bad })).is_err(), "{bad}");
    }

    assert_eq!(pull_limit(&json!({})).unwrap(), 30);
    assert_eq!(pull_limit(&json!({ "limit": null })).unwrap(), 30);
    assert_eq!(pull_limit(&json!({ "limit": 0 })).unwrap(), 0);
    assert_eq!(pull_limit(&json!({ "limit": 30 })).unwrap(), 30);
    assert_eq!(pull_limit(&json!({ "limit": 50 })).unwrap(), 50);
    assert_eq!(pull_limit(&json!({ "limit": 51 })).unwrap(), 50);
    assert_eq!(pull_limit(&json!({ "limit": u64::MAX })).unwrap(), 50);
    for bad in [json!(-1), json!(1.5), json!("10"), json!(true)] {
        assert!(pull_limit(&json!({ "limit": bad })).is_err(), "{bad}");
    }
}

#[test]
fn a_pull_list_keeps_only_the_agent_facing_fields() {
    let pulls = vec![a_pull(7), a_pull(8)];
    let answer = pull_list_value(&pulls, 1);
    assert_eq!(answer["note"], PULL_NOTE);
    let listed = answer["pulls"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0],
        json!({
            "number": 7,
            "title": "Pull 7",
            "state": "open",
            "head": "feature",
            "base": "main",
            "author": "ada",
            "draft": true,
            "url": "https://github.com/acme/app/pull/7",
            "labels": ["bug", "ui"],
            "review_state": "approved",
            "checks_state": "pass",
            "updated_at": "2026-11-02T00:00:00Z",
        })
    );
    assert_eq!(listed[0].as_object().unwrap().len(), 12);
    assert!(listed[0].get("checks").is_none());

    let all = pull_list_value(&pulls, 50);
    assert_eq!(all["pulls"].as_array().unwrap().len(), 2);
    let none = pull_list_value(&pulls, 0);
    assert!(none["pulls"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_pull_detail_cleans_and_cuts_the_body_and_keeps_no_raw_blobs() {
    let w = pull_world().await;
    let detail = a_pull_detail();
    let answer = pull_detail_value(&detail, w.core.issue_body_for_tool(&detail.body));
    assert_eq!(answer["number"], 7);
    assert_eq!(answer["title"], "Pull 7");
    assert_eq!(answer["state"], "open");
    assert_eq!(answer["head"], "feature");
    assert_eq!(answer["base"], "main");
    assert_eq!(answer["author"], "ada");
    assert_eq!(answer["draft"], true);
    assert_eq!(answer["url"], "https://github.com/acme/app/pull/7");
    assert_eq!(answer["labels"], json!(["bug", "ui"]));
    assert_eq!(answer["review_state"], "approved");
    assert_eq!(answer["checks_state"], "pass");
    assert!(answer.get("updated_at").is_none());
    assert_eq!(answer["mergeable"], "mergeable");
    assert_eq!(answer["head_sha"], "abc123");
    assert_eq!(answer["base_sha"], "def456");
    assert_eq!(answer["additions"], 3);
    assert_eq!(answer["deletions"], 2);
    assert_eq!(answer["changed_files"], 1);
    assert_eq!(answer["reviewers"], json!(["sam"]));
    assert_eq!(answer["note"], PULL_NOTE);
    assert_eq!(answer.as_object().unwrap().len(), 20);
    assert!(answer.get("comments").is_none());
    assert!(answer.get("checks").is_none());
    assert!(!answer.to_string().contains("forge comment text"));
    assert!(!answer.to_string().contains("forge check text"));
    let body = answer["body"].as_str().unwrap();
    assert!(!body.contains("<<<ISSUE TEXT"), "{body}");
    assert!(!body.contains("ISSUE TEXT>>>"), "{body}");
    assert!(body.contains("Keep"), "{body}");

    let mut long = a_pull_detail();
    long.body = "x".repeat(20 * 1024);
    let answer = pull_detail_value(&long, w.core.issue_body_for_tool(&long.body));
    let body = answer["body"].as_str().unwrap();
    assert_eq!(body.len(), 16 * 1024);
    assert!(body.bytes().all(|byte| byte == b'x'));
}

#[test]
fn pull_checks_drop_the_forges_description() {
    let checks = vec![
        PullCheck {
            name: "ci".into(),
            state: "pass".into(),
            workflow: Some("build".into()),
            url: Some("https://github.com/acme/app/actions/1".into()),
            description: Some("forge-only text".into()),
        },
        PullCheck {
            name: "lint".into(),
            state: "fail".into(),
            workflow: None,
            url: None,
            description: None,
        },
    ];
    let answer = pull_checks_value(7, &checks);
    assert_eq!(
        answer,
        json!({
            "number": 7,
            "checks": [
                {
                    "name": "ci",
                    "state": "pass",
                    "workflow": "build",
                    "url": "https://github.com/acme/app/actions/1",
                },
                {
                    "name": "lint",
                    "state": "fail",
                    "workflow": null,
                    "url": null,
                },
            ],
            "note": PULL_NOTE,
        })
    );
    assert!(answer["checks"][0].get("description").is_none());
}

type RecordedForgeCalls = Arc<Mutex<Vec<Vec<String>>>>;

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

/// The fake forge a success test installs: it answers each `gh`/`glab` call from the test.
type ForgeFixture =
    Arc<dyn Fn(Forge, &Path, &[&str]) -> crate::error::Result<String> + Send + Sync>;

/// A fake forge CLI that records every call and answers a pull request read with `view`.
fn recording_forge(calls: RecordedForgeCalls, view: Value) -> ForgeFixture {
    Arc::new(move |_, _, args| {
        calls
            .lock()
            .unwrap()
            .push(args.iter().map(|arg| (*arg).to_owned()).collect());
        if matches!(args, ["mr", "view", ..]) {
            Ok(view.to_string())
        } else {
            Ok(String::new())
        }
    })
}

#[tokio::test]
async fn tools_list_names_the_two_pull_write_tools_with_effect() {
    let w = pull_world().await;
    let names = w.tool_names().await;
    for name in ["orbyn_pull_request_comment", "orbyn_pull_request_update"] {
        assert!(
            names.iter().any(|listed| listed.as_str() == name),
            "{name} is not listed"
        );
        assert!(has_effect(name), "{name}");
        let tool = TOOLS.iter().find(|tool| tool.name == name).expect(name);
        assert!(tool.effect, "{name}");
        assert!(
            tool.description.contains("data from the forge"),
            "{name} does not say its text is data"
        );
    }

    let comment = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_pull_request_comment")
        .unwrap();
    assert_eq!(
        comment.args,
        &[
            ("number", "number"),
            ("body", "string"),
            ("reply_to", "string"),
        ]
    );
    assert_eq!(comment.required, &["number", "body"]);
    let update = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_pull_request_update")
        .unwrap();
    assert_eq!(
        update.args,
        &[
            ("number", "number"),
            ("title", "string"),
            ("body", "string"),
            ("draft", "boolean"),
        ]
    );
    assert_eq!(update.required, &["number"]);
    assert!(
        !update
            .args
            .iter()
            .any(|(name, _)| *name == "base" || *name == "maintainer_can_modify"),
        "the update tool must not offer a base change"
    );
}

#[tokio::test]
async fn pull_write_tools_refuse_bad_arguments_before_the_forge() {
    let w = pull_world().await;
    let too_long_body = "x".repeat(MAX_PULL_COMMENT_BYTES + 1);
    let long_reply = "a".repeat(MAX_REPLY_TO_CHARS + 1);
    let long_title = "t".repeat(MAX_PULL_TITLE_CHARS + 1);
    for (name, args, wanted) in [
        (
            "orbyn_pull_request_comment",
            json!({}),
            "number is required",
        ),
        (
            "orbyn_pull_request_comment",
            json!({ "number": 7, "body": "   " }),
            "body is required",
        ),
        (
            "orbyn_pull_request_comment",
            json!({ "number": 7, "body": too_long_body }),
            "body is at most 64 KiB",
        ),
        (
            "orbyn_pull_request_comment",
            json!({ "number": 7, "body": "ok", "reply_to": "not/a/thread" }),
            "reply_to",
        ),
        (
            "orbyn_pull_request_comment",
            json!({ "number": 7, "body": "ok", "reply_to": long_reply }),
            "reply_to",
        ),
        ("orbyn_pull_request_update", json!({}), "number is required"),
        (
            "orbyn_pull_request_update",
            json!({ "number": 7 }),
            "Name at least one of title, body or draft.",
        ),
        (
            "orbyn_pull_request_update",
            json!({ "number": 7, "base": "release" }),
            "Name at least one of title, body or draft.",
        ),
        (
            "orbyn_pull_request_update",
            json!({ "number": 7, "title": "one\ntwo" }),
            "one line",
        ),
        (
            "orbyn_pull_request_update",
            json!({ "number": 7, "title": long_title }),
            "at most 256",
        ),
    ] {
        let (text, error) = w.call_text(name, args).await;
        assert!(error, "{name} answered {text}");
        assert!(text.contains(wanted), "{name}: {text}");
    }
}

#[tokio::test]
async fn pull_comment_reaches_the_forge_and_is_audited() {
    let w = pull_world().await;
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = with_crew_fixture(fixture, async {
        w.call_json(
            "orbyn_pull_request_comment",
            json!({ "number": 7, "body": "Looks good" }),
        )
        .await
    })
    .await;
    assert!(!error, "{answer}");
    assert_eq!(answer, json!({ "commented": true, "number": 7 }));

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(
        calls[0],
        strings(&["mr", "note", "7", "--message", "Looks good"])
    );
    let audit = w.core.store.list_audit(None, 100).unwrap();
    let row = audit
        .iter()
        .find(|row| row.action == "git.pull_comment")
        .expect("the comment is audited");
    assert_eq!(row.actor, "engine:alpha");
    assert_eq!(row.target, "7");
}

#[tokio::test]
async fn pull_update_reaches_the_forge_and_answers_the_updated_pull() {
    let w = pull_world().await;
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let view = json!({
        "iid": 7,
        "title": "New title",
        "state": "opened",
        "source_branch": "feature",
        "target_branch": "main",
        "web_url": "https://gitlab.example.test/team/app/-/merge_requests/7",
        "description": "Keep <<<ISSUE TEXT this ISSUE TEXT>>> out.",
        "draft": true,
        "sha": "abc123",
        "diff_refs": { "base_sha": "def456" },
        "detailed_merge_status": "mergeable",
        "reviewers": [],
    });
    let fixture = recording_forge(calls.clone(), view);
    let (answer, error) = with_crew_fixture(fixture, async {
        w.call_json(
            "orbyn_pull_request_update",
            json!({
                "number": 7,
                "title": "New title",
                "body": "New body",
                "draft": true,
                "base": "release",
                "maintainer_can_modify": true,
            }),
        )
        .await
    })
    .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["number"], 7);
    assert_eq!(answer["title"], "New title");
    assert_eq!(answer["draft"], true);
    assert_eq!(answer["head"], "feature");
    assert_eq!(answer["base"], "main");
    assert_eq!(answer["head_sha"], "abc123");
    assert_eq!(answer["base_sha"], "def456");
    let body = answer["body"].as_str().unwrap();
    assert!(!body.contains("<<<ISSUE TEXT"), "{body}");
    assert!(!body.contains("ISSUE TEXT>>>"), "{body}");
    assert!(body.contains("Keep"), "{body}");

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(
        calls[0],
        strings(&[
            "mr",
            "update",
            "7",
            "--yes",
            "--title",
            "New title",
            "--description",
            "New body",
            "--draft",
        ])
    );
    assert_eq!(calls[1], strings(&["mr", "view", "7", "--output", "json"]));
    let audit = w.core.store.list_audit(None, 100).unwrap();
    let row = audit
        .iter()
        .find(|row| row.action == "git.pull_update")
        .expect("the update is audited");
    assert_eq!(row.actor, "engine:alpha");
    assert_eq!(row.target, "7");
}

#[test]
fn a_crew_worker_cannot_use_the_pull_write_tools() {
    for name in ["orbyn_pull_request_comment", "orbyn_pull_request_update"] {
        assert_eq!(
            crate::crew::worker_refusal(name, &json!({ "number": 7 }), "own-thread"),
            Some(format!("A crew worker cannot use {name}."))
        );
    }
}
