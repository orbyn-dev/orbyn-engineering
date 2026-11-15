//! The schema `crew_start` lists, and the single error a bad draft gets.

use std::sync::Arc;

use orbyn_contracts::CheckKind;
use serde_json::{Value, json};

use super::fake::stamper;
use super::harness::{Turn, world};
use crate::crew::draft::{
    ASSIGNERS, CHECK_KINDS, EFFECTS, KINDS, MODES, check_kind_is_real, example,
};
use crate::crew::{START_TOOL, tool_specs};

fn start_schema() -> Value {
    tool_specs()
        .into_iter()
        .find(|tool| tool["name"] == START_TOOL)
        .expect("crew_start is listed")
}

fn names(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

#[test]
fn the_tool_lists_the_enums_and_the_required_budget_fields() {
    let spec = start_schema();
    let brief = &spec["inputSchema"]["properties"]["brief"];
    let props = &brief["properties"];
    assert_eq!(names(&props["kind"]["enum"]), KINDS);
    assert_eq!(names(&props["allowed_effects"]["items"]["enum"]), EFFECTS);
    assert_eq!(
        names(&props["files"]["items"]["properties"]["mode"]["enum"]),
        MODES
    );
    assert_eq!(
        names(&props["engine"]["properties"]["assigned_by"]["enum"]),
        ASSIGNERS
    );
    assert_eq!(names(&props["budget"]["required"]), ["turns", "minutes"]);
    assert_eq!(props["budget"]["properties"]["minutes"]["maximum"], 1440);
    assert_eq!(props["stop_when"]["maxItems"], 16);
    let key = &spec["inputSchema"]["properties"]["answer_key"]["properties"];
    assert_eq!(
        names(&key["checks"]["items"]["properties"]["kind"]["enum"]),
        CHECK_KINDS
    );
    assert!(
        spec["description"]
            .as_str()
            .unwrap()
            .contains("never in the brief text")
    );
}

#[test]
fn every_listed_enum_value_is_a_real_one_and_the_example_parses() {
    for name in CHECK_KINDS {
        assert!(check_kind_is_real(name), "{name}");
    }
    assert_eq!(
        CHECK_KINDS.len(),
        18,
        "a new CheckKind is listed here and in the schema"
    );
    let _ = CheckKind::Command;
    for (list, field) in [(KINDS.to_vec(), "kind")] {
        for name in list {
            let mut brief = example()["brief"].clone();
            brief[field] = json!(name);
            stamper()
                .brief(&brief)
                .unwrap_or_else(|e| panic!("{name}: {}", e.message));
        }
    }
    for name in EFFECTS {
        let mut brief = example()["brief"].clone();
        brief["allowed_effects"] = json!([name]);
        stamper()
            .brief(&brief)
            .unwrap_or_else(|e| panic!("{name}: {}", e.message));
    }
    let call = example();
    let brief = stamper().brief(&call["brief"]).unwrap();
    stamper()
        .answer_key(&call["answer_key"], &brief.id)
        .unwrap();
    assert!(crate::crew::draft::problems(&call["brief"], &call["answer_key"]).is_empty());
}

#[tokio::test]
async fn a_draft_with_three_mistakes_gets_all_three_in_one_error() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let mut call = example();
    call["brief"]["files"] = json!(["infra/deploy.sh"]);
    call["brief"]["allowed_effects"] = json!(["run tests"]);
    call["brief"]["budget"] = json!({ "turns": 30 });
    let (answer, error) = w
        .start(call["brief"].clone(), call["answer_key"].clone())
        .await;
    assert!(error);
    assert_eq!(answer["code"], "invalid_object");
    let message = answer["message"].as_str().unwrap();
    for part in [
        "files[0]: a string, expected {path, mode: read|write}",
        "allowed_effects[0]: \"run tests\" is not one of read_repo",
        "budget.minutes: missing, a number 1 to 1440",
    ] {
        assert!(message.contains(part), "{part} not in {message}");
    }
}

#[test]
fn problems_in_the_answer_key_are_listed_with_the_brief_ones() {
    let mut call = example();
    call["brief"]["kind"] = json!("build");
    call["answer_key"]["checks"][1]["kind"] = json!("shell");
    call["answer_key"]["checks"][0]["params"] = json!({ "only": [] });
    let all = crate::crew::draft::problems(&call["brief"], &call["answer_key"]);
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(all[0].starts_with("kind: \"build\" is not one of plan"));
    assert!(all[2].starts_with("answer_key.checks[1].kind: \"shell\""));
    assert!(all[1].starts_with("answer_key.checks[0].params:"));
}
