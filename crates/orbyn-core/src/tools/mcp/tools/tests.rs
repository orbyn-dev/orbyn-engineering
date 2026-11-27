use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::Engine as _;
use orbyn_contracts::{
    ActionKind, AuditEvent, ConnectTracker, CreatePolicyRule, IssueActor, IssueCloseReason,
    IssueComment, IssueDetail, IssueState, IssueStateCategory, IssueSummary, PermissionMode,
    PolicyEffect, PullCheck, PullChecks, PullRequest, PullRequestDetail, TrackerKind,
};
use serde_json::{Value, json};

use crate::app::Core;
use crate::browser::preview::{
    DEFAULT_SCROLL, MAX_SCROLL, MAX_SELECTOR_CHARS, MAX_TYPE_CHARS, MAX_WAIT_TEXT_CHARS,
    PreviewAct, PreviewAction,
};
use crate::credentials::{Credentials, MemoryBackend};
use crate::devices::test_support::{Fake, fake_png};
use crate::error::CoreError;
use crate::governance::approvals::RunScope;
use crate::issues::{IssueQuery, IssueSource, NewIssue, ScopeList, StateFilter};
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

/// The exact refusal a parser answers: `Invalid` with the sentence the tool shows its caller.
fn invalid<T>(result: crate::error::Result<T>) -> String {
    match result {
        Err(CoreError::Invalid(message)) => message,
        Ok(_) => panic!("expected a refusal"),
        Err(other) => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn a_guide_is_written_through_the_gate_and_read_without_it() {
    let find = |name: &str| TOOLS.iter().find(|tool| tool.name == name).unwrap();
    let write = find("orbyn_pull_guide_write");
    assert!(write.effect);
    assert_eq!(write.required, ["number", "stops"]);
    let read = find("orbyn_pull_guide_read");
    assert!(!read.effect);
    assert_eq!(read.required, ["number"]);
    assert_eq!(
        invalid(pull_number(&json!({ "number": 0 }))),
        "number must be a pull request number."
    );
    assert_eq!(
        invalid(pull_number(&json!({ "number": -3 }))),
        "number is required."
    );
    assert_eq!(
        invalid(pull_number(&json!({ "number": 5_000_000_000u64 }))),
        "number must be a pull request number."
    );
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
        assert_eq!(
            invalid(guide_head(&json!({ "head_sha": bad }))),
            "head_sha is a commit id of 7 to 64 hexadecimal characters.",
            "{bad}"
        );
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
    assert_eq!(
        invalid(guide_stops(&json!({}))),
        "stops must list at least one stop.",
        "stops are required"
    );
    assert_eq!(
        invalid(guide_stops(&json!({ "stops": [] }))),
        "stops must list at least one stop."
    );
    assert_eq!(
        invalid(guide_stops(&json!({ "stops": ["text"] }))),
        "Each stop must be an object."
    );
    assert_eq!(
        invalid(guide_stops(&json!({ "stops": [{ "title": " " }] }))),
        "A stop's title is required."
    );
    let many: Vec<Value> = (0..=MAX_GUIDE_STOPS)
        .map(|_| json!({ "title": "t" }))
        .collect();
    assert_eq!(
        invalid(guide_stops(&json!({ "stops": many }))),
        "A guide holds at most 30 stops."
    );
    assert_eq!(
        invalid(stop(
            json!({ "title": "x".repeat(MAX_STOP_TITLE_CHARS + 1) })
        )),
        "A stop's title is at most 200 characters and cannot contain NUL."
    );
    assert_eq!(
        invalid(stop(
            json!({ "summary": "x".repeat(MAX_STOP_SUMMARY_CHARS + 1) })
        )),
        "A stop's summary is at most 4000 characters and cannot contain NUL."
    );
    assert_eq!(
        invalid(stop(json!({ "summary": "a\0b" }))),
        "A stop's summary is at most 4000 characters and cannot contain NUL."
    );
    assert_eq!(
        invalid(stop(json!({ "files": [{ "note": "no path" }] }))),
        "A file's path must be text."
    );
    assert_eq!(
        invalid(stop(
            json!({ "files": [{ "path": "p".repeat(MAX_GUIDE_PATH_BYTES + 1) }] })
        )),
        "A file's path is at most 1024 characters and cannot contain NUL."
    );
    assert_eq!(
        invalid(stop(
            json!({ "files": [{ "path": "é".repeat(MAX_GUIDE_PATH_BYTES / 2 + 1) }] })
        )),
        "A file's path is at most 1024 bytes."
    );
    let files: Vec<Value> = (0..=MAX_STOP_FILES)
        .map(|_| json!({ "path": "p" }))
        .collect();
    assert_eq!(
        invalid(stop(json!({ "files": files }))),
        "files holds at most 50."
    );
    let asks: Vec<Value> = (0..=MAX_STOP_QUESTIONS).map(|_| json!("q")).collect();
    assert_eq!(
        invalid(stop(json!({ "questions": asks }))),
        "questions holds at most 10."
    );
    assert_eq!(
        invalid(stop(json!({ "questions": [""] }))),
        "A question is required."
    );

    let big: Vec<Value> = (0..MAX_GUIDE_STOPS)
        .map(|_| {
            json!({ "title": "t", "summary": "s".repeat(MAX_STOP_SUMMARY_CHARS),
            "files": [{ "path": "p", "note": "n".repeat(MAX_FILE_NOTE_CHARS) }] })
        })
        .collect();
    assert_eq!(
        invalid(guide_stops(&json!({ "stops": big }))),
        "A guide is at most 131072 bytes.",
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
    assert_eq!(
        invalid(bounded_project_text(
            &json!({}),
            "path",
            MAX_PROJECT_PATH_BYTES
        )),
        "path is required."
    );
    assert_eq!(
        invalid(bounded_project_text(
            &json!({"path": "x".repeat(4097)}),
            "path",
            MAX_PROJECT_PATH_BYTES
        )),
        "path is at most 4096 bytes and cannot contain NUL."
    );
    assert_eq!(
        invalid(bounded_project_text(
            &json!({"path": "x\0y"}),
            "path",
            MAX_PROJECT_PATH_BYTES
        )),
        "path is at most 4096 bytes and cannot contain NUL."
    );
    assert_eq!(
        invalid(bounded_project_text(
            &json!({"project_id": "x".repeat(129)}),
            "project_id",
            MAX_PROJECT_ID_BYTES
        )),
        "project_id is at most 128 bytes and cannot contain NUL."
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
        assert_eq!(
            invalid(wait_seconds(&json!({"seconds": seconds}))),
            "seconds must be a non-negative integer.",
            "{seconds}"
        );
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
    assert_eq!(
        invalid(text(&json!({}), "tab_id")),
        "tab_id is required.",
        "a tab is required"
    );
    assert_eq!(
        invalid(text(&json!({ "tab_id": "  " }), "tab_id")),
        "tab_id is required."
    );
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
    assert_eq!(
        invalid(wait_target(&json!({ "selector": " #b ", "text": "done" }))),
        "Name a selector or text to wait for, not both."
    );
    assert_eq!(
        invalid(wait_target(&json!({ "both": true }))),
        "Name a selector or text to wait for."
    );
    assert_eq!(
        invalid(wait_target(&json!({ "selector": "   " }))),
        "Name a selector or text to wait for."
    );
    assert_eq!(
        invalid(wait_target(
            &json!({ "selector": "s".repeat(MAX_SELECTOR_CHARS + 1) })
        )),
        "A selector is at most 300 characters."
    );
    assert!(wait_target(&json!({ "selector": "s".repeat(MAX_SELECTOR_CHARS) })).is_ok());
    let long = "t".repeat(MAX_WAIT_TEXT_CHARS + 1);
    assert_eq!(
        invalid(wait_target(&json!({ "text": long }))),
        "Text to wait for is at most 200 characters."
    );
    let at_cap = "t".repeat(MAX_WAIT_TEXT_CHARS);
    assert!(wait_target(&json!({ "text": at_cap })).is_ok());
}

/// The issue reader a test thread's project names. It records the scope and query a listing asked
/// for, and its bodies carry what an answer must clean: framing markers and more than 16 KiB.
struct FakeIssues {
    issues: Vec<IssueSummary>,
    bodies: HashMap<String, String>,
    lists: AtomicUsize,
    /// Every detail read the tool asked for, so a refusal can prove nothing was read either.
    reads: AtomicUsize,
    /// Whether this tracker's edit is built. When it is, its issues offer the three state names
    /// GitHub offers, so a state move travels the same path a GitHub one does.
    editable: bool,
    scopes: Mutex<Vec<String>>,
    query: Mutex<Option<IssueQuery>>,
    /// Every write the tool sent, exactly as the tracker received it.
    writes: Mutex<Vec<Value>>,
}

impl FakeIssues {
    fn new() -> Arc<Self> {
        Self::build(false)
    }

    /// The same reader with its edit built, for the tools that write an issue.
    fn editing() -> Arc<Self> {
        Self::build(true)
    }

    /// The states a reader whose edit is built offers; the same names and reasons GitHub offers.
    fn states(&self) -> Option<Vec<IssueState>> {
        self.editable.then(|| {
            vec![
                IssueState {
                    category: IssueStateCategory::Open,
                    name: "Open".into(),
                    close_reason: None,
                },
                IssueState {
                    category: IssueStateCategory::Closed,
                    name: "Closed".into(),
                    close_reason: Some(IssueCloseReason::Completed),
                },
                IssueState {
                    category: IssueStateCategory::Closed,
                    name: "Closed as not planned".into(),
                    close_reason: Some(IssueCloseReason::NotPlanned),
                },
            ]
        })
    }

    fn build(editable: bool) -> Arc<Self> {
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
            reads: AtomicUsize::new(0),
            editable,
            scopes: Mutex::new(Vec::new()),
            query: Mutex::new(None),
            writes: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait]
impl IssueSource for FakeIssues {
    fn can_write(&self) -> bool {
        true
    }

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
        self.reads.fetch_add(1, Ordering::SeqCst);
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
            activity: vec![],
            reactions: None,
            states: self.states(),
        })
    }

    fn can_edit(&self) -> bool {
        self.editable
    }

    /// Moves the named issue the way a tracker with its edit built does, and answers the issue as
    /// the tracker stored it, so the tool's answer keeps the detail read's shape.
    async fn set_state(
        &self,
        scope: &str,
        key: &str,
        state: &str,
        close_reason: Option<IssueCloseReason>,
    ) -> crate::error::Result<IssueDetail> {
        self.writes.lock().unwrap().push(json!({
            "kind": "set_state",
            "scope": scope,
            "key": key,
            "state": state,
            "close_reason": close_reason,
        }));
        let mut detail = self.get(scope, key).await?;
        detail.summary.state = detail
            .states
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|offered| offered.name == state)
            .cloned()
            .ok_or_else(|| CoreError::Invalid("That state is not one this issue offers.".into()))?;
        Ok(detail)
    }

    async fn create(&self, scope: &str, new: &NewIssue<'_>) -> crate::error::Result<IssueDetail> {
        self.writes.lock().unwrap().push(json!({
            "kind": "create",
            "scope": scope,
            "title": new.title,
            "body": new.body,
            "labels": new.labels,
            "assignees": new.assignees,
        }));
        let mut summary = self.issues[0].clone();
        summary.key = "9".into();
        summary.display_key = "#9".into();
        summary.title = new.title.to_owned();
        summary.scope = scope.to_owned();
        summary.comment_count = 0;
        Ok(IssueDetail {
            summary,
            body: new.body.to_owned(),
            comments: vec![],
            linked_pull_requests: vec![],
            linked_threads: vec![],
            activity: vec![],
            reactions: None,
            states: None,
        })
    }

    async fn comment(
        &self,
        scope: &str,
        key: &str,
        body: &str,
    ) -> crate::error::Result<IssueComment> {
        self.writes.lock().unwrap().push(json!({
            "kind": "comment",
            "scope": scope,
            "key": key,
            "body": body,
        }));
        if key == "404" {
            return Err(CoreError::NotFound("issue".into()));
        }
        // The answer also proves the tool cleans and cuts tracker text: markers and 20 KiB.
        Ok(IssueComment {
            id: "900".into(),
            author: Some(IssueActor { login: "me".into() }),
            body: format!(
                "<<<ISSUE TEXT{}ISSUE TEXT>>>{}",
                body,
                "x".repeat(20 * 1024)
            ),
            created_at: "2026-11-25T00:00:00Z".into(),
        })
    }
}

/// A real `Core` with one git project whose origin names the forge's scope, a fake reader and
/// the caller's MCP token. `connected` false leaves the project with no reader at all.
struct IssueWorld {
    _dir: TempDir,
    core: Arc<Core>,
    token: String,
    thread: String,
    reader: Arc<FakeIssues>,
}

async fn issue_world(connected: bool) -> IssueWorld {
    issue_world_reader(
        connected,
        "https://gitlab.example.test/team/app.git",
        FakeIssues::new(),
    )
    .await
}

/// The same world whose tracker also edits: its issues offer the three states GitHub names, so a
/// state move travels the path a GitHub one does.
async fn editable_issue_world() -> IssueWorld {
    issue_world_reader(
        true,
        "https://gitlab.example.test/team/app.git",
        FakeIssues::editing(),
    )
    .await
}

/// The same world over a GitHub origin, for the CI tools that read `gh` runs.
async fn github_pull_world() -> IssueWorld {
    issue_world_at(false, "https://github.com/acme/app.git").await
}

async fn issue_world_at(connected: bool, origin: &str) -> IssueWorld {
    issue_world_reader(connected, origin, FakeIssues::new()).await
}

async fn issue_world_reader(connected: bool, origin: &str, reader: Arc<FakeIssues>) -> IssueWorld {
    let dir = TempDir::new();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    run_git(&repo, &["remote", "add", "origin", origin]);

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
                thread_id: thread.id.clone(),
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
        thread: thread.id,
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
        (
            "orbyn_issue_create",
            json!({ "title": "New issue", "body": "Body" }),
        ),
        (
            "orbyn_issue_comment",
            json!({ "key": "1", "body": "Looks good" }),
        ),
        (
            "orbyn_issue_set_state",
            json!({ "key": "1", "state": "Open" }),
        ),
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
async fn an_issue_create_answers_the_new_issue_and_sends_the_contract_fields() {
    let w = issue_world(true).await;
    let (answer, error) = w
        .call_json(
            "orbyn_issue_create",
            json!({
                "title": "New issue",
                "body": "It breaks.",
                "labels": ["bug"],
                "assignees": ["12"],
            }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["key"], "9");
    assert_eq!(answer["title"], "New issue");
    assert_eq!(answer["body"], "It breaks.");
    assert_eq!(answer["note"], ISSUE_NOTE);
    assert_eq!(
        w.reader.writes.lock().unwrap().clone(),
        vec![json!({
            "kind": "create",
            "scope": "team/app",
            "title": "New issue",
            "body": "It breaks.",
            "labels": ["bug"],
            "assignees": ["12"],
        })],
        "the tool sends the contract's fields in the project's own scope"
    );
}

#[tokio::test]
async fn an_issue_comment_answers_the_stored_comment_and_cleans_the_tracker_text() {
    let w = issue_world(true).await;
    let (answer, error) = w
        .call_json(
            "orbyn_issue_comment",
            json!({ "key": "1", "body": "Looks good." }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["id"], "900");
    assert_eq!(answer["author"]["login"], "me");
    assert_eq!(answer["created_at"], "2026-11-25T00:00:00Z");
    assert_eq!(answer["note"], ISSUE_NOTE);
    let body = answer["body"].as_str().unwrap();
    assert!(body.starts_with("Looks good."), "{body}");
    assert!(
        !body.contains("ISSUE TEXT"),
        "the framing markers are removed"
    );
    assert_eq!(body.len(), 16 * 1024, "the answer cuts the tracker's text");
    assert_eq!(
        w.reader.writes.lock().unwrap().clone(),
        vec![json!({
            "kind": "comment",
            "scope": "team/app",
            "key": "1",
            "body": "Looks good.",
        })]
    );
}

#[tokio::test]
async fn an_issue_create_refuses_fields_the_contract_refuses() {
    let w = issue_world(true).await;
    for (args, wanted) in [
        (json!({}), "title is required."),
        (
            json!({ "title": "T", "body": "x".repeat(64 * 1024 + 1) }),
            "The issue body is over 64 KiB.",
        ),
        (
            json!({ "title": "T", "labels": "bug" }),
            "labels must be a list of strings.",
        ),
        (
            json!({ "title": "T", "assignees": [12] }),
            "assignees must be a list of strings.",
        ),
        (
            json!({ "title": "T", "template": "missing" }),
            "Unknown issue template.",
        ),
    ] {
        let (text, error) = w.call_text("orbyn_issue_create", args.clone()).await;
        assert!(error, "{args} was not refused: {text}");
        assert!(text.contains(wanted), "{args}: {text}");
    }
    assert!(
        w.reader.writes.lock().unwrap().is_empty(),
        "a refused call sends no write"
    );
}

#[tokio::test]
async fn an_issue_comment_refuses_a_blank_body_and_an_unknown_key() {
    let w = issue_world(true).await;
    let (text, error) = w
        .call_text("orbyn_issue_comment", json!({ "key": "1", "body": "   " }))
        .await;
    assert!(error, "{text}");
    assert_eq!(text, "body is required.");
    // The tracker's refusal keeps its contract wording; the tool adds nothing.
    let (text, error) = w
        .call_text(
            "orbyn_issue_comment",
            json!({ "key": "404", "body": "Looks good." }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(text, "issue not found");
    assert_eq!(
        w.reader.writes.lock().unwrap().len(),
        1,
        "the unknown-key attempt reached the tracker once and the blank body never"
    );
}

#[tokio::test]
async fn an_issue_state_move_answers_the_issue_and_sends_the_contract_fields() {
    let w = editable_issue_world().await;
    let (answer, error) = w
        .call_json(
            "orbyn_issue_set_state",
            json!({ "key": "1", "state": "Closed as not planned", "close_reason": "not_planned" }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["key"], "1");
    assert_eq!(answer["state"]["name"], "Closed as not planned");
    assert_eq!(answer["state"]["category"], "closed");
    assert_eq!(answer["state"]["close_reason"], "not_planned");
    assert_eq!(answer["note"], ISSUE_NOTE);
    let body = answer["body"].as_str().unwrap();
    assert!(body.contains("Keep"), "{body}");
    assert!(!body.contains("ISSUE TEXT"), "{body}");
    assert_eq!(
        w.reader.writes.lock().unwrap().clone(),
        vec![json!({
            "kind": "set_state",
            "scope": "team/app",
            "key": "1",
            "state": "Closed as not planned",
            "close_reason": "not_planned",
        })],
        "the tool sends the contract's fields in the project's own scope"
    );
}

#[tokio::test]
async fn an_issue_state_move_defaults_the_reason_and_reopens_without_one() {
    let w = editable_issue_world().await;
    // A closed state with no reason is completed: the Core's contract default, not the tool's guess.
    let (answer, error) = w
        .call_json(
            "orbyn_issue_set_state",
            json!({ "key": "1", "state": "Closed" }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"]["name"], "Closed");
    assert_eq!(answer["state"]["close_reason"], "completed");
    let (answer, error) = w
        .call_json(
            "orbyn_issue_set_state",
            json!({ "key": "1", "state": "Open" }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"]["name"], "Open");
    assert_eq!(
        w.reader.writes.lock().unwrap().clone(),
        vec![
            json!({
                "kind": "set_state",
                "scope": "team/app",
                "key": "1",
                "state": "Closed",
                "close_reason": "completed",
            }),
            json!({
                "kind": "set_state",
                "scope": "team/app",
                "key": "1",
                "state": "Open",
                "close_reason": null,
            }),
        ],
        "the reason the contract defaults to is the one the tracker receives"
    );
}

#[tokio::test]
async fn an_issue_state_move_refuses_arguments_the_tool_cannot_take() {
    let w = editable_issue_world().await;
    for (args, wanted) in [
        (json!({}), "key is required."),
        (json!({ "key": 1, "state": "Open" }), "key is required."),
        (json!({ "key": "1" }), "state is required."),
        (json!({ "key": "1", "state": " " }), "state is required."),
        (
            json!({ "key": "1", "state": "Open", "close_reason": 7 }),
            "close_reason must be a string.",
        ),
        (
            json!({ "key": "1", "state": "Open", "close_reason": "done" }),
            "close_reason must be completed or not_planned.",
        ),
        (
            // The route's serde enum reads the reason exactly, so a padded value is refused there
            // and here alike.
            json!({ "key": "1", "state": "Closed", "close_reason": " not_planned " }),
            "close_reason must be completed or not_planned.",
        ),
        (
            json!({ "key": "1", "state": "Done" }),
            "That state is not one this issue offers.",
        ),
        (
            json!({ "key": "1", "state": "Open", "close_reason": "completed" }),
            "A close reason is only for a closed state.",
        ),
    ] {
        let (text, error) = w.call_text("orbyn_issue_set_state", args.clone()).await;
        assert!(error, "{args} was not refused: {text}");
        assert!(text.contains(wanted), "{args}: {text}");
    }
    assert!(
        w.reader.writes.lock().unwrap().is_empty(),
        "a refused move sends no write"
    );
}

#[tokio::test]
async fn an_issue_state_move_passes_an_unsupported_edit_through_as_a_failure() {
    // The fake GitLab reader keeps its edit unbuilt, as GitLab, Forgejo, Azure DevOps and Linear do:
    // the route's 422 unsupported reaches the tool as a plain failure, and nothing is read or sent.
    let w = issue_world(true).await;
    let (text, error) = w
        .call_text(
            "orbyn_issue_set_state",
            json!({ "key": "1", "state": "Open" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(text, "GitLab issue edits arrive in a later update.");
    assert!(w.reader.writes.lock().unwrap().is_empty());
    assert_eq!(
        w.reader.reads.load(Ordering::SeqCst),
        0,
        "nothing was read either"
    );
}

#[tokio::test]
async fn a_denied_state_move_answers_the_denial_and_runs_nothing() {
    let w = editable_issue_world().await;
    w.core
        .store
        .add_rule(
            &CreatePolicyRule {
                effect: PolicyEffect::Deny,
                kind: ActionKind::Tool,
                pattern: "=orbyn_issue_set_state".into(),
                engine: None,
                note: None,
            },
            "test",
        )
        .unwrap();
    let (text, error) = w
        .call_text(
            "orbyn_issue_set_state",
            json!({ "key": "1", "state": "Closed" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(
        text,
        "Blocked by policy: Matched rule =orbyn_issue_set_state"
    );
    assert!(
        w.reader.writes.lock().unwrap().is_empty(),
        "the tracker was written"
    );
    assert_eq!(w.reader.reads.load(Ordering::SeqCst), 0, "nothing was read");
    let audit = w.core.store.list_audit(None, 100).unwrap();
    assert!(
        audit
            .iter()
            .any(|row| row.action == "action.deny" && row.target == "orbyn_issue_set_state"),
        "a denied call is audited, and only as a denial"
    );
}

#[tokio::test]
async fn a_crew_worker_cannot_use_the_issue_state_tool() {
    let w = editable_issue_world().await;
    make_crew_worker(&w.core, &w.thread);
    let (text, error) = w
        .call_text(
            "orbyn_issue_set_state",
            json!({ "key": "1", "state": "Closed" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(text, "A crew worker cannot use orbyn_issue_set_state.");
    assert!(w.reader.writes.lock().unwrap().is_empty());
    assert_eq!(
        w.reader.reads.load(Ordering::SeqCst),
        0,
        "the refusal runs nothing"
    );
}

#[tokio::test]
async fn the_state_tool_is_offered_and_gated_like_the_other_issue_writes() {
    let w = issue_world(true).await;
    assert!(
        is_tool("orbyn_issue_set_state"),
        "the service's state route exists, so the tool is offered"
    );
    assert!(
        has_effect("orbyn_issue_set_state"),
        "a state move writes to the tracker"
    );
    let names = w.tool_names().await;
    assert!(
        names.iter().any(|name| name == "orbyn_issue_set_state"),
        "the tool list names it: {names:?}"
    );
}

#[tokio::test]
async fn tools_list_names_all_eight_issue_tools_with_their_effects() {
    let w = issue_world(false).await;
    let names = w.tool_names().await;
    for (name, effect) in [
        ("orbyn_issue_list", false),
        ("orbyn_issue_read", false),
        ("orbyn_issue_create", true),
        ("orbyn_issue_comment", true),
        ("orbyn_issue_set_state", true),
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
    let state = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_issue_set_state")
        .unwrap();
    assert_eq!(
        state.args,
        &[
            ("key", "string"),
            ("state", "string"),
            ("close_reason", "string")
        ]
    );
    assert_eq!(state.required, &["key", "state"]);
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
/// The forge fixture is task-local, while a pull read the cache runs on its own task needs
/// `run_with_fixture` to drive it inside that fixture's scope. These tests pin the answer
/// shapes and the MCP surface; the write tools are exercised below with the fake forge.
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
            "state must be open, closed, merged or all.",
        ),
        (
            "orbyn_pull_request_list",
            json!({ "state": 7 }),
            "state must be open, closed, merged or all.",
        ),
        (
            "orbyn_pull_request_list",
            json!({ "limit": -1 }),
            "limit must be a whole number.",
        ),
        (
            "orbyn_pull_request_list",
            json!({ "limit": 1.5 }),
            "limit must be a whole number.",
        ),
        ("orbyn_pull_request_read", json!({}), "number is required."),
        (
            "orbyn_pull_request_read",
            json!({ "number": 0 }),
            "number must be a pull request number.",
        ),
        (
            "orbyn_pull_request_checks",
            json!({ "number": 5_000_000_000u64 }),
            "number must be a pull request number.",
        ),
    ] {
        let (text, error) = w.call_text(name, args).await;
        assert!(error, "{name} answered {text}");
        assert_eq!(text, wanted, "{name}");
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
        assert_eq!(
            invalid(pull_filter(&json!({ "state": bad }))),
            "state must be open, closed, merged or all.",
            "{bad}"
        );
    }

    assert_eq!(pull_limit(&json!({})).unwrap(), 30);
    assert_eq!(pull_limit(&json!({ "limit": null })).unwrap(), 30);
    assert_eq!(pull_limit(&json!({ "limit": 0 })).unwrap(), 0);
    assert_eq!(pull_limit(&json!({ "limit": 30 })).unwrap(), 30);
    assert_eq!(pull_limit(&json!({ "limit": 50 })).unwrap(), 50);
    assert_eq!(pull_limit(&json!({ "limit": 51 })).unwrap(), 50);
    assert_eq!(pull_limit(&json!({ "limit": u64::MAX })).unwrap(), 50);
    for bad in [json!(-1), json!(1.5), json!("10"), json!(true)] {
        assert_eq!(
            invalid(pull_limit(&json!({ "limit": bad }))),
            "limit must be a whole number.",
            "{bad}"
        );
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

/// The one check every fake GitHub forge answers, and the run its link names.
const CHECK_NAME: &str = "ci";
const CHECK_URL: &str = "https://github.com/acme/app/actions/runs/123/job/9";
/// A token shaped like a GitHub one, so the redactor's fixed set recognises it.
const FAKE_TOKEN: &str = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";

/// Forty KiB of filler with the token before its tail, so the last 32 KiB still holds
/// the token's place: a tail cut alone would not hide it.
fn check_log_text() -> String {
    let mut log = "h".repeat(24 * 1024);
    log.push('\n');
    log.push_str(FAKE_TOKEN);
    log.push('\n');
    log.push_str(&"t".repeat(16 * 1024));
    log.push_str("END-OF-LOG\n");
    log
}

/// A fake forge CLI that records every call and answers a pull request read with `view`,
/// a GitHub checks read with one failing check, and its run's failed log.
fn recording_forge(calls: RecordedForgeCalls, view: Value) -> ForgeFixture {
    Arc::new(move |_, _, args| {
        calls
            .lock()
            .unwrap()
            .push(args.iter().map(|arg| (*arg).to_owned()).collect());
        if matches!(args, ["mr", "view", ..]) {
            Ok(view.to_string())
        } else if matches!(args, ["pr", "checks", ..]) {
            Ok(json!([{
                "name": CHECK_NAME,
                "state": "FAILURE",
                "link": CHECK_URL,
                "workflow": "CI",
                "description": "the failing job",
                "bucket": "fail",
            }])
            .to_string())
        } else if matches!(args, ["run", "view", "123", "--log-failed"]) {
            Ok(check_log_text())
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
async fn tools_list_names_the_pull_respond_tool_with_effect() {
    let w = pull_world().await;
    let names = w.tool_names().await;
    let name = "orbyn_pull_request_respond";
    assert!(
        names.iter().any(|listed| listed.as_str() == name),
        "{name} is not listed"
    );
    assert!(has_effect(name));
    let tool = TOOLS.iter().find(|tool| tool.name == name).unwrap();
    assert!(tool.effect);
    assert!(
        tool.description.contains("data from the forge"),
        "the respond tool does not say its text is data"
    );
    assert!(
        tool.description.contains("review thread"),
        "the respond tool does not name the thread it acts on"
    );
    assert_eq!(
        tool.args,
        &[
            ("number", "number"),
            ("thread", "string"),
            ("body", "string"),
            ("resolved", "boolean"),
        ]
    );
    assert_eq!(tool.required, &["number", "thread"]);
}

#[tokio::test]
async fn tools_list_names_the_two_pull_review_merge_tools_with_effect() {
    let w = pull_world().await;
    let names = w.tool_names().await;
    for name in ["orbyn_pull_request_review", "orbyn_pull_request_merge"] {
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

    let review = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_pull_request_review")
        .unwrap();
    assert_eq!(
        review.args,
        &[
            ("number", "number"),
            ("event", "string"),
            ("body", "string"),
        ]
    );
    assert_eq!(review.required, &["number", "event"]);
    let merge = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_pull_request_merge")
        .unwrap();
    assert_eq!(
        merge.args,
        &[
            ("number", "number"),
            ("method", "string"),
            ("head_sha", "string"),
            ("delete_branch", "boolean"),
        ]
    );
    assert_eq!(merge.required, &["number", "method", "head_sha"]);
    assert!(
        !merge
            .args
            .iter()
            .any(|(name, _)| *name == "auto" || *name == "subject" || *name == "body"),
        "the merge tool must not offer auto, a subject or a body"
    );
}

#[tokio::test]
async fn tools_list_names_the_ci_tools_with_only_the_rerun_gated() {
    let w = pull_world().await;
    let names = w.tool_names().await;
    for (name, effect) in [
        ("orbyn_pull_request_ci_logs", false),
        ("orbyn_pull_request_ci_rerun", true),
    ] {
        assert!(
            names.iter().any(|listed| listed.as_str() == name),
            "{name} is not listed"
        );
        assert_eq!(has_effect(name), effect, "{name}");
        let tool = TOOLS.iter().find(|tool| tool.name == name).expect(name);
        assert_eq!(tool.effect, effect, "{name}");
        assert_eq!(tool.args, &[("number", "number"), ("check", "string")]);
        assert_eq!(tool.required, &["number", "check"]);
        assert!(
            tool.description.contains("data from the forge"),
            "{name} does not say its text is data"
        );
    }

    let logs = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_pull_request_ci_logs")
        .unwrap();
    assert!(
        logs.description.contains("redacted"),
        "{}",
        logs.description
    );
    assert!(
        logs.description.contains("last 32 KiB"),
        "{}",
        logs.description
    );
    let rerun = TOOLS
        .iter()
        .find(|tool| tool.name == "orbyn_pull_request_ci_rerun")
        .unwrap();
    assert!(
        rerun.description.contains("failed jobs"),
        "{}",
        rerun.description
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
async fn pull_review_and_merge_refuse_bad_arguments_before_the_forge() {
    let w = pull_world().await;
    let too_long_body = "x".repeat(MAX_PULL_COMMENT_BYTES + 1);
    for (name, args, wanted) in [
        (
            "orbyn_pull_request_review",
            json!({ "number": 7 }),
            "event is required",
        ),
        (
            "orbyn_pull_request_review",
            json!({ "number": 7, "event": "approve_all" }),
            "event must be",
        ),
        (
            "orbyn_pull_request_review",
            json!({ "number": 7, "event": "request_changes" }),
            "body is required",
        ),
        (
            "orbyn_pull_request_review",
            json!({ "number": 7, "event": "comment" }),
            "body is required",
        ),
        (
            "orbyn_pull_request_review",
            json!({ "number": 7, "event": "comment", "body": "   " }),
            "body is required",
        ),
        (
            "orbyn_pull_request_review",
            json!({ "number": 7, "event": "approve", "body": too_long_body }),
            "body is at most 64 KiB",
        ),
        (
            "orbyn_pull_request_merge",
            json!({ "method": "merge", "head_sha": "abc1234" }),
            "number is required",
        ),
        (
            "orbyn_pull_request_merge",
            json!({ "number": 7, "method": "auto", "head_sha": "abc1234" }),
            "method must be",
        ),
        (
            "orbyn_pull_request_merge",
            json!({ "number": 7, "method": "merge" }),
            "head_sha is required",
        ),
        (
            "orbyn_pull_request_merge",
            json!({ "number": 7, "method": "merge", "head_sha": "not-a-sha" }),
            "7 to 64 hex",
        ),
        (
            "orbyn_pull_request_merge",
            json!({ "number": 7, "method": "merge", "head_sha": "abc123" }),
            "7 to 64 hex",
        ),
        (
            "orbyn_pull_request_merge",
            json!({ "number": 7, "method": "merge", "head_sha": "abc1234", "delete_branch": "yes" }),
            "delete_branch must be",
        ),
    ] {
        let (text, error) = w.call_text(name, args).await;
        assert!(error, "{name} answered {text}");
        assert!(text.contains(wanted), "{name}: {text}");
    }
}

#[test]
fn pull_respond_refuses_bad_arguments_before_the_forge() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let too_long_body = "x".repeat(MAX_PULL_COMMENT_BYTES + 1);
    let long_thread = "t".repeat(MAX_REPLY_TO_CHARS + 1);
    for (args, wanted) in [
        (
            json!({ "number": 7, "resolved": true }),
            "thread is required",
        ),
        (
            json!({ "number": 7, "thread": "  ", "resolved": true }),
            "thread is required",
        ),
        (
            json!({ "number": 7, "thread": "not/a/thread", "resolved": true }),
            "thread must be a review thread id",
        ),
        (
            json!({ "number": 7, "thread": long_thread, "resolved": true }),
            "thread must be a review thread id",
        ),
        (
            json!({ "number": 7, "thread": "T:1", "resolved": true }),
            "thread must be a review thread id",
        ),
        (
            json!({ "number": 7, "thread": 123, "resolved": true }),
            "thread must be a string",
        ),
        (
            json!({ "number": 7, "thread": "T1" }),
            "Name a reply, a resolved value, or both.",
        ),
        (
            json!({ "number": 7, "thread": "T1", "body": "   " }),
            "body must not be blank",
        ),
        (
            json!({ "number": 7, "thread": "T1", "body": too_long_body }),
            "body is at most 64 KiB",
        ),
        (
            json!({ "number": 7, "thread": "T1", "body": "ok", "resolved": "yes" }),
            "resolved must be true or false",
        ),
        (
            json!({ "thread": "T1", "resolved": true }),
            "number is required",
        ),
    ] {
        let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
        let fixture = recording_forge(calls.clone(), json!(null));
        let (text, error) = run_with_fixture(&runtime, fixture, async {
            w.call_text("orbyn_pull_request_respond", args).await
        });
        assert!(error, "answered {text}");
        assert!(text.contains(wanted), "{text}");
        let calls = calls.lock().unwrap();
        assert!(calls.is_empty(), "reached the forge: {calls:?}");
    }
}

#[test]
fn pull_respond_replies_to_and_resolves_a_github_review_thread() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = run_with_fixture(&runtime, fixture, async {
        w.call_json(
            "orbyn_pull_request_respond",
            json!({ "number": 7, "thread": "T1", "body": "Looks good", "resolved": true }),
        )
        .await
    });
    assert!(!error, "{answer}");
    assert_eq!(
        answer,
        json!({ "number": 7, "thread": "T1", "replied": true, "resolved": true })
    );

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(&calls[0][..3], strings(&["api", "graphql", "-f"]));
    assert!(
        calls[0]
            .iter()
            .any(|argument| argument.contains("addPullRequestReviewThreadReply")),
        "{calls:?}"
    );
    assert!(
        calls[0]
            .iter()
            .any(|argument| argument == "pullRequestReviewThreadId=T1"),
        "{calls:?}"
    );
    assert!(
        calls[0]
            .iter()
            .any(|argument| argument == "body=Looks good"),
        "{calls:?}"
    );
    assert_eq!(&calls[1][..3], strings(&["api", "graphql", "-f"]));
    assert!(
        calls[1]
            .iter()
            .any(|argument| argument.contains("resolveReviewThread")),
        "{calls:?}"
    );
    assert!(
        calls[1].iter().any(|argument| argument == "id=T1"),
        "{calls:?}"
    );
    drop(calls);
    the_respond_was_gated_and_audited_once(&w, "T1");
}

#[test]
fn pull_respond_replies_through_a_gitlab_discussion() {
    let runtime = forge_runtime();
    let w = runtime.block_on(pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = run_with_fixture(&runtime, fixture, async {
        w.call_json(
            "orbyn_pull_request_respond",
            json!({ "number": 7, "thread": "d1", "body": "Thanks", "resolved": true }),
        )
        .await
    });
    assert!(!error, "{answer}");
    assert_eq!(
        answer,
        json!({ "number": 7, "thread": "d1", "replied": true, "resolved": true })
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(
        calls[0],
        strings(&[
            "api",
            "-X",
            "POST",
            "projects/:id/merge_requests/7/discussions/d1/notes",
            "-f",
            "body=Thanks",
        ])
    );
    assert_eq!(
        calls[1],
        strings(&[
            "api",
            "-X",
            "PUT",
            "projects/:id/merge_requests/7/discussions/d1",
            "-F",
            "resolved=true",
        ])
    );
    drop(calls);
    the_respond_was_gated_and_audited_once(&w, "d1");
}

#[test]
fn pull_respond_resolves_and_unresolves_without_a_reply() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = run_with_fixture(&runtime, fixture, async {
        w.call_json(
            "orbyn_pull_request_respond",
            json!({ "number": 7, "thread": "T1", "resolved": false }),
        )
        .await
    });
    assert!(!error, "{answer}");
    assert_eq!(
        answer,
        json!({ "number": 7, "thread": "T1", "replied": false, "resolved": false })
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        calls[0]
            .iter()
            .any(|argument| argument.contains("unresolveReviewThread")),
        "{calls:?}"
    );
    drop(calls);
    the_respond_was_gated_and_audited_once(&w, "T1");
}

#[test]
fn a_denied_pull_respond_answers_the_denial_and_runs_nothing() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    w.core
        .store
        .add_rule(
            &CreatePolicyRule {
                effect: PolicyEffect::Deny,
                kind: ActionKind::Tool,
                pattern: "=orbyn_pull_request_respond".into(),
                engine: None,
                note: None,
            },
            "test",
        )
        .unwrap();
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (text, error) = run_with_fixture(&runtime, fixture, async {
        w.call_text(
            "orbyn_pull_request_respond",
            json!({ "number": 7, "thread": "T1", "resolved": true }),
        )
        .await
    });
    assert!(error, "{text}");
    assert_eq!(
        text,
        "Blocked by policy: Matched rule =orbyn_pull_request_respond"
    );
    assert!(calls.lock().unwrap().is_empty(), "the forge was reached");
    let audit = w.core.store.list_audit(None, 100).unwrap();
    assert!(
        audit.iter().any(|row| {
            row.action == "action.deny" && row.target == "orbyn_pull_request_respond"
        })
    );
    assert!(
        audit
            .iter()
            .all(|row| row.action != "git.pull_thread_respond"),
        "a denied call is not audited as done"
    );
}

#[test]
fn a_resolution_that_fails_after_a_reply_says_the_reply_was_posted() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture: ForgeFixture = {
        let calls = calls.clone();
        Arc::new(move |_, _, args| {
            calls
                .lock()
                .unwrap()
                .push(args.iter().map(|arg| (*arg).to_owned()).collect());
            if args
                .iter()
                .any(|argument| argument.contains("resolveReviewThread"))
            {
                Err(CoreError::Git(
                    "HTTP 403: Resource not accessible by integration".into(),
                ))
            } else {
                Ok(String::new())
            }
        })
    };
    let (text, error) = run_with_fixture(&runtime, fixture, async {
        w.call_text(
            "orbyn_pull_request_respond",
            json!({ "number": 7, "thread": "T1", "body": "Thanks", "resolved": true }),
        )
        .await
    });
    assert!(error, "{text}");
    assert!(text.contains("The reply was posted"), "{text}");
    assert!(text.contains("HTTP 403"), "{text}");
    assert!(text.contains("Retry without body"), "{text}");
    assert_eq!(calls.lock().unwrap().len(), 2, "the reply was not retried");
}

#[test]
fn pull_respond_replies_without_touching_the_thread_state() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = run_with_fixture(&runtime, fixture, async {
        w.call_json(
            "orbyn_pull_request_respond",
            json!({ "number": 7, "thread": "T1", "body": "Looks good" }),
        )
        .await
    });
    assert!(!error, "{answer}");
    assert_eq!(
        answer,
        json!({ "number": 7, "thread": "T1", "replied": true, "resolved": null })
    );
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        1,
        "a reply alone sends one mutation: {calls:?}"
    );
    assert!(
        calls[0]
            .iter()
            .any(|argument| argument.contains("addPullRequestReviewThreadReply")),
        "{calls:?}"
    );
    drop(calls);
    the_respond_was_gated_and_audited_once(&w, "T1");
}

#[test]
fn a_forge_without_review_threads_says_it_cannot_reply() {
    let runtime = forge_runtime();
    let w = runtime.block_on(issue_world_at(
        false,
        "https://dev.azure.com/org/project/_git/app",
    ));
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (text, error) = run_with_fixture(&runtime, fixture, async {
        w.call_text(
            "orbyn_pull_request_respond",
            json!({ "number": 7, "thread": "T1", "body": "Thanks" }),
        )
        .await
    });
    assert!(error, "{text}");
    assert_eq!(
        text,
        "Azure DevOps cannot reply to a review thread from Orbyn."
    );
    assert!(calls.lock().unwrap().is_empty(), "the forge was reached");
}

/// The respond tool was allowed once by the gate, and its write has exactly one audit row after
/// that allow.
fn the_respond_was_gated_and_audited_once(w: &IssueWorld, thread: &str) {
    let audit = w.core.store.list_audit(None, 100).unwrap();
    let allows: Vec<usize> = audit
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.action == "action.allow" && row.target == "orbyn_pull_request_respond"
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(allows.len(), 1, "the gate allowed the tool once");
    let writes: Vec<&AuditEvent> = audit
        .iter()
        .filter(|row| row.action == "git.pull_thread_respond")
        .collect();
    assert_eq!(writes.len(), 1, "one write, one audit row");
    assert_eq!(writes[0].actor, "engine:alpha");
    assert_eq!(writes[0].target, thread);
    let allow = allows[0];
    let write = audit
        .iter()
        .position(|row| row.action == "git.pull_thread_respond")
        .unwrap();
    // The store lists audit rows newest first, so the later row has the smaller index.
    assert!(write < allow, "the write row comes after the gate's allow");
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

#[tokio::test]
async fn pull_review_reaches_the_forge_and_is_audited() {
    let w = pull_world().await;
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = with_crew_fixture(fixture, async {
        w.call_json(
            "orbyn_pull_request_review",
            json!({
                "number": 7,
                "event": "comment",
                "body": "Please fix the flaky test",
            }),
        )
        .await
    })
    .await;
    assert!(!error, "{answer}");
    assert_eq!(
        answer,
        json!({ "reviewed": true, "number": 7, "event": "comment" })
    );

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(
        calls[0],
        strings(&["mr", "note", "7", "--message", "Please fix the flaky test"])
    );
    let audit = w.core.store.list_audit(None, 100).unwrap();
    let row = audit
        .iter()
        .find(|row| row.action == "git.review")
        .expect("the review is audited");
    assert_eq!(row.actor, "engine:alpha");
    assert_eq!(row.target, "7");
}

#[tokio::test]
async fn pull_review_approves_without_a_body() {
    let w = pull_world().await;
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = with_crew_fixture(fixture, async {
        w.call_json(
            "orbyn_pull_request_review",
            json!({ "number": 7, "event": "approve" }),
        )
        .await
    })
    .await;
    assert!(!error, "{answer}");
    assert_eq!(
        answer,
        json!({ "reviewed": true, "number": 7, "event": "approve" })
    );
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0], strings(&["mr", "approve", "7"]));
}

/// A pull request the fake forge answers as ready to merge.
const PULL_HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER_HEAD: &str = "fedcba9876543210fedcba9876543210fedcba98";

fn merge_view() -> Value {
    json!({
        "iid": 7,
        "title": "Merge me",
        "state": "opened",
        "source_branch": "feature",
        "target_branch": "main",
        "web_url": "https://gitlab.example.test/team/app/-/merge_requests/7",
        "description": "",
        "draft": false,
        "sha": PULL_HEAD,
        "diff_refs": { "base_sha": OTHER_HEAD },
        "detailed_merge_status": "mergeable",
        "reviewers": [],
    })
}

#[test]
fn a_merge_head_matches_its_full_id_or_prefix_ignoring_case() {
    assert!(pull_head_matches("0123456", PULL_HEAD));
    assert!(pull_head_matches(
        &PULL_HEAD.to_ascii_uppercase(),
        PULL_HEAD
    ));
    assert!(!pull_head_matches("012345", PULL_HEAD));
    assert!(!pull_head_matches(OTHER_HEAD, PULL_HEAD));
}

/// A current-thread runtime: every task a pull read spawns runs on the thread that holds
/// the task-local forge fixture, so a merge's fresh cache read reaches the fake forge.
fn forge_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// Runs `future` with a fake forge visible to the pull cache's own task.
fn run_with_fixture<T>(
    runtime: &tokio::runtime::Runtime,
    fixture: ForgeFixture,
    future: impl std::future::Future<Output = T>,
) -> T {
    use std::task::{Context, Poll, Waker};
    let scoped = with_crew_fixture(fixture, async move { runtime.block_on(future) });
    let mut scoped = std::pin::pin!(scoped);
    match std::future::Future::poll(scoped.as_mut(), &mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("the fake forge call did not finish in one poll"),
    }
}

#[test]
fn pull_merge_reaches_the_forge_and_is_audited() {
    let runtime = forge_runtime();
    let w = runtime.block_on(pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), merge_view());
    let (answer, error) = run_with_fixture(&runtime, fixture, async {
        w.call_json(
            "orbyn_pull_request_merge",
            json!({
                "number": 7,
                "method": "squash",
                "head_sha": "0123456",
                "delete_branch": true,
            }),
        )
        .await
    });
    assert!(!error, "{answer}");
    assert_eq!(
        answer,
        json!({ "merged": true, "number": 7, "method": "squash" })
    );

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(calls[0], strings(&["mr", "view", "7", "--output", "json"]));
    assert_eq!(
        calls[1],
        strings(&[
            "mr",
            "merge",
            "7",
            "--yes",
            "--squash",
            "--remove-source-branch",
            "--sha",
            PULL_HEAD,
        ])
    );
    assert!(
        !calls[1]
            .iter()
            .any(|argument| argument.as_str() == "--auto-merge"),
        "the merge is never automatic"
    );
    let audit = w.core.store.list_audit(None, 100).unwrap();
    let row = audit
        .iter()
        .find(|row| row.action == "git.merge")
        .expect("the merge is audited");
    assert_eq!(row.actor, "engine:alpha");
    assert_eq!(row.target, "7");
}

#[test]
fn pull_merge_refuses_a_pull_request_that_is_not_ready_after_the_fresh_read() {
    let runtime = forge_runtime();
    let w = runtime.block_on(pull_world());
    let mut closed = merge_view();
    closed["state"] = json!("closed");
    let mut draft = merge_view();
    draft["draft"] = json!(true);
    let mut conflicting = merge_view();
    conflicting["detailed_merge_status"] = json!("conflict");
    let mut failing = merge_view();
    failing["statusCheckRollup"] = json!([{ "status": "COMPLETED", "conclusion": "FAILURE" }]);
    let mut pending = merge_view();
    pending["statusCheckRollup"] = json!([{ "status": "IN_PROGRESS" }]);
    let mut changes_requested = merge_view();
    changes_requested["reviewDecision"] = json!("CHANGES_REQUESTED");
    let mut moved = merge_view();
    moved["sha"] = json!(OTHER_HEAD);
    let mut unreadable = merge_view();
    assert!(unreadable.as_object_mut().unwrap().remove("sha").is_some());
    for (case, view, wanted) in [
        ("closed", closed, "Only an open pull request"),
        ("draft", draft, "draft pull request"),
        ("conflicting", conflicting, "conflicts"),
        ("failing checks", failing, "failing checks"),
        ("pending checks", pending, "pending checks"),
        ("changes requested", changes_requested, "changes requested"),
        ("moved head", moved, PULL_HEAD_MOVED),
        ("unreadable head", unreadable, PULL_HEAD_MOVED),
    ] {
        let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
        let fixture = recording_forge(calls.clone(), view);
        let (text, error) = run_with_fixture(&runtime, fixture, async {
            w.call_text(
                "orbyn_pull_request_merge",
                json!({ "number": 7, "method": "merge", "head_sha": "0123456" }),
            )
            .await
        });
        assert!(error, "{case} merged: {text}");
        assert!(text.contains(wanted), "{case}: {text}");
        let calls = calls.lock().unwrap();
        let merge = strings(&["mr", "merge"]);
        assert!(
            !calls.iter().any(|args| args.starts_with(&merge)),
            "{case} reached the forge merge: {calls:?}"
        );
        assert_eq!(calls.len(), 1, "{case}: {calls:?}");
        assert_eq!(
            calls[0],
            strings(&["mr", "view", "7", "--output", "json"]),
            "{case}"
        );
    }
}

#[tokio::test]
async fn pull_ci_tools_refuse_a_forge_that_cannot_read_or_rerun_checks() {
    let w = pull_world().await;
    for (name, wanted) in [
        (
            "orbyn_pull_request_ci_logs",
            "GitLab cannot read a check log from Orbyn.",
        ),
        (
            "orbyn_pull_request_ci_rerun",
            "GitLab cannot re-run a check from Orbyn.",
        ),
    ] {
        let (text, error) = w
            .call_text(name, json!({ "number": 7, "check": CHECK_NAME }))
            .await;
        assert!(error, "{name} answered {text}");
        assert!(text.contains(wanted), "{name}: {text}");
    }
}

#[test]
fn pull_ci_logs_answers_a_redacted_tail_from_a_fresh_checks_read() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = run_with_fixture(&runtime, fixture, async {
        w.call_json(
            "orbyn_pull_request_ci_logs",
            json!({ "number": 7, "check": CHECK_NAME }),
        )
        .await
    });
    assert!(!error, "{answer}");
    assert_eq!(answer["number"], 7);
    assert_eq!(answer["check"], CHECK_NAME);
    assert_eq!(answer["note"], PULL_NOTE);
    let log = answer["log"].as_str().unwrap();
    assert!(!log.contains(FAKE_TOKEN), "the fake token is not hidden");
    assert!(
        log.contains("[hidden: GitHub token]"),
        "the token was cut away instead of redacted: {log:.80}"
    );
    assert!(log.len() <= 32 * 1024, "the log is {} bytes", log.len());
    assert!(log.ends_with("END-OF-LOG\n"), "the log's end was cut");

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(
        calls[0],
        strings(&[
            "pr",
            "checks",
            "7",
            "--json",
            "name,state,link,workflow,description,bucket",
        ])
    );
    assert_eq!(calls[1], strings(&["run", "view", "123", "--log-failed"]));
}

#[test]
fn pull_ci_logs_refuses_a_check_that_is_not_there_without_a_run_read() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (text, error) = run_with_fixture(&runtime, fixture, async {
        w.call_text(
            "orbyn_pull_request_ci_logs",
            json!({ "number": 7, "check": "not-ci" }),
        )
        .await
    });
    assert!(error, "{text}");
    assert!(
        text.contains("No check of that name on this pull request."),
        "{text}"
    );
    let calls = calls.lock().unwrap();
    assert!(
        !calls
            .iter()
            .any(|args| args.first().map(String::as_str) == Some("run")),
        "the missing check reached a run: {calls:?}"
    );
}

#[test]
fn pull_ci_rerun_reruns_the_failed_jobs_and_is_audited() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
    let fixture = recording_forge(calls.clone(), json!(null));
    let (answer, error) = run_with_fixture(&runtime, fixture, async {
        w.call_json(
            "orbyn_pull_request_ci_rerun",
            json!({ "number": 7, "check": CHECK_NAME }),
        )
        .await
    });
    assert!(!error, "{answer}");
    assert_eq!(
        answer,
        json!({ "rerun": true, "number": 7, "check": CHECK_NAME })
    );

    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(
        calls[0],
        strings(&[
            "pr",
            "checks",
            "7",
            "--json",
            "name,state,link,workflow,description,bucket",
        ])
    );
    assert_eq!(calls[1], strings(&["run", "rerun", "123", "--failed"]));

    let audit = w.core.store.list_audit(None, 100).unwrap();
    let row = audit
        .iter()
        .find(|row| row.action == "git.ci_rerun")
        .expect("the rerun is audited");
    assert_eq!(row.actor, "engine:alpha");
    assert_eq!(row.target, "7");
}

#[test]
fn pull_ci_tools_refuse_bad_arguments_before_the_forge() {
    let runtime = forge_runtime();
    let w = runtime.block_on(github_pull_world());
    let long_check = "c".repeat(MAX_CHECK_NAME_CHARS + 1);
    for (name, args, wanted) in [
        (
            "orbyn_pull_request_ci_logs",
            json!({ "number": 7 }),
            "check is required",
        ),
        (
            "orbyn_pull_request_ci_logs",
            json!({ "number": 7, "check": "   " }),
            "check is required",
        ),
        (
            "orbyn_pull_request_ci_logs",
            json!({ "number": 7, "check": "one\ntwo" }),
            "one line",
        ),
        (
            "orbyn_pull_request_ci_logs",
            json!({ "number": 7, "check": "one\rtwo" }),
            "one line",
        ),
        (
            "orbyn_pull_request_ci_logs",
            json!({ "number": 7, "check": long_check }),
            "at most 256",
        ),
        (
            "orbyn_pull_request_ci_logs",
            json!({ "check": CHECK_NAME }),
            "number is required",
        ),
        (
            "orbyn_pull_request_ci_rerun",
            json!({ "number": 7 }),
            "check is required",
        ),
        (
            "orbyn_pull_request_ci_rerun",
            json!({ "number": 7, "check": "one\ntwo" }),
            "one line",
        ),
        (
            "orbyn_pull_request_ci_rerun",
            json!({ "check": CHECK_NAME }),
            "number is required",
        ),
    ] {
        let calls: RecordedForgeCalls = Arc::new(Mutex::new(Vec::new()));
        let fixture = recording_forge(calls.clone(), json!(null));
        let (text, error) =
            run_with_fixture(&runtime, fixture, async { w.call_text(name, args).await });
        assert!(error, "{name} answered {text}");
        assert!(text.contains(wanted), "{name}: {text}");
        let calls = calls.lock().unwrap();
        assert!(calls.is_empty(), "{name} reached the forge: {calls:?}");
    }
}

#[test]
fn a_crew_worker_cannot_use_the_pull_write_or_ci_tools() {
    for name in [
        "orbyn_pull_request_comment",
        "orbyn_pull_request_respond",
        "orbyn_pull_request_update",
        "orbyn_pull_request_review",
        "orbyn_pull_request_merge",
        "orbyn_pull_request_ci_logs",
        "orbyn_pull_request_ci_rerun",
    ] {
        assert_eq!(
            crate::crew::worker_refusal(name, &json!({ "number": 7 }), "own-thread"),
            Some(format!("A crew worker cannot use {name}."))
        );
    }
}

/// A real Core with one git project, the caller's MCP token, and the device service the test
/// asked for: the fake platform tools, or the same service with no platform tools at all.
struct DeviceWorld {
    _dir: TempDir,
    core: Arc<Core>,
    token: String,
    thread: String,
    binding: McpBinding,
    fake: Fake,
}

impl DeviceWorld {
    async fn new(devices: impl FnOnce(&Fake) -> crate::devices::Devices) -> Self {
        let dir = TempDir::new();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init", "-q", "-b", "main"]);
        let fake = Fake::new();
        let mut config =
            crate::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        config.engines.claude_program = dir.path().join("no-claude").to_string_lossy().into_owned();
        config.engines.codex_program = dir.path().join("no-codex").to_string_lossy().into_owned();
        config.engines.known_engine_ids = Some(Vec::new());
        let mut core = Core::open(config).unwrap();
        core.devices = Arc::new(devices(&fake));
        let core = Arc::new(core);
        core.mcp.bind_host(&core);
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
        // A device tool is only reachable inside a live run, and the gate asks a person for
        // an effect like any other, so the world has a live run from the start.
        let run = core
            .store
            .admit_run(crate::store::Admission {
                thread_id: &thread.id,
                prompt: "device tools",
                actor: "test",
            })
            .unwrap();
        core.store.mark_starting(&run.id).unwrap();
        core.store.mark_running(&run.id).unwrap();
        let binding = McpBinding {
            scope: RunScope {
                run_id: run.id.clone(),
                thread_id: thread.id.clone(),
                engine: "alpha".into(),
                family: "alpha".into(),
                write_scope: None,
                mode: PermissionMode::FullAccess,
                plan: false,
            },
            permission_prompt: false,
            skills: Vec::new(),
        };
        let token = core.mcp.register(binding.clone()).unwrap();
        Self {
            _dir: dir,
            core,
            token,
            thread: thread.id,
            binding,
            fake,
        }
    }

    /// One request through the MCP transport, the way the shell sends it: the JSON-RPC
    /// envelope, the token's binding and the budget timer around the dispatch entry.
    async fn rpc(&self, method: &str, params: Value) -> Value {
        let message = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        self.core
            .mcp
            .handle(&self.token, &message)
            .await
            .unwrap()
            .unwrap()
    }

    /// Calls the tool through that same transport, so a device tool test covers the chain a
    /// live engine does and not only the dispatch entry behind it (#1515).
    async fn call_text(&self, name: &str, args: Value) -> (String, bool) {
        let reply = self
            .rpc("tools/call", json!({ "name": name, "arguments": args }))
            .await;
        if let Some(text) = reply["result"]["content"][0]["text"].as_str() {
            return (text.to_owned(), reply["result"]["isError"] == json!(true));
        }
        // A name the tool table never held is the transport's own error, not a tool result.
        (
            reply["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            true,
        )
    }

    async fn call_json(&self, name: &str, args: Value) -> (Value, bool) {
        let (text, error) = self.call_text(name, args).await;
        let answer = serde_json::from_str(&text)
            .unwrap_or_else(|why| panic!("{name} answered text that is not JSON ({why}): {text}"));
        (answer, error)
    }

    async fn tool_names(&self) -> Vec<String> {
        let reply = self
            .core
            .mcp
            .handle(
                &self.token,
                &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} }),
            )
            .await
            .unwrap()
            .unwrap();
        reply["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_owned())
            .collect()
    }

    fn audit(&self) -> Vec<AuditEvent> {
        self.core.store.list_audit(None, 100).unwrap()
    }

    /// The same world with a permission mode that asks the gate, so a test can answer it.
    fn in_mode(mut self, mode: PermissionMode) -> Self {
        self.binding.scope.mode = mode;
        self
    }
}

/// The one audit row that action wrote.
fn the_one<'a>(audit: &'a [AuditEvent], action: &str) -> &'a AuditEvent {
    let rows: Vec<&AuditEvent> = audit.iter().filter(|row| row.action == action).collect();
    assert_eq!(rows.len(), 1, "{action}: {rows:?}");
    rows[0]
}

/// The gate allowed exactly this tool once, as a tool action.
fn the_gate_allowed_once(audit: &[AuditEvent], tool: &str) {
    let row = the_one(audit, "action.allow");
    assert_eq!(row.target, tool);
    assert_eq!(row.detail["kind"], json!("tool"));
    assert!(
        audit.iter().all(|row| row.action != "action.deny"),
        "no denial"
    );
}

/// Makes the thread the worker of a stored Crew task.
fn make_crew_worker(core: &Core, thread: &str) {
    use orbyn_contracts as wire;
    let stamper = crate::crew::Stamper {
        crew_id: wire::CrewId::new("crew-1").unwrap(),
        engine: wire::CrewToken::new("alpha").unwrap(),
        member: wire::CrewMember::new("lead-01").unwrap(),
        now: wire::CrewTimestamp::new("2026-11-12T10:00:00Z").unwrap(),
    };
    let brief = stamper
        .brief(&json!({
            "kind": "implement",
            "outcome": "The device tools.",
            "inputs": [],
            "files": [{ "path": "src/**", "mode": "write" }],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["Done."],
            "budget": { "turns": 20, "minutes": 30 }
        }))
        .unwrap();
    let key = stamper
        .answer_key(
            &json!({
                "checks": [{ "id": "chk-run", "kind": "command",
                    "params": { "run": "echo ok" }, "proves": "It runs." }],
                "protected_paths": ["tests/**"]
            }),
            &brief.id,
        )
        .unwrap();
    let task = core
        .store
        .create_crew_task(&brief, &key, "run-device-tools")
        .unwrap();
    core.store
        .add_crew_run(&task.id, "lead-thread", "member-01")
        .unwrap();
    core.store
        .set_crew_worker(&task.id, thread, "/work", None)
        .unwrap();
}

/// The device tool names a machine like this one lists, in the table's order.
fn device_tool_names(devices: &crate::devices::Devices) -> Vec<String> {
    tool_specs(devices.tools_available())
        .filter_map(|spec| spec["name"].as_str().map(str::to_owned))
        .filter(|name| is_device_tool(name))
        .collect()
}

#[test]
fn the_device_tools_are_listed_only_when_one_platform_has_its_tools() {
    let fake = Fake::new();
    assert_eq!(
        device_tool_names(&fake.devices()),
        [
            "orbyn_device_list",
            "orbyn_device_screenshot",
            "orbyn_device_fold",
            "orbyn_device_unfold",
            "orbyn_device_rotate",
            "orbyn_device_button",
            "orbyn_device_hinge",
            "orbyn_device_hinge_preset",
            "orbyn_device_power_off",
        ]
    );
    assert_eq!(
        device_tool_names(&fake.unavailable_devices()),
        Vec::<String>::new()
    );
    for name in device_tools() {
        assert!(has_effect(name), "{name}");
        assert!(is_tool(name), "{name}");
    }
}

/// Every device tool, in the table's order, so a test can walk the whole surface.
fn device_tools() -> [&'static str; 9] {
    [
        "orbyn_device_list",
        "orbyn_device_screenshot",
        "orbyn_device_fold",
        "orbyn_device_unfold",
        "orbyn_device_rotate",
        "orbyn_device_button",
        "orbyn_device_hinge",
        "orbyn_device_hinge_preset",
        "orbyn_device_power_off",
    ]
}

#[tokio::test]
async fn the_device_tools_are_hidden_from_a_machine_without_platform_tools() {
    let w = DeviceWorld::new(Fake::unavailable_devices).await;
    let names = w.tool_names().await;
    assert!(!names.iter().any(|name| is_device_tool(name)), "{names:?}");
    // A hidden tool answers its own NotFound, as if the table never held it.
    for name in ["orbyn_device_list", "orbyn_device_power_off"] {
        let (text, error) = w.call_text(name, json!({})).await;
        assert!(error, "{name}: {text}");
        assert_eq!(text, format!("tool {name} not found"));
    }
    // A name the table never held is the transport's own error, not a tool's answer.
    let (text, error) = w.call_text("orbyn_no_such_tool", json!({})).await;
    assert!(error, "{text}");
    assert_eq!(text, "Unknown tool orbyn_no_such_tool.");
}

#[tokio::test]
async fn the_device_list_is_gated_once_and_audited_before_the_effect() {
    let w = DeviceWorld::new(Fake::devices).await;
    w.fake.set("orientation", 2);
    let (answer, error) = w.call_json("orbyn_device_list", json!({})).await;
    assert!(!error, "{answer}");
    // The service's own list: ids, names, platforms, states; no paths.
    assert_eq!(
        answer["devices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|device| device["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        [
            "android:Pixel_8",
            "android:Tablet",
            "ios:UDID-1",
            "ios:UDID-2"
        ]
    );
    assert_eq!(answer["devices"][0]["state"], "booted");
    assert_eq!(answer["devices"][1]["state"], "shutdown");
    assert_eq!(answer["devices"][0]["platform"], "android");
    assert!(answer["platforms"][0]["available"].as_bool().unwrap());

    let audit = w.audit();
    the_gate_allowed_once(&audit, "orbyn_device_list");
    let row = the_one(&audit, "device.tool.list");
    assert_eq!(row.actor, "engine:alpha");
    assert_eq!(row.target, "");
    assert_eq!(row.thread_id.as_deref(), Some(w.thread.as_str()));
    // Rule 6: written before the list is read, so it holds fixed values only.
    assert_eq!(row.detail, json!({}));
}

#[tokio::test]
async fn the_device_screenshot_is_gated_once_and_audited_before_the_effect() {
    let w = DeviceWorld::new(Fake::devices).await;
    w.fake.set("screenshot_bytes", 64);
    let (answer, error) = w
        .call_json(
            "orbyn_device_screenshot",
            json!({ "device_id": "android:Pixel_8" }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["device"]["id"], "android:Pixel_8");
    let image = base64::engine::general_purpose::STANDARD.encode(fake_png(64));
    assert_eq!(answer["image"], json!(image));
    // The effect ran: the fake tool took exactly one screenshot.
    let screenshots = w
        .fake
        .calls()
        .into_iter()
        .filter(|call| {
            call["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg.as_str() == Some("screencap"))
        })
        .count();
    assert_eq!(screenshots, 1);

    let audit = w.audit();
    the_gate_allowed_once(&audit, "orbyn_device_screenshot");
    let row = the_one(&audit, "device.tool.screenshot");
    assert_eq!(row.actor, "engine:alpha");
    assert_eq!(row.target, "android:Pixel_8");
    // Rule 6: written before the shot, so it holds fixed values only.
    assert_eq!(row.detail, json!({}));
    let serialized = serde_json::to_string(row).unwrap();
    assert!(!serialized.contains(&image[..16]), "the row holds no bytes");
    assert!(!serialized.contains('/'), "the row holds no path");
}

#[tokio::test]
async fn a_denied_device_call_answers_the_denial_and_runs_nothing() {
    for (tool, args) in [
        ("orbyn_device_list", json!({})),
        (
            "orbyn_device_screenshot",
            json!({ "device_id": "android:Pixel_8" }),
        ),
        (
            "orbyn_device_fold",
            json!({ "device_id": "android:Pixel_Fold" }),
        ),
        (
            "orbyn_device_unfold",
            json!({ "device_id": "android:Pixel_Fold" }),
        ),
        (
            "orbyn_device_rotate",
            json!({ "device_id": "android:Pixel_8", "value": "portrait" }),
        ),
        (
            "orbyn_device_button",
            json!({ "device_id": "android:Pixel_8", "value": "home" }),
        ),
        (
            "orbyn_device_hinge",
            json!({ "device_id": "android:Pixel_Fold", "angle": 90 }),
        ),
        (
            "orbyn_device_hinge_preset",
            json!({ "device_id": "android:Pixel_Fold", "preset": "open" }),
        ),
        (
            "orbyn_device_power_off",
            json!({ "device_id": "android:Pixel_8" }),
        ),
    ] {
        let w = DeviceWorld::new(Fake::devices).await;
        w.core
            .store
            .add_rule(
                &CreatePolicyRule {
                    effect: PolicyEffect::Deny,
                    kind: ActionKind::Tool,
                    pattern: format!("={tool}"),
                    engine: None,
                    note: None,
                },
                "test",
            )
            .unwrap();
        let (text, error) = w.call_text(tool, args).await;
        assert!(error, "{tool}: {text}");
        assert_eq!(text, format!("Blocked by policy: Matched rule ={tool}"));
        // The gate refused before either tool ran, and no device tool row was written.
        assert!(w.fake.calls().is_empty(), "{tool} reached a platform tool");
        let audit = w.audit();
        let row = the_one(&audit, "action.deny");
        assert_eq!(row.target, tool);
        assert_eq!(row.detail["kind"], json!("tool"));
        assert!(
            audit
                .iter()
                .all(|row| !row.action.starts_with("device.tool.")),
            "{tool} was audited as done"
        );
    }
}

#[tokio::test]
async fn a_crew_worker_is_refused_the_device_tools_before_the_gate() {
    let w = DeviceWorld::new(Fake::devices).await;
    make_crew_worker(&w.core, &w.thread);
    for (tool, args) in [
        ("orbyn_device_list", json!({})),
        (
            "orbyn_device_screenshot",
            json!({ "device_id": "android:Pixel_8" }),
        ),
        (
            "orbyn_device_fold",
            json!({ "device_id": "android:Pixel_Fold" }),
        ),
        (
            "orbyn_device_unfold",
            json!({ "device_id": "android:Pixel_Fold" }),
        ),
        (
            "orbyn_device_rotate",
            json!({ "device_id": "android:Pixel_8", "value": "portrait" }),
        ),
        (
            "orbyn_device_button",
            json!({ "device_id": "android:Pixel_8", "value": "home" }),
        ),
        (
            "orbyn_device_hinge",
            json!({ "device_id": "android:Pixel_Fold", "angle": 90 }),
        ),
        (
            "orbyn_device_hinge_preset",
            json!({ "device_id": "android:Pixel_Fold", "preset": "open" }),
        ),
        (
            "orbyn_device_power_off",
            json!({ "device_id": "android:Pixel_8" }),
        ),
    ] {
        let (text, error) = w.call_text(tool, args).await;
        assert!(error, "{tool}: {text}");
        assert_eq!(text, format!("A crew worker cannot use {tool}."));
    }
    // Refused before the gate: no decision, no platform process, no device tool row.
    assert!(w.fake.calls().is_empty());
    let audit = w.audit();
    assert!(
        audit
            .iter()
            .all(|row| row.action != "action.allow" && row.action != "action.deny"),
        "{audit:?}"
    );
    assert!(
        audit
            .iter()
            .all(|row| !row.action.starts_with("device.tool."))
    );
}

#[tokio::test]
async fn an_unknown_device_id_is_the_services_device_not_found() {
    let w = DeviceWorld::new(Fake::devices).await;
    let (text, error) = w
        .call_text(
            "orbyn_device_screenshot",
            json!({ "device_id": "android:NoSuchDevice" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(text, crate::devices::DEVICE_NOT_FOUND_MESSAGE);
    // No screenshot was taken for a device the list never named.
    let screenshots = w
        .fake
        .calls()
        .iter()
        .filter(|call| {
            call["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg.as_str() == Some("screencap"))
        })
        .count();
    assert_eq!(screenshots, 0);
    // The row exists even though the device call failed: it is written before that call.
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.screenshot");
    assert_eq!(row.target, "android:NoSuchDevice");
    assert_eq!(row.detail, json!({}));
}

#[tokio::test]
async fn a_screenshot_over_the_tools_image_cap_is_taken_and_says_its_size() {
    let w = DeviceWorld::new(Fake::devices).await;
    // 200 KiB of PNG: 273068 base64 characters, over the tool's 192 KiB image cap.
    w.fake.set("screenshot_bytes", 200 * 1024);
    let (answer, error) = w
        .call_json(
            "orbyn_device_screenshot",
            json!({ "device_id": "android:Pixel_8" }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["device"]["id"], "android:Pixel_8");
    assert!(answer.get("image").is_none(), "{answer}");
    assert_eq!(answer["image_base64_bytes"], json!(273_068));
    let note = answer["note"].as_str().unwrap();
    assert!(note.contains("over the tool's 192 KiB image cap"), "{note}");
    assert!(!note.contains('/'), "the note holds no path");
    // The row is written before the call, so it never carries the image's size either.
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.screenshot");
    assert_eq!(row.detail, json!({}));
    let serialized = serde_json::to_string(row).unwrap();
    assert!(!serialized.contains("273068"), "the row holds no size");
}

#[tokio::test]
async fn the_device_list_row_is_written_before_the_devices_are_read() {
    let w = DeviceWorld::new(Fake::devices).await;
    w.fake.set("list_delay_ms", 2000);
    let core = w.core.clone();
    let binding = w.binding.clone();
    let call = tokio::spawn(async move {
        core.call_orbyn_tool(&binding, "orbyn_device_list", &json!({}))
            .await
    });
    w.fake.wait_for_call("adb", "devices").await;
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.list");
    assert_eq!(row.target, "");
    assert_eq!(row.detail, json!({}));
    let answer = call.await.unwrap().unwrap();
    assert_eq!(answer["devices"].as_array().unwrap().len(), 4);
    // One row for the one call, not a second one after the answer.
    let audit = w.audit();
    assert_eq!(
        audit
            .iter()
            .filter(|row| row.action == "device.tool.list")
            .count(),
        1
    );
}

#[tokio::test]
async fn the_device_screenshot_row_is_written_before_the_device_answers() {
    let w = DeviceWorld::new(Fake::devices).await;
    // The fake logs the screenshot command and then holds its answer, so the row has to be
    // there while the call is still running.
    w.fake.set("screenshot_delay_ms", 2000);
    let core = w.core.clone();
    let binding = w.binding.clone();
    let args = json!({ "device_id": "android:Pixel_8" });
    let call = tokio::spawn(async move {
        core.call_orbyn_tool(&binding, "orbyn_device_screenshot", &args)
            .await
    });
    w.fake.wait_for_call("adb", "screencap").await;
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.screenshot");
    assert_eq!(row.target, "android:Pixel_8");
    assert_eq!(row.detail, json!({}));
    let answer = call.await.unwrap().unwrap();
    assert_eq!(answer["ok"], true);
    // One row for the one call, not a second one after the answer.
    let audit = w.audit();
    assert_eq!(
        audit
            .iter()
            .filter(|row| row.action == "device.tool.screenshot")
            .count(),
        1
    );
}

/// The screenshot's success path through the transport on a normal debug test thread: the
/// reply is the JSON-RPC envelope the shell reads, with the image inside it. A device call
/// that grew past the thread's stack would abort this test instead of failing it (#1515).
#[tokio::test]
async fn the_device_screenshot_succeeds_through_the_mcp_transport() {
    let w = DeviceWorld::new(Fake::devices).await;
    let reply = w
        .rpc(
            "tools/call",
            json!({
                "name": "orbyn_device_screenshot",
                "arguments": { "device_id": "android:Pixel_8" },
            }),
        )
        .await;
    assert!(reply.get("error").is_none(), "{reply}");
    let content = &reply["result"]["content"][0];
    assert_eq!(content["type"], "text", "{reply}");
    assert!(reply["result"]["isError"] != json!(true), "{reply}");
    let answer: Value = serde_json::from_str(content["text"].as_str().unwrap()).unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["device"]["id"], "android:Pixel_8", "{answer}");
    assert!(
        answer["image"]
            .as_str()
            .is_some_and(|image| !image.is_empty()),
        "{answer}"
    );
    // One row for the one call, written before the device answered (rule 6).
    the_one(&w.audit(), "device.tool.screenshot");
}

/// The list tool takes the same transport path, so its counts arrive inside the envelope
/// too, and the deep future is not only the screenshot's (#1515).
#[tokio::test]
async fn the_device_list_succeeds_through_the_mcp_transport() {
    let w = DeviceWorld::new(Fake::devices).await;
    let reply = w
        .rpc(
            "tools/call",
            json!({ "name": "orbyn_device_list", "arguments": {} }),
        )
        .await;
    assert!(reply.get("error").is_none(), "{reply}");
    assert!(reply["result"]["isError"] != json!(true), "{reply}");
    let answer: Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(answer["devices"].as_array().unwrap().len(), 4, "{answer}");
    assert_eq!(answer["platforms"].as_array().unwrap().len(), 2, "{answer}");
    the_one(&w.audit(), "device.tool.list");
}

#[tokio::test]
async fn a_path_like_device_id_never_reaches_the_audit_target() {
    for id in [
        "/Users/alice/private.png",
        "C:\\Users\\alice\\private.png",
        "android:Pi\ncel",
    ] {
        let w = DeviceWorld::new(Fake::devices).await;
        let (text, error) = w
            .call_text("orbyn_device_screenshot", json!({ "device_id": id }))
            .await;
        assert!(error, "{id}: {text}");
        assert_eq!(text, crate::devices::DEVICE_NOT_FOUND_MESSAGE, "{id}");
        // The row exists even though the call failed, because it is written before the call.
        let audit = w.audit();
        let row = the_one(&audit, "device.tool.screenshot");
        assert_eq!(row.target, "-", "{id}");
        assert_eq!(row.detail, json!({}));
        let serialized = serde_json::to_string(row).unwrap();
        assert!(!serialized.contains('/'), "{id}: {serialized}");
        assert!(!serialized.contains('\\'), "{id}: {serialized}");
        assert!(!serialized.contains("alice"), "{id}: {serialized}");
        assert!(!serialized.contains("private"), "{id}: {serialized}");
    }
}

/// Every argv the fake platform tool ran, as the words after the tool name.
fn device_calls(w: &DeviceWorld, tool: &str) -> Vec<Vec<String>> {
    w.fake
        .calls()
        .into_iter()
        .filter(|call| call["tool"] == tool)
        .map(|call| {
            call["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|arg| arg.as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

/// Whether an argv ends with exactly these words.
fn argv_ends_with(argv: &[String], tail: &[&str]) -> bool {
    argv.len() >= tail.len()
        && argv[argv.len() - tail.len()..]
            .iter()
            .zip(tail)
            .all(|(word, wanted)| word == wanted)
}

/// The approvals waiting for a person, once a tool has asked the gate in a mode that asks.
async fn wait_for_pending_approval(w: &DeviceWorld) -> Vec<orbyn_contracts::Approval> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let pending = w
                .core
                .store
                .list_approvals(Some(orbyn_contracts::ApprovalState::Pending))
                .unwrap();
            if !pending.is_empty() {
                return pending;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the tool never asked the gate")
}

#[test]
fn every_device_action_tool_is_an_effect_tool_that_takes_a_device_id() {
    for name in [
        "orbyn_device_fold",
        "orbyn_device_unfold",
        "orbyn_device_rotate",
        "orbyn_device_button",
        "orbyn_device_hinge",
        "orbyn_device_hinge_preset",
        "orbyn_device_power_off",
    ] {
        assert!(is_tool(name), "{name}");
        assert!(has_effect(name), "{name}");
        let tool = TOOLS.iter().find(|tool| tool.name == name).expect(name);
        assert!(tool.effect, "{name} must go through the gate");
        assert_eq!(tool.required.first().copied(), Some("device_id"), "{name}");
    }
}

#[test]
fn the_device_action_arguments_are_checked_against_the_services_own_sets() {
    for value in [
        "portrait",
        "landscape_left",
        "portrait_upside_down",
        "landscape_right",
    ] {
        assert!(device_orientation(value).is_ok(), "{value}");
    }
    assert_eq!(
        invalid(device_orientation("sideways")),
        "value must be portrait, landscape_left, portrait_upside_down or landscape_right."
    );
    for value in ["home", "back", "recents"] {
        assert!(device_button(value).is_ok(), "{value}");
    }
    assert_eq!(
        invalid(device_button("power")),
        "value must be home, back or recents."
    );
    for value in ["closed", "half_open", "open"] {
        assert!(hinge_preset(value).is_ok(), "{value}");
    }
    assert_eq!(
        invalid(hinge_preset("tent")),
        "preset must be closed, half_open or open."
    );
    for angle in [1, 90, 179] {
        assert_eq!(hinge_angle(&json!({ "angle": angle })).unwrap(), angle);
    }
    assert_eq!(invalid(hinge_angle(&json!({}))), "angle is required.");
    for angle in [json!(0), json!(180), json!(1000), json!(-1), json!(90.5)] {
        assert_eq!(
            invalid(hinge_angle(&json!({ "angle": angle }))),
            "angle must be a whole number from 1 to 179.",
            "{angle}"
        );
    }
}

#[tokio::test]
async fn an_invalid_device_action_is_refused_before_the_gate_asks() {
    let w = DeviceWorld::new(Fake::devices).await;
    for (tool, args, wanted) in [
        ("orbyn_device_fold", json!({}), "device_id is required."),
        (
            "orbyn_device_rotate",
            json!({ "device_id": "android:Pixel_8", "value": "sideways" }),
            "value must be portrait, landscape_left, portrait_upside_down or landscape_right.",
        ),
        (
            "orbyn_device_button",
            json!({ "device_id": "android:Pixel_8", "value": "power" }),
            "value must be home, back or recents.",
        ),
        (
            "orbyn_device_hinge",
            json!({ "device_id": "android:Pixel_Fold" }),
            "angle is required.",
        ),
        (
            "orbyn_device_hinge",
            json!({ "device_id": "android:Pixel_Fold", "angle": 0 }),
            "angle must be a whole number from 1 to 179.",
        ),
        (
            "orbyn_device_hinge_preset",
            json!({ "device_id": "android:Pixel_Fold", "preset": "tent" }),
            "preset must be closed, half_open or open.",
        ),
        (
            "orbyn_device_power_off",
            json!({ "device_id": "  " }),
            "device_id is required.",
        ),
    ] {
        let (text, error) = w.call_text(tool, args).await;
        assert!(error, "{tool}: {text}");
        assert_eq!(text, wanted, "{tool}");
    }
    // Refused before the gate and before any platform tool: no decision, no effect.
    assert!(
        w.fake.calls().is_empty(),
        "an invalid action reached a tool"
    );
    let audit = w.audit();
    assert!(
        audit
            .iter()
            .all(|row| row.action != "action.allow" && row.action != "action.deny"),
        "{audit:?}"
    );
    assert!(
        audit
            .iter()
            .all(|row| !row.action.starts_with("device.tool.")),
        "{audit:?}"
    );
}

#[tokio::test]
async fn the_fold_and_unfold_tools_send_the_emulator_commands_and_answer_the_read_back() {
    for (tool, action, posture) in [
        ("orbyn_device_fold", "fold", "closed"),
        ("orbyn_device_unfold", "unfold", "opened"),
    ] {
        let w = DeviceWorld::new(Fake::devices).await;
        w.fake.set("android_mode", "foldable");
        let (answer, error) = w
            .call_json(tool, json!({ "device_id": "android:Pixel_Fold" }))
            .await;
        assert!(!error, "{tool}: {answer}");
        assert_eq!(answer["ok"], true, "{tool}");
        assert_eq!(answer["device"]["id"], "android:Pixel_Fold", "{tool}");
        assert_eq!(answer["device"]["fold"]["posture"], posture, "{tool}");
        assert!(
            device_calls(&w, "adb")
                .iter()
                .any(|argv| argv_ends_with(argv, &["emu", action])),
            "{tool}: {:?}",
            device_calls(&w, "adb")
        );
        let audit = w.audit();
        the_gate_allowed_once(&audit, tool);
        let row = the_one(&audit, &format!("device.tool.{action}"));
        assert_eq!(row.actor, "engine:alpha", "{tool}");
        assert_eq!(row.target, "android:Pixel_Fold", "{tool}");
        assert_eq!(row.detail, json!({}), "{tool}");
    }
}

#[tokio::test]
async fn the_rotate_tool_sends_the_orientation_the_caller_chose() {
    for (value, rotation) in [
        ("portrait", "0"),
        ("landscape_left", "1"),
        ("portrait_upside_down", "2"),
        ("landscape_right", "3"),
    ] {
        let w = DeviceWorld::new(Fake::devices).await;
        let (answer, error) = w
            .call_json(
                "orbyn_device_rotate",
                json!({ "device_id": "android:Pixel_8", "value": value }),
            )
            .await;
        assert!(!error, "{value}: {answer}");
        assert_eq!(answer["ok"], true, "{value}");
        assert_eq!(answer["device"]["id"], "android:Pixel_8", "{value}");
        // The service reads the display back rather than trusting the request.
        assert_eq!(answer["device"]["orientation"], value, "{value}");
        let calls = device_calls(&w, "adb");
        assert!(
            calls.iter().any(|argv| argv_ends_with(
                argv,
                &[
                    "shell",
                    "settings",
                    "put",
                    "system",
                    "accelerometer_rotation",
                    "0"
                ]
            )),
            "{value}: {calls:?}"
        );
        assert!(
            calls.iter().any(|argv| argv_ends_with(
                argv,
                &[
                    "shell",
                    "settings",
                    "put",
                    "system",
                    "user_rotation",
                    rotation
                ]
            )),
            "{value}: {calls:?}"
        );
        let audit = w.audit();
        the_gate_allowed_once(&audit, "orbyn_device_rotate");
        let row = the_one(&audit, "device.tool.rotate");
        assert_eq!(row.target, "android:Pixel_8");
        assert_eq!(row.detail, json!({}));
    }
}

#[tokio::test]
async fn the_button_tool_presses_the_named_button() {
    for (value, key) in [
        ("home", "KEYCODE_HOME"),
        ("back", "KEYCODE_BACK"),
        ("recents", "KEYCODE_APP_SWITCH"),
    ] {
        let w = DeviceWorld::new(Fake::devices).await;
        let (answer, error) = w
            .call_json(
                "orbyn_device_button",
                json!({ "device_id": "android:Pixel_8", "value": value }),
            )
            .await;
        assert!(!error, "{value}: {answer}");
        assert_eq!(answer["device"]["id"], "android:Pixel_8", "{value}");
        assert!(
            device_calls(&w, "adb")
                .iter()
                .any(|argv| argv_ends_with(argv, &["shell", "input", "keyevent", key])),
            "{value}: {:?}",
            device_calls(&w, "adb")
        );
        let audit = w.audit();
        the_gate_allowed_once(&audit, "orbyn_device_button");
        let row = the_one(&audit, "device.tool.button");
        assert_eq!(row.target, "android:Pixel_8");
        assert_eq!(row.detail, json!({}));
    }
    // iOS has home only, through simctl.
    let w = DeviceWorld::new(Fake::devices).await;
    let (answer, error) = w
        .call_json(
            "orbyn_device_button",
            json!({ "device_id": "ios:UDID-1", "value": "home" }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["device"]["id"], "ios:UDID-1");
    assert!(
        device_calls(&w, "xcrun")
            .iter()
            .any(|argv| argv_ends_with(argv, &["simctl", "ui", "UDID-1", "home"])),
        "{:?}",
        device_calls(&w, "xcrun")
    );
    let audit = w.audit();
    the_gate_allowed_once(&audit, "orbyn_device_button");
    the_one(&audit, "device.tool.button");
}

#[tokio::test]
async fn the_hinge_tool_sends_the_angle_and_answers_the_read_back() {
    let w = DeviceWorld::new(Fake::devices).await;
    w.fake.set("android_mode", "foldable");
    let (answer, error) = w
        .call_json(
            "orbyn_device_hinge",
            json!({ "device_id": "android:Pixel_Fold", "angle": 90 }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["device"]["id"], "android:Pixel_Fold");
    assert_eq!(answer["device"]["fold"]["posture"], "half_opened");
    assert_eq!(answer["device"]["fold"]["hinge_angle"].as_f64(), Some(90.0));
    assert!(
        device_calls(&w, "adb")
            .iter()
            .any(|argv| argv_ends_with(argv, &["emu", "sensor", "set", "hinge-angle0", "90"])),
        "{:?}",
        device_calls(&w, "adb")
    );
    let audit = w.audit();
    the_gate_allowed_once(&audit, "orbyn_device_hinge");
    let row = the_one(&audit, "device.tool.hinge");
    assert_eq!(row.target, "android:Pixel_Fold");
    assert_eq!(row.detail, json!({}));
}

#[tokio::test]
async fn the_hinge_preset_tool_sends_the_services_own_pose() {
    for (preset, action, posture, angle) in [
        ("closed", &["emu", "fold"][..], "closed", 0.0),
        (
            "half_open",
            &["emu", "sensor", "set", "hinge-angle0", "130"],
            "half_opened",
            130.0,
        ),
        ("open", &["emu", "unfold"][..], "opened", 180.0),
    ] {
        let w = DeviceWorld::new(Fake::devices).await;
        w.fake.set("android_mode", "foldable");
        let (answer, error) = w
            .call_json(
                "orbyn_device_hinge_preset",
                json!({ "device_id": "android:Pixel_Fold", "preset": preset }),
            )
            .await;
        assert!(!error, "{preset}: {answer}");
        assert_eq!(answer["device"]["id"], "android:Pixel_Fold", "{preset}");
        assert_eq!(answer["device"]["fold"]["posture"], posture, "{preset}");
        assert_eq!(
            answer["device"]["fold"]["hinge_angle"].as_f64(),
            Some(angle),
            "{preset}"
        );
        assert!(
            device_calls(&w, "adb")
                .iter()
                .any(|argv| argv_ends_with(argv, action)),
            "{preset}: {:?}",
            device_calls(&w, "adb")
        );
        let audit = w.audit();
        the_gate_allowed_once(&audit, "orbyn_device_hinge_preset");
        let row = the_one(&audit, "device.tool.hinge_preset");
        assert_eq!(row.target, "android:Pixel_Fold");
        assert_eq!(row.detail, json!({}));
    }
}

#[tokio::test]
async fn the_power_off_tool_shuts_the_device_down_and_answers_the_read_back() {
    // Android: the emulator console's kill, then the list reads it back as shutdown.
    let w = DeviceWorld::new(Fake::devices).await;
    let (answer, error) = w
        .call_json(
            "orbyn_device_power_off",
            json!({ "device_id": "android:Pixel_8" }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["device"]["id"], "android:Pixel_8");
    assert_eq!(answer["device"]["state"], "shutdown");
    assert!(
        device_calls(&w, "adb")
            .iter()
            .any(|argv| argv_ends_with(argv, &["emu", "kill"])),
        "{:?}",
        device_calls(&w, "adb")
    );
    let audit = w.audit();
    the_gate_allowed_once(&audit, "orbyn_device_power_off");
    let row = the_one(&audit, "device.tool.power_off");
    assert_eq!(row.actor, "engine:alpha");
    assert_eq!(row.target, "android:Pixel_8");
    assert_eq!(row.detail, json!({}));
    // iOS: simctl shutdown, then the list reads it back as shutdown.
    let w = DeviceWorld::new(Fake::devices).await;
    let (answer, error) = w
        .call_json(
            "orbyn_device_power_off",
            json!({ "device_id": "ios:UDID-1" }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["device"]["id"], "ios:UDID-1");
    assert_eq!(answer["device"]["state"], "shutdown");
    assert!(
        device_calls(&w, "xcrun")
            .iter()
            .any(|argv| argv_ends_with(argv, &["simctl", "shutdown", "UDID-1"])),
        "{:?}",
        device_calls(&w, "xcrun")
    );
    let audit = w.audit();
    the_gate_allowed_once(&audit, "orbyn_device_power_off");
    the_one(&audit, "device.tool.power_off");
}

#[tokio::test]
async fn the_device_action_tools_pass_the_services_refusals_through() {
    // A device that does not fold.
    let w = DeviceWorld::new(Fake::devices).await;
    let (text, error) = w
        .call_text(
            "orbyn_device_fold",
            json!({ "device_id": "android:Pixel_8" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(text, crate::devices::NOT_FOLDABLE_MESSAGE);
    assert!(
        !device_calls(&w, "adb")
            .iter()
            .any(|argv| argv_ends_with(argv, &["emu", "fold"])),
        "a fold reached the emulator"
    );
    // The row is written before the call, so the failed action still has its one row.
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.fold");
    assert_eq!(row.target, "android:Pixel_8");

    // A shutdown device lists no action: power off refuses it the same way.
    let w = DeviceWorld::new(Fake::devices).await;
    let (text, error) = w
        .call_text(
            "orbyn_device_button",
            json!({ "device_id": "android:Tablet", "value": "home" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(text, crate::devices::DEVICE_NOT_BOOTED_MESSAGE);

    let w = DeviceWorld::new(Fake::devices).await;
    let (text, error) = w
        .call_text(
            "orbyn_device_power_off",
            json!({ "device_id": "android:Tablet" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(text, crate::devices::DEVICE_NOT_BOOTED_MESSAGE);
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.power_off");
    assert_eq!(row.target, "android:Tablet");

    // iOS has no rotate, back or recents.
    for (tool, args) in [
        (
            "orbyn_device_rotate",
            json!({ "device_id": "ios:UDID-1", "value": "portrait" }),
        ),
        (
            "orbyn_device_button",
            json!({ "device_id": "ios:UDID-1", "value": "back" }),
        ),
        (
            "orbyn_device_button",
            json!({ "device_id": "ios:UDID-1", "value": "recents" }),
        ),
    ] {
        let w = DeviceWorld::new(Fake::devices).await;
        let (text, error) = w.call_text(tool, args).await;
        assert!(error, "{tool}: {text}");
        assert_eq!(text, crate::devices::ACTION_UNSUPPORTED_MESSAGE, "{tool}");
    }
}

#[tokio::test]
async fn a_device_action_answers_tool_missing_when_that_platforms_tools_are_gone() {
    let w = DeviceWorld::new(Fake::android_less_devices).await;
    let (text, error) = w
        .call_text(
            "orbyn_device_power_off",
            json!({ "device_id": "android:Pixel_8" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(
        text,
        "Android tools were not found. Install Android platform-tools and the emulator."
    );
    // The row is written before the service answers its fixed copy.
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.power_off");
    assert_eq!(row.target, "android:Pixel_8");
}

#[tokio::test]
async fn a_path_like_device_id_never_reaches_a_device_action_row() {
    let w = DeviceWorld::new(Fake::devices).await;
    let (text, error) = w
        .call_text(
            "orbyn_device_power_off",
            json!({ "device_id": "/Users/alice/private.png" }),
        )
        .await;
    assert!(error, "{text}");
    assert_eq!(text, crate::devices::DEVICE_NOT_FOUND_MESSAGE);
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.power_off");
    assert_eq!(row.target, "-");
    assert_eq!(row.detail, json!({}));
    let serialized = serde_json::to_string(row).unwrap();
    assert!(!serialized.contains('/'), "{serialized}");
    assert!(!serialized.contains("alice"), "{serialized}");
    assert!(!serialized.contains("private"), "{serialized}");
}

#[tokio::test]
async fn the_device_action_row_is_written_before_the_device_answers() {
    let w = DeviceWorld::new(Fake::devices).await;
    // The fake logs the button command and then holds its answer, so the row has to be
    // there while the call is still running.
    w.fake.set("action_delay_ms", 1000);
    let core = w.core.clone();
    let binding = w.binding.clone();
    let args = json!({ "device_id": "android:Pixel_8", "value": "home" });
    let call = tokio::spawn(async move {
        core.call_orbyn_tool(&binding, "orbyn_device_button", &args)
            .await
    });
    w.fake.wait_for_call("adb", "keyevent").await;
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.button");
    assert_eq!(row.target, "android:Pixel_8");
    assert_eq!(row.detail, json!({}));
    let answer = call.await.unwrap().unwrap();
    assert_eq!(answer["ok"], true);
    // One row for the one call, not a second one after the answer.
    let audit = w.audit();
    assert_eq!(
        audit
            .iter()
            .filter(|row| row.action == "device.tool.button")
            .count(),
        1
    );
}

#[tokio::test]
async fn the_power_off_tool_asks_a_person_to_power_off_that_device() {
    let w = DeviceWorld::new(Fake::devices)
        .await
        .in_mode(PermissionMode::Supervised);
    let core = w.core.clone();
    let binding = w.binding.clone();
    let call = tokio::spawn(async move {
        core.call_orbyn_tool(
            &binding,
            "orbyn_device_power_off",
            &json!({ "device_id": "android:Pixel_8" }),
        )
        .await
    });
    let pending = wait_for_pending_approval(&w).await;
    assert_eq!(pending.len(), 1, "{pending:?}");
    let ask = &pending[0];
    assert_eq!(ask.kind, ActionKind::Tool);
    // The confirmation names the one device the call is allowed to touch.
    assert_eq!(ask.summary, "Power off device android:Pixel_8");
    assert_eq!(ask.detail, json!({ "device_id": "android:Pixel_8" }));
    // Nothing ran while the person decides.
    assert!(
        w.fake.calls().is_empty(),
        "the device was powered off before the answer"
    );
    w.core.gate.decide(&ask.id, true, false, "test").unwrap();
    let answer = call.await.unwrap().unwrap();
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["device"]["state"], "shutdown");
    let audit = w.audit();
    let row = the_one(&audit, "device.tool.power_off");
    assert_eq!(row.target, "android:Pixel_8");
}

#[test]
fn a_crew_worker_cannot_use_the_device_tools() {
    for name in device_tools() {
        assert_eq!(
            crate::crew::worker_refusal(
                name,
                &json!({ "device_id": "android:Pixel_8" }),
                "own-thread"
            ),
            Some(format!("A crew worker cannot use {name}."))
        );
    }
}
