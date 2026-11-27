//! The schema `crew_start` lists, and the single error a bad draft gets.

use std::sync::Arc;

use orbyn_contracts::{BriefAssigner, BriefEffect, BriefKind, CheckKind, LeaseMode};
use serde_json::{Value, json};
use ts_rs::TS;

use super::fake::stamper;
use super::harness::{Turn, world};
use crate::crew::draft::{ASSIGNERS, CHECK_KINDS, EFFECTS, KINDS, MODES, example, problems};
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

/// The variants the contract really has, read from its TypeScript declaration.
fn variants<T: TS>() -> Vec<String> {
    let decl = T::decl(&ts_rs::Config::default());
    let body = decl.split_once('=').expect("a type alias").1;
    body.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_listed_enums_match_the_contract_variant_for_variant() {
    for (name, listed, real) in [
        ("BriefKind", KINDS.to_vec(), variants::<BriefKind>()),
        ("BriefEffect", EFFECTS.to_vec(), variants::<BriefEffect>()),
        ("LeaseMode", MODES.to_vec(), variants::<LeaseMode>()),
        (
            "BriefAssigner",
            ASSIGNERS.to_vec(),
            variants::<BriefAssigner>(),
        ),
        ("CheckKind", CHECK_KINDS.to_vec(), variants::<CheckKind>()),
    ] {
        assert!(!real.is_empty(), "{name} read no variants");
        assert_eq!(listed, real, "{name}: update draft.rs");
    }
}

#[test]
fn the_example_parses_and_every_listed_value_is_accepted() {
    for name in KINDS {
        let mut brief = example()["brief"].clone();
        brief["kind"] = json!(name);
        stamper()
            .brief(&brief)
            .unwrap_or_else(|e| panic!("{name}: {}", e.message));
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
    assert!(problems(&call["brief"], &call["answer_key"]).is_empty());
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
    // The example names its checks by id, so break one by its name rather than by the place the fixture lists
    // it in.
    let checks = call["answer_key"]["checks"]
        .as_array_mut()
        .expect("the example lists its answer key's checks");
    let dry_run = checks
        .iter_mut()
        .find(|check| check["id"] == "dry-run")
        .expect("the example's dry-run check");
    dry_run["kind"] = json!("shell");
    let scope = checks
        .iter_mut()
        .find(|check| check["id"] == "scope")
        .expect("the example's scope check");
    scope["params"] = json!({ "only": [] });

    let all = crate::crew::draft::problems(&call["brief"], &call["answer_key"]);
    // The brief's problems come first, then the answer key's in its walk order, each naming the field it is
    // about: pin that order as the list of paths, then find each problem by the path it names rather than by a
    // place.
    let paths: Vec<&str> = all
        .iter()
        .map(|problem| {
            problem
                .split_once(": ")
                .expect("a field path and what is wrong")
                .0
        })
        .collect();
    assert_eq!(
        paths,
        [
            "brief.kind",
            "answer_key.checks[0].params",
            "answer_key.checks[1].kind"
        ],
        "{all:?}"
    );
    let problem = |path: &str| {
        all.iter()
            .find(|problem| problem.starts_with(path))
            .unwrap_or_else(|| panic!("no {path} problem: {all:?}"))
    };
    assert!(problem("brief.kind").starts_with("brief.kind: \"build\" is not one of plan"));
    assert!(
        problem("answer_key.checks[1].kind").starts_with("answer_key.checks[1].kind: \"shell\"")
    );
    assert!(problem("answer_key.checks[0].params").starts_with("answer_key.checks[0].params:"));
}

#[tokio::test]
async fn the_example_starts_a_task_through_the_real_path() {
    let w = world(true, Arc::new(|_, _| Turn::Hang)).await;
    let call = example();
    let (answer, error) = w
        .start(call["brief"].clone(), call["answer_key"].clone())
        .await;
    assert!(!error, "the example was refused: {answer}");
}

#[test]
fn a_string_engine_and_a_budget_of_agents_are_both_named() {
    let call = example();
    let mut brief = call["brief"].clone();
    brief["engine"] = json!("claude");
    brief["budget"] = json!({ "turns": 5, "minutes": 5, "agents": 2 });
    let all = problems(&brief, &call["answer_key"]);
    assert!(
        all.iter()
            .any(|p| p.starts_with("brief.engine: a string, expected {id, assigned_by")),
        "{all:?}"
    );
    assert!(
        all.iter()
            .any(|p| p.starts_with("brief.budget.agents: not a field; allowed: turns, minutes")),
        "{all:?}"
    );
}
