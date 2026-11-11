//! The Rust types against the protocol's own examples: every valid example reads into
//! its type and writes back to the same canonical JSON, every invalid example is refused,
//! and what the types write validates against the schemas, so a field added to a type
//! without its schema (or the reverse) fails here.

use std::collections::BTreeSet;

use orbyn_contracts::{CrewError, CrewObject, ErrorCode, canonical_json, object_id, reject_floats};
use serde_json::{Value, json};

use super::model;
use super::{example_parts, files, protocol_dir, read_json, validator};

fn examples() -> Vec<(String, String, bool, Value)> {
    files(&protocol_dir().join("schemas/examples"), ".json")
        .into_iter()
        .map(|path| {
            let (schema, valid) = example_parts(&path);
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (schema, name, valid, read_json(&path))
        })
        .collect()
}

fn type_of(schema: &str) -> String {
    schema.replace('-', "_")
}

#[test]
fn every_valid_example_round_trips_through_its_type() {
    let v = validator();
    let mut types = BTreeSet::new();
    for (schema, name, valid, value) in examples() {
        if !valid || schema == "error" {
            continue;
        }
        let object = CrewObject::parse(&value)
            .unwrap_or_else(|e| panic!("{name} should read into its type: {e}"));
        assert_eq!(
            object.type_name(),
            type_of(&schema),
            "{name} reads as the wrong type"
        );
        let back = object.to_value();
        assert_eq!(back, value, "{name} does not serialise back to itself");
        assert_eq!(
            canonical_json(&back).unwrap(),
            canonical_json(&value).unwrap(),
            "{name}: canonical JSON differs"
        );
        assert_eq!(
            object.computed_id().unwrap().as_str(),
            model::object_id(&value).unwrap(),
            "{name}: the id differs from the reference model's"
        );
        v.check(&schema, &back)
            .unwrap_or_else(|e| panic!("{name}: what the type writes fails its schema: {e}"));
        v.check("object", &back)
            .unwrap_or_else(|e| panic!("{name}: what the type writes fails object.schema: {e}"));
        types.insert(object.type_name());
    }
    assert_eq!(
        types.len(),
        13,
        "every object type has a valid example: {types:?}"
    );
}

/// The field an invalid example is wrong in: the last named segment of the pointer in
/// `invalid-examples.json`. A failure at the root (a missing field or a failed `not`)
/// names no field there, so the test names it.
fn field_of(name: &str, at: &str) -> String {
    let named = at
        .split('/')
        .rev()
        .find(|s| !s.is_empty() && s.parse::<usize>().is_err());
    if let Some(field) = named {
        return field.to_owned();
    }
    match name {
        "answer-key.invalid.no-protected-paths.json" => "protected_paths",
        "brief.invalid.no-budget.json" => "budget",
        "evidence.invalid.no-does-not-prove.json" => "does_not_prove",
        "fact.invalid.contract-without-check.json" => "check",
        "finding.invalid.no-commit.json" => "commit",
        "gate.invalid.lane-on-checks.json" | "gate.invalid.review-without-lane.json" => "lane",
        "lease.invalid.target-without-transfer.json"
        | "lease.invalid.transfer-without-target.json" => "to",
        "message.invalid.answer-without-reply.json" => "in_reply_to",
        "report.invalid.no-not-verified.json" | "result.invalid.no-not-verified.json" => {
            "not_verified"
        }
        "result.invalid.reason-on-passed.json" | "result.invalid.stopped-without-reason.json" => {
            "stop_reason"
        }
        "result.invalid.route-missing.json" => "route",
        "route-decision.invalid.assigned-without-request.json" => "requested",
        "route-decision.invalid.lane-on-builder.json"
        | "route-decision.invalid.review-without-lane.json" => "lane",
        other => panic!("{other}: name the field it is wrong in"),
    }
    .to_owned()
}

/// Every invalid example is refused as `invalid_object`, and the refusal names the field
/// `invalid-examples.json` says it is wrong in, so an incidental refusal cannot hide a
/// missing rule.
#[test]
fn every_invalid_example_is_refused_for_its_own_field() {
    let expected = read_json(&protocol_dir().join("schemas/invalid-examples.json"));
    let mut wrong = Vec::new();
    for (schema, name, valid, value) in examples() {
        if valid {
            continue;
        }
        let refusal: Option<CrewError> = if schema == "error" {
            CrewError::parse(&value).err()
        } else {
            CrewObject::parse(&value).err()
        };
        let refusal = refusal.unwrap_or_else(|| panic!("{name} should be refused"));
        let want = &expected["expected"][name.as_str()];
        let field = field_of(&name, want["at"].as_str().unwrap());
        let code = if name == "brief.invalid.wrong-version.json" {
            ErrorCode::UnsupportedVersion
        } else {
            ErrorCode::InvalidObject
        };
        if refusal.code != code {
            wrong.push(format!(
                "{name}: code {:?}: {}",
                refusal.code, refusal.message
            ));
        } else if !refusal.message.contains(&field) {
            wrong.push(format!(
                "{name}: should name `{field}`: {}",
                refusal.message
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_error_object_round_trips() {
    for (schema, name, valid, value) in examples() {
        if schema != "error" || !valid {
            continue;
        }
        let error: CrewError = serde_json::from_value(value.clone()).unwrap();
        error.validate().unwrap();
        assert_eq!(serde_json::to_value(&error).unwrap(), value, "{name}");
    }
}

#[test]
fn a_float_is_refused_even_when_it_is_whole() {
    let mut brief = read_json(&protocol_dir().join("schemas/examples/brief.valid.basic.json"));
    brief["budget"]["minutes"] = json!(30.0);
    assert_eq!(
        CrewObject::parse(&brief).unwrap_err().code,
        ErrorCode::InvalidObject
    );
    assert!(reject_floats(&brief).is_err());
    assert!(object_id(&brief).is_err(), "a float has no canonical form");
    // A float nested in a fact's value is refused too.
    let mut fact = read_json(&protocol_dir().join("schemas/examples/fact.valid.contract.json"));
    fact["value"] = json!({"ratio": 0.5});
    assert_eq!(
        CrewObject::parse(&fact).unwrap_err().code,
        ErrorCode::InvalidObject
    );
}

#[test]
fn errors_follow_the_spec_order() {
    let mut brief = read_json(&protocol_dir().join("schemas/examples/brief.valid.basic.json"));
    brief["v"] = json!(1);
    brief["budget"]["turns"] = json!(0);
    assert_eq!(
        CrewObject::parse(&brief).unwrap_err().code,
        ErrorCode::UnsupportedVersion,
        "the version is checked before the rest"
    );
    brief["v"] = json!(0);
    assert_eq!(
        CrewObject::parse(&brief).unwrap_err().code,
        ErrorCode::InvalidObject
    );
    brief["budget"]["turns"] = json!(21);
    assert_eq!(
        CrewObject::parse(&brief).unwrap_err().code,
        ErrorCode::IdMismatch,
        "the id covers the whole object, so changing a field makes it stale"
    );
    let mut unknown = brief.clone();
    unknown["type"] = json!("ledger");
    assert_eq!(
        CrewObject::parse(&unknown).unwrap_err().code,
        ErrorCode::InvalidObject
    );
}

/// H1: the brief a worker reads has no field that can hold the key, and the key is its
/// own type with its own `type`.
#[test]
fn the_brief_has_no_place_for_the_answer_key() {
    let dir = protocol_dir().join("schemas/examples");
    let brief = CrewObject::parse(&read_json(&dir.join("brief.valid.basic.json"))).unwrap();
    let key = CrewObject::parse(&read_json(&dir.join("answer-key.valid.basic.json"))).unwrap();
    let wire = brief.to_value();
    for forbidden in ["checks", "protected_paths", "answer_key"] {
        assert!(
            wire.get(forbidden).is_none(),
            "a brief carries no {forbidden}"
        );
    }
    assert_eq!(key.type_name(), "answer_key");
    assert!(matches!(key, CrewObject::AnswerKey(_)));
    // A brief with the key's fields smuggled in is refused, never ignored.
    let mut smuggled = wire.clone();
    smuggled["checks"] = key.to_value()["checks"].clone();
    assert!(CrewObject::parse(&smuggled).is_err());
}

/// H1 in the types: the union a worker reads has no answer key variant.
#[test]
fn the_worker_union_cannot_hold_an_answer_key() {
    use orbyn_contracts::WorkerObject;
    let dir = protocol_dir().join("schemas/examples");
    let key = read_json(&dir.join("answer-key.valid.basic.json"));
    assert!(serde_json::from_value::<WorkerObject>(key.clone()).is_err());
    let key = CrewObject::parse(&key).unwrap();
    assert_eq!(
        WorkerObject::try_from(key).unwrap_err().code,
        ErrorCode::AnswerKeyForbidden
    );
    let brief = CrewObject::parse(&read_json(&dir.join("brief.valid.basic.json"))).unwrap();
    let worker = WorkerObject::try_from(brief.clone()).unwrap();
    assert_eq!(CrewObject::from(worker.clone()), brief);
    assert_eq!(serde_json::to_value(&worker).unwrap(), brief.to_value());
}
