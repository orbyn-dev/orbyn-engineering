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

#[test]
fn every_invalid_example_is_refused() {
    for (schema, name, valid, value) in examples() {
        if valid {
            continue;
        }
        let refusal: Option<CrewError> = if schema == "error" {
            serde_json::from_value::<CrewError>(value.clone())
                .map_err(|e| CrewError::new(ErrorCode::InvalidObject, e.to_string()))
                .and_then(|e| e.validate().map(|()| e))
                .err()
        } else {
            CrewObject::parse(&value).err()
        };
        let refusal = refusal.unwrap_or_else(|| panic!("{name} should be refused"));
        // The example's id is not what refuses it: the object is wrong in itself.
        assert_ne!(
            refusal.code,
            ErrorCode::IdMismatch,
            "{name} is refused only for its id: {refusal}"
        );
    }
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
