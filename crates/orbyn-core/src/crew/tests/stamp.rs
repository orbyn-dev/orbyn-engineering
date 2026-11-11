//! Drafts, the secret scan and the answer-key leak check at the edge of the router.

use orbyn_contracts::{AnswerKey, CrewObject, ErrorCode};
use serde_json::{Value, json};

use super::fake::*;
use crate::crew::{find_secret, key_needles, refuse_leak, refuse_secret};

fn code(error: Result<impl std::fmt::Debug, orbyn_contracts::CrewError>) -> ErrorCode {
    error.unwrap_err().code
}

#[test]
fn a_draft_is_stamped_into_a_brief_that_parses_back_with_its_hash() {
    let brief = brief(json!({}));
    assert_eq!(brief.author.member.as_ref().unwrap().as_str(), "lead-01");
    let value = CrewObject::Brief(brief.clone()).to_value();
    assert!(matches!(CrewObject::parse(&value), Ok(CrewObject::Brief(b)) if b == brief));
}

#[test]
fn a_draft_that_carries_a_field_the_core_stamps_is_refused() {
    for field in [
        "id",
        "crew_id",
        "author",
        "created_at",
        "v",
        "type",
        "run_id",
    ] {
        let mut draft = draft(json!({}));
        draft[field] = json!("forged");
        assert_eq!(
            code(stamper().brief(&draft)),
            ErrorCode::InvalidObject,
            "{field}"
        );
    }
}

#[test]
fn a_draft_with_an_answer_key_field_a_float_or_a_bad_shape_is_refused() {
    let key = draft(json!({ "answer_key": { "checks": [] } }));
    assert_eq!(code(stamper().brief(&key)), ErrorCode::InvalidObject);
    let float = draft(json!({ "budget": { "turns": 20, "minutes": 30.0 } }));
    assert_eq!(code(stamper().brief(&float)), ErrorCode::InvalidObject);
    let zero = draft(json!({ "budget": { "turns": 0, "minutes": 30 } }));
    assert_eq!(code(stamper().brief(&zero)), ErrorCode::InvalidObject);
    let kind = draft(json!({ "kind": "deploy" }));
    assert_eq!(code(stamper().brief(&kind)), ErrorCode::InvalidObject);
    assert_eq!(code(stamper().brief(&json!([]))), ErrorCode::InvalidObject);
}

#[test]
fn the_secret_scan_names_a_class_and_never_the_match() {
    let secrets = [
        "sk-abcdefghijklmnopqrstuvwxyz0123",
        "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
        "AKIAABCDEFGHIJKLMNOP",
        "Bearer abcdefghijklmnopqrstuvwxyz",
        "password = hunter2hunter2",
        "-----BEGIN RSA PRIVATE KEY-----",
    ];
    for secret in secrets {
        let value = json!({ "outcome": format!("use {secret} to log in") });
        assert!(find_secret(&value).is_some(), "{secret}");
        let error = refuse_secret(&value).unwrap_err();
        assert_eq!(error.code, ErrorCode::SecretDetected);
        assert!(!error.message.contains(secret), "{secret}");
    }
    // A key counts too, and plain text does not.
    assert!(find_secret(&json!({ "api_key": "abcdefgh1234" })).is_some());
    assert!(find_secret(&json!({ "outcome": "Add a parse_duration function." })).is_none());
}

fn key() -> AnswerKey {
    let text = include_str!(
        "../../../../../docs/crew/protocol/schemas/examples/answer-key.valid.basic.json"
    );
    match CrewObject::parse(&serde_json::from_str::<Value>(text).unwrap()).unwrap() {
        CrewObject::AnswerKey(key) => key,
        _ => unreachable!(),
    }
}

#[test]
fn a_decision_never_holds_the_answer_key_and_the_check_catches_it_if_it_did() {
    let key = key();
    let needles = key_needles(&key);
    assert!(needles.iter().any(|n| n == "cargo test -p crew-demo"));
    assert!(needles.contains(&key.id.as_str().to_owned()));

    let decision = routed(
        &brief(json!({})),
        &[engine("claude"), engine("codex")],
        &Ask::default(),
    );
    let value = CrewObject::RouteDecision(decision).to_value();
    assert!(refuse_leak(&value, &needles).is_ok());

    // An engine whose model id was set to a check command would carry it into a reason.
    let mut odd = engine("zed");
    odd.models.as_mut().unwrap()[0].id = "cargo test -p crew-demo".into();
    let leaky = routed(&brief(json!({})), &[odd], &Ask::default());
    let value = CrewObject::RouteDecision(leaky).to_value();
    assert_eq!(
        refuse_leak(&value, &needles).unwrap_err().code,
        ErrorCode::AnswerKeyForbidden
    );
}
