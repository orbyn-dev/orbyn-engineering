//! Crew Protocol v0 conformance (docs/crew/protocol): compiles every JSON Schema,
//! validates every example, replays every data-driven case against the reference
//! model, and keeps the spec, the schemas and the cases in step. No engine runs.

mod model;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use boon::{Compiler, SchemaIndex, Schemas};
use model::{Model, R, find_secret, object_id};
use serde_json::{Value, json};

fn protocol_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/crew/protocol")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn files(dir: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().ends_with(suffix))
        .collect();
    found.sort();
    found
}

/// Every schema in `schemas/`, compiled together, addressed by file stem.
pub struct Validator {
    schemas: Schemas,
    index: HashMap<String, SchemaIndex>,
}

impl Validator {
    fn load() -> Self {
        let mut compiler = Compiler::new();
        compiler.enable_format_assertions();
        let mut urls = Vec::new();
        for path in files(&protocol_dir().join("schemas"), ".schema.json") {
            let schema = read_json(&path);
            let url = schema["$id"]
                .as_str()
                .expect("every schema has an $id")
                .to_owned();
            let stem = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .trim_end_matches(".schema.json")
                .to_owned();
            assert!(
                url.ends_with(&format!("/{stem}.schema.json")),
                "{stem}: $id must end with its file name"
            );
            compiler
                .add_resource(&url, schema)
                .unwrap_or_else(|e| panic!("{stem}: {e}"));
            urls.push((stem, url));
        }
        let mut schemas = Schemas::new();
        let mut index = HashMap::new();
        for (stem, url) in urls {
            let idx = compiler
                .compile(&url, &mut schemas)
                .unwrap_or_else(|e| panic!("{stem}: {e}"));
            index.insert(stem, idx);
        }
        Validator { schemas, index }
    }

    pub fn has(&self, stem: &str) -> bool {
        self.index.contains_key(stem)
    }

    pub fn check(&self, stem: &str, value: &Value) -> Result<(), String> {
        let idx = *self.index.get(stem).ok_or(format!("no schema {stem}"))?;
        self.schemas
            .validate(value, idx)
            .map_err(|e| format!("{e}"))
    }
}

fn validator() -> Validator {
    Validator::load()
}

#[test]
fn every_schema_compiles() {
    let v = validator();
    for stem in [
        "common",
        "check",
        "object",
        "error",
        "brief",
        "answer-key",
        "lease",
        "message",
        "fact",
        "claim",
        "report",
        "evidence",
        "finding",
        "disagreement",
        "gate",
        "result",
        "route-decision",
    ] {
        assert!(v.has(stem), "missing schema {stem}");
    }
}

/// `<schema>.<valid|invalid>.<name>.json`
fn example_parts(path: &Path) -> (String, bool) {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut parts = name.splitn(3, '.');
    let schema = parts.next().unwrap().to_owned();
    match parts.next() {
        Some("valid") => (schema, true),
        Some("invalid") => (schema, false),
        _ => panic!("{name}: examples are named <schema>.<valid|invalid>.<name>.json"),
    }
}

#[test]
fn examples_validate_or_fail_as_named() {
    let v = validator();
    let mut covered: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for path in files(&protocol_dir().join("schemas/examples"), ".json") {
        let (schema, valid) = example_parts(&path);
        let object = read_json(&path);
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let result = v.check(&schema, &object);
        let entry = covered.entry(schema.clone()).or_default();
        if valid {
            entry.0 += 1;
            assert!(
                result.is_ok(),
                "{name} should be valid: {}",
                result.unwrap_err()
            );
            if schema != "error" {
                assert_eq!(
                    object["id"],
                    json!(object_id(&object).unwrap()),
                    "{name}: stale id (run docs/crew/protocol/tools/stamp_ids.py)"
                );
                let any = v.check("object", &object);
                assert!(
                    any.is_ok(),
                    "{name} should match object.schema.json: {}",
                    any.unwrap_err()
                );
            }
            assert_eq!(find_secret(&object), None, "{name} holds a secret");
        } else {
            entry.1 += 1;
            assert!(result.is_err(), "{name} should be invalid");
        }
    }
    let objects = [
        "answer-key",
        "brief",
        "claim",
        "disagreement",
        "error",
        "evidence",
        "fact",
        "finding",
        "gate",
        "lease",
        "message",
        "report",
        "result",
        "route-decision",
    ];
    for schema in objects {
        let (valid, invalid) = covered.get(schema).copied().unwrap_or_default();
        assert!(
            valid >= 1 && invalid >= 1,
            "{schema} needs a valid and an invalid example (has {valid} and {invalid})"
        );
    }
}

fn expand(value: &mut Value) {
    match value {
        Value::String(s) => {
            // Fake secrets are built here so no secret-shaped literal sits in the repository.
            for (name, fake) in [
                ("github_token", format!("ghp_{}", "a".repeat(36))),
                ("aws_key", format!("AKIA{}", "ABCDEFGHIJKLMNOP")),
                (
                    "private_key",
                    format!("-----BEGIN RSA {}-----", "PRIVATE KEY"),
                ),
                ("bearer", format!("Bearer {}", "x".repeat(30))),
                ("assignment", format!("password = {}", "hunter2hunter2")),
            ] {
                *s = s.replace(&format!("{{{{fake:{name}}}}}"), &fake);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(expand),
        Value::Object(map) => map.values_mut().for_each(expand),
        _ => {}
    }
}

/// An object, or `{"$example": name, "mutate": {key: value}}` (top-level keys only).
fn resolve(value: &Value) -> Value {
    let Some(name) = value["$example"].as_str() else {
        return value.clone();
    };
    let mut object = read_json(&protocol_dir().join(format!("schemas/examples/{name}.json")));
    if let Some(changes) = value["mutate"].as_object() {
        for (k, v) in changes {
            object[k] = v.clone();
        }
    }
    object
}

fn outcome(result: &R<Value>) -> String {
    match result {
        Ok(_) => "ok".to_owned(),
        Err((code, _)) => (*code).to_owned(),
    }
}

fn check_text(label: &str, text: &str, want: &Value) {
    for needle in want["contains"].as_array().into_iter().flatten() {
        assert!(
            text.contains(needle.as_str().unwrap()),
            "{label}: missing {needle} in {text}"
        );
    }
    for needle in want["absent"].as_array().into_iter().flatten() {
        assert!(
            !text.contains(needle.as_str().unwrap()),
            "{label}: unexpected {needle} in {text}"
        );
    }
    for (needle, n) in want["counts"].as_object().into_iter().flatten() {
        assert_eq!(
            text.matches(needle.as_str()).count() as u64,
            n.as_u64().unwrap(),
            "{label}: count of {needle} in {text}"
        );
    }
}

fn run_case(v: &Validator, case: &Value) {
    let id = case["id"].as_str().unwrap();
    let limits = &case["limits"];
    let mut model = Model::new(
        v,
        limits["exchanges"].as_u64().unwrap_or(3) as u32,
        limits["posts"].as_u64().unwrap_or(30) as u32,
    );
    for (n, step) in case["steps"].as_array().expect("steps").iter().enumerate() {
        let label = format!("{id} step {}", n + 1);
        for _ in 0..step["repeat"].as_u64().unwrap_or(1) {
            let notices = model.lead_notices;
            let result: R<Value> = match step["op"].as_str().unwrap() {
                "call" => model.call(
                    step["tool"].as_str().unwrap(),
                    &step["caller"],
                    &step["args"],
                ),
                "validate" => model.validate(&resolve(&step["object"])),
                "wrap" => Ok(Value::String(model::wrap_message(&resolve(
                    &step["object"],
                )))),
                "transition" => model.transition(
                    step["to"].as_str().unwrap(),
                    step["evidence"].as_u64().unwrap_or(0) as usize,
                    step["person"].as_bool().unwrap_or(false),
                ),
                "verify_failed" => model.verify_failed(step["fingerprint"].as_str().unwrap()),
                "end_run" => {
                    model.end_run();
                    Ok(json!({ "ok": true }))
                }
                other => panic!("{label}: unknown op {other}"),
            };
            let expect = &step["expect"];
            let want = expect["error"].as_str().unwrap_or("ok");
            assert_eq!(
                outcome(&result),
                want,
                "{label} ({}): {:?}",
                case["title"],
                result.as_ref().err()
            );
            if let Ok(value) = &result {
                for (k, w) in expect["result_has"].as_object().into_iter().flatten() {
                    assert_eq!(&value[k], w, "{label}: result.{k}");
                }
                let text = value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string());
                check_text(&label, &text, &expect["result_text"]);
                if let Some(text) = value["delivered_as"].as_str() {
                    check_text(&label, text, &expect["delivered_text"]);
                }
            }
            if let Some(state) = expect["task_state"].as_str() {
                assert_eq!(model.task_state(), Some(state), "{label}: task state");
            }
            if let Some(told) = expect["lead_notified"].as_bool() {
                assert_eq!(model.lead_notices > notices, told, "{label}: lead notified");
            }
            if let Some(ended) = expect["run_ended"].as_bool() {
                assert_eq!(model.ended(), ended, "{label}: run ended");
            }
        }
    }
}

/// Replaces every `{"$use": name, "set": {...}}` with the named fixture, top-level keys overridden.
fn resolve_uses(value: &mut Value, fixtures: &Value) {
    if let Some(name) = value["$use"].as_str() {
        let mut fixture = fixtures[name].clone();
        assert!(!fixture.is_null(), "unknown fixture {name}");
        for (k, v) in value["set"].as_object().into_iter().flatten() {
            fixture[k] = v.clone();
        }
        *value = fixture;
        return;
    }
    match value {
        Value::Array(items) => items.iter_mut().for_each(|v| resolve_uses(v, fixtures)),
        Value::Object(map) => map.values_mut().for_each(|v| resolve_uses(v, fixtures)),
        _ => {}
    }
}

fn load_cases() -> Vec<(String, Value)> {
    let fixtures = read_json(&protocol_dir().join("conformance/fixtures.json"))["fixtures"].clone();
    let mut cases = Vec::new();
    for path in files(&protocol_dir().join("conformance/cases"), ".json") {
        let mut suite = read_json(&path);
        expand(&mut suite);
        resolve_uses(&mut suite, &fixtures);
        for case in suite["cases"].as_array().expect("cases").iter() {
            cases.push((
                path.file_name().unwrap().to_string_lossy().into_owned(),
                case.clone(),
            ));
        }
    }
    cases
}

#[test]
fn conformance_cases_pass_against_the_reference_model() {
    let v = validator();
    let cases = load_cases();
    assert!(
        cases.len() >= 30,
        "expected at least 30 cases, found {}",
        cases.len()
    );
    let mut seen = BTreeSet::new();
    for (_, case) in &cases {
        assert!(
            seen.insert(case["id"].as_str().unwrap().to_owned()),
            "duplicate case id {}",
            case["id"]
        );
        run_case(&v, case);
    }
}

fn spec() -> String {
    fs::read_to_string(protocol_dir().join("SPEC.md")).unwrap()
}

#[test]
fn spec_error_codes_match_the_error_schema_and_the_cases() {
    let schema = read_json(&protocol_dir().join("schemas/error.schema.json"));
    let codes: BTreeSet<String> = schema["properties"]["code"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    let spec = spec();
    for code in &codes {
        assert!(
            spec.contains(&format!("| `{code}` |")),
            "SPEC.md has no table row for error code {code}"
        );
    }
    let mut used = BTreeSet::new();
    for (_, case) in load_cases() {
        for step in case["steps"].as_array().unwrap() {
            if let Some(code) = step["expect"]["error"].as_str() {
                assert!(
                    codes.contains(code),
                    "case {} expects unknown code {code}",
                    case["id"]
                );
                used.insert(code.to_owned());
            }
        }
    }
    // Codes the router, the join flow and transfers will exercise once those exist.
    let later: BTreeSet<&str> = ["invalid_token", "unknown_member"].into_iter().collect();
    for code in &codes {
        assert!(
            used.contains(code) || later.contains(code.as_str()),
            "no case expects error code {code}"
        );
    }
}

#[test]
fn every_hard_rule_has_a_case() {
    let spec = spec();
    let rules: BTreeSet<String> = spec
        .lines()
        .filter_map(|l| {
            l.strip_prefix("| ")?
                .split(" |")
                .next()
                .filter(|c| {
                    c.starts_with('H') && c[1..].chars().all(|d| d.is_ascii_digit()) && c.len() > 1
                })
                .map(str::to_owned)
        })
        .collect();
    assert!(
        rules.len() >= 10,
        "SPEC.md lists too few hard rules: {rules:?}"
    );
    let mut covered = BTreeSet::new();
    for (_, case) in load_cases() {
        for rule in case["rules"]
            .as_array()
            .expect("every case names the rules it covers")
        {
            covered.insert(rule.as_str().unwrap().to_owned());
        }
    }
    for rule in &rules {
        assert!(
            covered.contains(rule),
            "hard rule {rule} has no conformance case"
        );
    }
    for rule in &covered {
        assert!(
            rules.contains(rule),
            "a case names {rule}, which SPEC.md does not define"
        );
    }
}
