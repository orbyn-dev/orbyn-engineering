use crate::browser::preview::{
    DEFAULT_SCROLL, MAX_SCROLL, MAX_SELECTOR_CHARS, MAX_TYPE_CHARS, MAX_WAIT_TEXT_CHARS,
    PreviewAct, PreviewAction,
};
use crate::error::CoreError;
use serde_json::{Value, json};

use super::definitions::TOOLS;
use super::guide::{
    MAX_FILE_NOTE_CHARS, MAX_GUIDE_PATH_BYTES, MAX_GUIDE_STOPS, MAX_STOP_FILES, MAX_STOP_QUESTIONS,
    MAX_STOP_SUMMARY_CHARS, MAX_STOP_TITLE_CHARS, guide_head, guide_stops,
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
