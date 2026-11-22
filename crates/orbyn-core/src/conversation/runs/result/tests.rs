use orbyn_contracts::{AgUiEvent, RunPurpose};
use serde_json::{Value, json};

use super::*;

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": { "n": { "type": "integer" } },
        "required": ["n"],
    })
}

fn request() -> ResultRequest {
    ResultRequest::from_body(Some(schema()), None)
        .unwrap()
        .unwrap()
}

fn terminal() -> ResultRequest {
    ResultRequest::from_body(None, Some(RunPurpose::TerminalProposal))
        .unwrap()
        .unwrap()
}

fn refused(schema: Value) {
    let error = ResultRequest::from_body(Some(schema), None).unwrap_err();
    assert!(
        matches!(&error, CoreError::Invalid(text) if text == SCHEMA_REFUSED),
        "{error:?}"
    );
}

fn custom(event: &AgUiEvent) -> (&str, &Value) {
    match event {
        AgUiEvent::Custom { name, value } => (name.as_str(), value),
        other => panic!("not a custom event: {other:?}"),
    }
}

#[test]
fn no_schema_and_no_purpose_asks_for_nothing() {
    assert!(ResultRequest::from_body(None, None).unwrap().is_none());
}

#[test]
fn a_valid_schema_is_accepted_and_the_two_ways_to_ask_are_exclusive() {
    assert!(
        ResultRequest::from_body(Some(schema()), None)
            .unwrap()
            .is_some()
    );
    let both = ResultRequest::from_body(Some(schema()), Some(RunPurpose::TerminalProposal));
    assert!(matches!(both, Err(CoreError::Invalid(_))));
}

#[test]
fn a_schema_that_breaks_a_rule_is_refused_with_fixed_words() {
    // Not an object.
    refused(json!(true));
    refused(json!([1]));
    // A type this draft does not know.
    refused(json!({ "type": "no-such-type" }));
    // Over 16 KiB.
    refused(json!({ "description": "x".repeat(MAX_SCHEMA_BYTES) }));
    // A remote reference, however it is spelled.
    refused(json!({ "$ref": "http://127.0.0.1:1/schema.json" }));
    refused(json!({ "properties": { "a": { "$ref": "https://example.com/a.json" } } }));
    refused(json!({ "$ref": "file:///etc/passwd" }));
    refused(json!({ "$dynamicRef": "#x" }));
    // Nesting deeper than 32.
    let mut deep = json!({ "type": "string" });
    for _ in 0..40 {
        deep = json!({ "items": deep });
    }
    refused(deep);
}

#[test]
fn a_local_reference_is_allowed() {
    let schema = json!({
        "$defs": { "n": { "type": "integer" } },
        "type": "object",
        "properties": { "n": { "$ref": "#/$defs/n" } },
    });
    let request = ResultRequest::from_body(Some(schema), None)
        .unwrap()
        .unwrap();
    assert_eq!(
        request.read("```json\n{\"n\": 3}\n```").unwrap(),
        json!({ "n": 3 })
    );
}

#[test]
fn the_last_fenced_block_wins_and_text_around_it_is_fine() {
    let request = request();
    let text = "first ```json\n{\"n\": 1}\n``` then\n```json\n{\"n\": 2}\n```";
    assert_eq!(request.read(text).unwrap(), json!({ "n": 2 }));
    assert_eq!(
        request
            .read("Done.\n```json\n{\"n\": 5}\n```\nThanks!")
            .unwrap(),
        json!({ "n": 5 })
    );
}

#[test]
fn a_bare_json_message_is_read() {
    assert_eq!(request().read("  {\"n\": 7}\n").unwrap(), json!({ "n": 7 }));
}

#[test]
fn the_failures_are_fixed_codes() {
    let request = request();
    assert_eq!(request.read("I could not do that."), Err(Reason::NoJson));
    assert_eq!(
        request.read("```json\n{oops\n```"),
        Err(Reason::NotValidJson)
    );
    assert_eq!(
        request.read("```json\n{\"n\": \"x\"}\n```"),
        Err(Reason::SchemaMismatch)
    );
    let huge = format!(
        "```json\n{{\"n\": 1, \"pad\": \"{}\"}}\n```",
        "x".repeat(MAX_RESULT_BYTES)
    );
    assert_eq!(request.read(&huge), Err(Reason::TooLarge));
}

#[test]
fn the_event_is_one_custom_event_with_the_value_or_a_reason() {
    let ok = request().events("```json\n{\"n\": 1}\n```");
    assert_eq!(ok.len(), 1);
    assert_eq!(
        custom(&ok[0]),
        (RESULT_EVENT, &json!({ "ok": true, "value": { "n": 1 } }))
    );
    let bad = request().events("nothing");
    assert_eq!(bad.len(), 1);
    assert_eq!(
        custom(&bad[0]),
        (RESULT_EVENT, &json!({ "ok": false, "reason": "no_json" }))
    );
}

fn proposal_events(command: &Value, note: Option<&str>) -> Vec<AgUiEvent> {
    let mut value = json!({ "command": command });
    if let Some(note) = note {
        value["note"] = json!(note);
    }
    terminal().events(&format!("```json\n{value}\n```"))
}

fn proposals(events: &[AgUiEvent]) -> Vec<&Value> {
    events
        .iter()
        .map(custom)
        .filter(|(name, _)| *name == PROPOSAL_EVENT)
        .map(|(_, value)| value)
        .collect()
}

#[test]
fn a_terminal_proposal_run_emits_the_result_and_the_proposal() {
    let events = proposal_events(&json!("  ls -la\t. "), Some("List files"));
    assert_eq!(events.len(), 2);
    assert_eq!(custom(&events[0]).0, RESULT_EVENT);
    assert_eq!(
        proposals(&events),
        vec![&json!({ "command": "ls -la\t.", "note": "List files" })]
    );
    // The note is optional.
    assert_eq!(
        proposals(&proposal_events(&json!("pwd"), None)),
        vec![&json!({ "command": "pwd" })]
    );
}

#[test]
fn a_proposal_that_breaks_a_rule_is_dropped_whole() {
    for command in [
        json!("echo a\necho b"),
        json!("echo a\rb"),
        json!("echo \u{1b}[31mred"),
        json!("echo \u{0}"),
        json!("x".repeat(4097)),
        json!(""),
        json!("   "),
    ] {
        let events = proposal_events(&command, None);
        assert!(proposals(&events).is_empty(), "{command}");
        // The typed result still records what the engine answered.
        assert_eq!(custom(&events[0]).0, RESULT_EVENT);
    }
    assert!(proposals(&proposal_events(&json!("ls"), Some(&"n".repeat(1025)))).is_empty());
    // At the limits it is kept.
    assert_eq!(
        proposals(&proposal_events(
            &json!("x".repeat(4096)),
            Some(&"n".repeat(1024))
        ))
        .len(),
        1
    );
}

#[test]
fn a_secret_looking_command_is_not_filtered_here() {
    // The web masks it; the service reports text only.
    let events = proposal_events(&json!("export TOKEN=sk-abcdefghijklmnop"), None);
    assert_eq!(proposals(&events).len(), 1);
}

#[test]
fn a_proposal_with_a_wrong_shape_is_a_schema_mismatch_not_a_proposal() {
    let events = terminal().events("```json\n{\"command\": 5}\n```");
    assert_eq!(events.len(), 1);
    assert_eq!(custom(&events[0]).1["reason"], "schema_mismatch");
    let extra = terminal().events("```json\n{\"command\": \"ls\", \"other\": 1}\n```");
    assert_eq!(custom(&extra[0]).1["reason"], "schema_mismatch");
}

#[test]
fn a_caller_schema_never_emits_a_proposal() {
    let schema = json!({ "type": "object" });
    let request = ResultRequest::from_body(Some(schema), None)
        .unwrap()
        .unwrap();
    let events = request.events("```json\n{\"command\": \"ls\"}\n```");
    assert!(proposals(&events).is_empty());
}

#[test]
fn the_instruction_keeps_the_persons_text_whole_and_once() {
    let prompt = "Count the files in src/.";
    let instructed = request().instructed(prompt);
    assert!(instructed.starts_with(prompt));
    assert_eq!(instructed.matches(prompt).count(), 1);
    assert!(instructed.contains("```json"));
    assert!(instructed.contains(&serde_json::to_string(&schema()).unwrap()));
}

#[test]
fn the_answer_is_the_last_finished_assistant_message_and_never_the_user_echo() {
    let mut answer = AnswerText::default();
    let events = [
        AgUiEvent::TextMessageStart {
            message_id: "u".into(),
            role: "user".into(),
        },
        AgUiEvent::TextMessageContent {
            message_id: "u".into(),
            delta: "```json\n{\"n\": 99}\n```".into(),
        },
        AgUiEvent::TextMessageEnd {
            message_id: "u".into(),
        },
        AgUiEvent::TextMessageStart {
            message_id: "a1".into(),
            role: "assistant".into(),
        },
        AgUiEvent::TextMessageContent {
            message_id: "a1".into(),
            delta: "```json\n{\"n\": 1}\n```".into(),
        },
        AgUiEvent::TextMessageEnd {
            message_id: "a1".into(),
        },
        AgUiEvent::TextMessageStart {
            message_id: "a2".into(),
            role: "assistant".into(),
        },
        AgUiEvent::TextMessageContent {
            message_id: "a2".into(),
            delta: "```json\n{\"n\": ".into(),
        },
        AgUiEvent::TextMessageContent {
            message_id: "a2".into(),
            delta: "2}\n```".into(),
        },
        AgUiEvent::TextMessageEnd {
            message_id: "a2".into(),
        },
    ];
    for event in &events {
        answer.observe(event);
    }
    let text = answer.finish().unwrap();
    assert_eq!(request().read(&text).unwrap(), json!({ "n": 2 }));
}

#[test]
fn a_run_with_no_assistant_message_has_no_answer() {
    assert!(AnswerText::default().finish().is_none());
    assert_eq!(custom(&request().events("")[0]).1["reason"], "no_json");
}

#[test]
fn a_remote_reference_is_refused_before_any_io() {
    // Nothing listens on this port: if the compiler tried to load it, the error would differ and a loader would run.
    // The refusal comes from the reference scan, and the loader itself refuses everything.
    refused(json!({ "$ref": "http://127.0.0.1:9/never" }));
    let loader = NoLoader;
    assert!(boon::UrlLoader::load(&loader, "http://127.0.0.1:9/never").is_err());
}
