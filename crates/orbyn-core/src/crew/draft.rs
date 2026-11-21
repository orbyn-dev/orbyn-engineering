//! What `crew_start` tells a lead about its drafts: the JSON Schema the tool lists and, when a draft
//! does not parse, every problem at once. serde stops at the first one, so a lead that learns the
//! shape one error at a time needs a call per mistake. The walk here only reads the raw JSON; it
//! never accepts anything the real types refuse (the real parse still runs first and last).

use orbyn_contracts::{
    CrewCheck, CrewHash, CrewId, CrewLongText, CrewPath, CrewPathPattern, CrewShortText, CrewToken,
};
use serde_json::{Map, Value, json};

pub(super) const KINDS: [&str; 7] = [
    "plan",
    "implement",
    "review",
    "test",
    "research",
    "ui",
    "docs",
];
pub(super) const EFFECTS: [&str; 6] = [
    "read_repo",
    "write_leased",
    "run_checks",
    "run_commands",
    "network",
    "install_dependencies",
];
pub(super) const MODES: [&str; 2] = ["read", "write"];
pub(super) const ASSIGNERS: [&str; 2] = ["person", "lead"];
pub(super) const CHECK_KINDS: [&str; 18] = [
    "command",
    "test",
    "lint",
    "typecheck",
    "custom_script",
    "file_scope",
    "diff_size",
    "schema",
    "http_probe",
    "preview",
    "visual_compare",
    "repo_required",
    "manual",
    "screenshot_only",
    "accessibility_audit",
    "performance_budget",
    "secret_scan",
    "licence_scan",
];

/// The core adds these; a draft does not carry them.
const STAMPED: [&str; 7] = [
    "type",
    "v",
    "id",
    "crew_id",
    "author",
    "created_at",
    "run_id",
];

/// One complete, valid `crew_start` call. A test parses it, so it stays true.
pub(super) fn example() -> Value {
    json!({
        "brief": {
            "kind": "implement",
            "outcome": "Add a --dry-run flag to scripts/deploy.sh that prints the steps and changes nothing.",
            "inputs": [{ "kind": "path", "path": "scripts/deploy.sh" }],
            "files": [
                { "path": "scripts/deploy.sh", "mode": "write" },
                { "path": "docs/deploy.md", "mode": "write" }
            ],
            "allowed_effects": ["read_repo", "write_leased", "run_checks"],
            "stop_when": ["The flag works and docs/deploy.md mentions it."],
            "budget": { "turns": 30, "minutes": 20 }
        },
        "answer_key": {
            "checks": [
                { "id": "scope", "kind": "file_scope",
                  "params": { "only": ["scripts/deploy.sh", "docs/deploy.md"] },
                  "proves": "Only the leased files changed." },
                { "id": "dry-run", "kind": "command",
                  "params": { "run": "sh scripts/deploy.sh --dry-run" },
                  "proves": "The dry run exits 0.",
                  "does_not_prove": "That a real deploy works." }
            ],
            "protected_paths": []
        }
    })
}

fn string_array(items: &[&str]) -> Value {
    json!({ "type": "string", "enum": items })
}

/// The JSON Schema of the `brief` draft.
pub(super) fn brief_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "outcome", "inputs", "files", "allowed_effects", "stop_when", "budget"],
        "properties": {
            "kind": string_array(&KINDS),
            "outcome": { "type": "string", "minLength": 1, "maxLength": 4000 },
            "inputs": {
                "type": "array", "maxItems": 64,
                "items": { "anyOf": [
                    { "type": "object", "additionalProperties": false, "required": ["kind", "text"],
                      "properties": { "kind": { "enum": ["text"] },
                          "text": { "type": "string", "minLength": 1, "maxLength": 8000 } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "path"],
                      "properties": { "kind": { "enum": ["path"] },
                          "path": { "type": "string", "description": "repo-relative, no leading slash or .." } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "ref"],
                      "properties": { "kind": { "enum": ["ref"] },
                          "ref": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" } } }
                ] }
            },
            "files": {
                "type": "array", "maxItems": 256,
                "description": "The files the worker may touch. A path, or a directory ending in /**.",
                "items": { "type": "object", "additionalProperties": false, "required": ["path", "mode"],
                    "properties": { "path": { "type": "string" }, "mode": string_array(&MODES) } }
            },
            "allowed_effects": {
                "type": "array", "uniqueItems": true, "items": string_array(&EFFECTS)
            },
            "stop_when": {
                "type": "array", "minItems": 1, "maxItems": 16,
                "items": { "type": "string", "minLength": 1, "maxLength": 1000 }
            },
            "budget": {
                "type": "object", "additionalProperties": false, "required": ["turns", "minutes"],
                "properties": {
                    "turns": { "type": "integer", "minimum": 1, "maximum": 1000 },
                    "minutes": { "type": "integer", "minimum": 1, "maximum": 1440 }
                }
            },
            "needs": {
                "type": "array", "maxItems": 16, "uniqueItems": true,
                "items": { "type": "string", "description": "a lowercase capability such as image or mcp" }
            },
            "engine": {
                "type": "object", "additionalProperties": false, "required": ["id", "assigned_by"],
                "properties": {
                    "id": { "type": "string", "description": "a lowercase engine id" },
                    "model": { "type": "string" },
                    "assigned_by": string_array(&ASSIGNERS),
                    "tag": { "type": "string", "minLength": 1, "maxLength": 1000 }
                }
            },
            "refs": {
                "type": "array", "maxItems": 64,
                "items": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" }
            }
        }
    })
}

/// The JSON Schema of the `answer_key` draft.
pub(super) fn answer_key_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["checks", "protected_paths"],
        "properties": {
            "checks": {
                "type": "array", "minItems": 1,
                "items": {
                    "type": "object", "additionalProperties": false,
                    "required": ["id", "kind", "params", "proves"],
                    "properties": {
                        "id": { "type": "string", "description": "letters, digits, . _ : -, up to 128" },
                        "kind": string_array(&CHECK_KINDS),
                        "params": {
                            "type": "object",
                            "description": "By kind. command, test, lint, typecheck, custom_script: {run, expect_exit?}. \
                                file_scope: {only: [path]}. diff_size: {max_files?, max_lines?}. \
                                schema: {schema, instance}: one self-contained JSON Schema and the JSON instance to validate against it. \
                                http_probe: {url (loopback), expect_status}. \
                                preview, screenshot_only, accessibility_audit: {url (loopback), screenshot?}. \
                                visual_compare: {frame, tolerance_permille}. performance_budget: {metric, max}. \
                                repo_required: {name?, note?}: runs the repository's own checks (.orbyn/crew.json checks[].run). \
                                manual: {name?, note?}. secret_scan: {}: the changed files, for recognised secrets. licence_scan: {}."
                        },
                        "proves": { "type": "string", "minLength": 1, "maxLength": 1000 },
                        "does_not_prove": { "type": "string", "minLength": 1, "maxLength": 1000 }
                    }
                }
            },
            "protected_paths": {
                "type": "array", "maxItems": 256,
                "description": "Files the checks rely on that the worker must not change; never a file the brief leases for write.",
                "items": { "type": "string" }
            }
        }
    })
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

struct Walk {
    out: Vec<String>,
}

impl Walk {
    fn add(&mut self, path: &str, message: impl std::fmt::Display) {
        self.out.push(format!("{path}: {message}"));
    }

    /// The object at `path`, with unknown fields and carried envelope fields reported.
    fn object<'a>(
        &mut self,
        path: &str,
        value: &'a Value,
        known: &[&str],
        shape: &str,
    ) -> Option<&'a Map<String, Value>> {
        let Some(map) = value.as_object() else {
            self.add(path, format!("{}, expected {shape}", kind_of(value)));
            return None;
        };
        for key in map.keys() {
            let at = join(path, key);
            if path.is_empty() && (STAMPED.contains(&key.as_str()) || key == "brief") {
                self.add(&at, "the core stamps this field; a draft does not carry it");
            } else if !known.contains(&key.as_str()) {
                self.add(&at, format!("not a field; allowed: {}", known.join(", ")));
            }
        }
        Some(map)
    }

    fn field<'a>(
        &mut self,
        path: &str,
        map: &'a Map<String, Value>,
        key: &str,
        hint: &str,
    ) -> Option<&'a Value> {
        let found = map.get(key).filter(|v| !v.is_null());
        if found.is_none() {
            self.add(&join(path, key), format!("missing, {hint}"));
        }
        found
    }

    fn one_of(&mut self, path: &str, value: &Value, allowed: &[&str]) {
        match value.as_str() {
            Some(text) if allowed.contains(&text) => {}
            Some(text) => self.add(
                path,
                format!("{text:?} is not one of {}", allowed.join(", ")),
            ),
            None => self.add(
                path,
                format!("{}, expected one of {}", kind_of(value), allowed.join(", ")),
            ),
        }
    }

    fn text(
        &mut self,
        path: &str,
        value: &Value,
        rule: impl Fn(&str) -> Result<(), String>,
        hint: &str,
    ) {
        match value.as_str() {
            Some(text) => {
                if let Err(why) = rule(text) {
                    self.add(path, why);
                }
            }
            None => self.add(path, format!("{}, expected {hint}", kind_of(value))),
        }
    }

    fn list<'a>(
        &mut self,
        path: &str,
        value: &'a Value,
        min: usize,
        max: usize,
        hint: &str,
    ) -> Option<&'a Vec<Value>> {
        let Some(items) = value.as_array() else {
            self.add(path, format!("{}, expected {hint}", kind_of(value)));
            return None;
        };
        if !(min..=max).contains(&items.len()) {
            self.add(
                path,
                format!("{} items, expected {min} to {max}", items.len()),
            );
        }
        Some(items)
    }

    fn number(&mut self, path: &str, value: &Value, min: u64, max: u64) {
        match value.as_u64() {
            Some(n) if (min..=max).contains(&n) => {}
            Some(n) => self.add(
                path,
                format!("{n} is out of range, a number {min} to {max}"),
            ),
            None => self.add(
                path,
                format!("{}, expected a whole number {min} to {max}", kind_of(value)),
            ),
        }
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

fn err<T>(check: fn(String) -> Result<T, String>) -> impl Fn(&str) -> Result<(), String> {
    move |text| check(text.to_owned()).map(|_| ())
}

fn walk_brief(w: &mut Walk, draft: &Value) {
    const KNOWN: [&str; 10] = [
        "kind",
        "outcome",
        "inputs",
        "files",
        "allowed_effects",
        "stop_when",
        "budget",
        "needs",
        "engine",
        "refs",
    ];
    let shape = "an object {kind, outcome, inputs, files, allowed_effects, stop_when, budget}";
    if !draft.is_object() {
        w.add("brief", format!("{}, expected {shape}", kind_of(draft)));
        return;
    }
    let Some(map) = w.object("", draft, &KNOWN, shape) else {
        return;
    };
    let at = |key: &str| key.to_owned();
    if let Some(v) = w.field("", map, "kind", &format!("one of {}", KINDS.join(", "))) {
        w.one_of(&at("kind"), v, &KINDS);
    }
    if let Some(v) = w.field("", map, "outcome", "a string 1 to 4000 characters") {
        match v.as_str() {
            Some(t) if (1..=4000).contains(&t.chars().count()) => {}
            Some(t) => w.add(
                "outcome",
                format!("{} characters, expected 1 to 4000", t.chars().count()),
            ),
            None => w.add(
                "outcome",
                format!("{}, expected a string 1 to 4000 characters", kind_of(v)),
            ),
        }
    }
    if let Some(v) = w.field("", map, "inputs", "an array, [] if none") {
        let hint = "an array of {kind: text, text} | {kind: path, path} | {kind: ref, ref}";
        for (i, item) in w
            .list("inputs", v, 0, 64, hint)
            .into_iter()
            .flatten()
            .enumerate()
        {
            walk_input(w, &format!("inputs[{i}]"), item);
        }
    }
    if let Some(v) = w.field(
        "",
        map,
        "files",
        "an array of {path, mode: read|write}, [] if none",
    ) {
        let hint = "an array of {path, mode: read|write}";
        for (i, item) in w
            .list("files", v, 0, 256, hint)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let path = format!("files[{i}]");
            let Some(f) = w.object(&path, item, &["path", "mode"], "{path, mode: read|write}")
            else {
                continue;
            };
            if let Some(v) = w.field(&path, f, "path", "a path, or a directory ending in /**") {
                w.text(
                    &format!("{path}.path"),
                    v,
                    err(CrewPathPattern::new),
                    "a path string",
                );
            }
            if let Some(v) = w.field(&path, f, "mode", "read or write") {
                w.one_of(&format!("{path}.mode"), v, &MODES);
            }
        }
    }
    if let Some(v) = w.field(
        "",
        map,
        "allowed_effects",
        &format!("an array of {}", EFFECTS.join(", ")),
    ) {
        let hint = format!("an array of {}", EFFECTS.join(", "));
        let items = w.list("allowed_effects", v, 0, usize::MAX, &hint);
        for (i, item) in items.into_iter().flatten().enumerate() {
            w.one_of(&format!("allowed_effects[{i}]"), item, &EFFECTS);
        }
    }
    if let Some(v) = w.field("", map, "stop_when", "an array of 1 to 16 short strings") {
        let items = w.list("stop_when", v, 1, 16, "an array of 1 to 16 short strings");
        for (i, item) in items.into_iter().flatten().enumerate() {
            w.text(
                &format!("stop_when[{i}]"),
                item,
                err(CrewShortText::new),
                "a string 1 to 1000 characters",
            );
        }
    }
    if let Some(v) = w.field("", map, "budget", "{turns: 1 to 1000, minutes: 1 to 1440}")
        && let Some(b) = w.object("budget", v, &["turns", "minutes"], "{turns, minutes}")
    {
        if let Some(v) = w.field("budget", b, "turns", "a number 1 to 1000") {
            w.number("budget.turns", v, 1, 1000);
        }
        if let Some(v) = w.field("budget", b, "minutes", "a number 1 to 1440") {
            w.number("budget.minutes", v, 1, 1440);
        }
    }
    if let Some(v) = map.get("needs").filter(|v| !v.is_null()) {
        for (i, item) in w
            .list("needs", v, 0, 16, "an array of lowercase words")
            .into_iter()
            .flatten()
            .enumerate()
        {
            w.text(
                &format!("needs[{i}]"),
                item,
                err(CrewToken::new),
                "a lowercase word",
            );
        }
    }
    if let Some(v) = map.get("engine").filter(|v| !v.is_null()) {
        let shape = "{id, assigned_by: person|lead, model?, tag?}";
        if let Some(e) = w.object("engine", v, &["id", "model", "assigned_by", "tag"], shape) {
            if let Some(v) = w.field("engine", e, "id", "a lowercase engine id") {
                w.text("engine.id", v, err(CrewToken::new), "a lowercase engine id");
            }
            if let Some(v) = e.get("model").filter(|v| !v.is_null()) {
                w.text(
                    "engine.model",
                    v,
                    err(CrewToken::new),
                    "a lowercase model id",
                );
            }
            if let Some(v) = w.field("engine", e, "assigned_by", "person or lead") {
                w.one_of("engine.assigned_by", v, &ASSIGNERS);
            }
            if let Some(v) = e.get("tag").filter(|v| !v.is_null()) {
                w.text("engine.tag", v, err(CrewShortText::new), "a string");
            }
        }
    }
    if let Some(v) = map.get("refs").filter(|v| !v.is_null()) {
        for (i, item) in w
            .list("refs", v, 0, 64, "an array of sha256 hashes")
            .into_iter()
            .flatten()
            .enumerate()
        {
            w.text(
                &format!("refs[{i}]"),
                item,
                err(CrewHash::new),
                "a sha256 hash",
            );
        }
    }
}

type Rule = Box<dyn Fn(&str) -> Result<(), String>>;

fn walk_input(w: &mut Walk, path: &str, item: &Value) {
    let shape = "{kind: text, text} | {kind: path, path} | {kind: ref, ref}";
    let Some(map) = item.as_object() else {
        w.add(path, format!("{}, expected {shape}", kind_of(item)));
        return;
    };
    let kind = map.get("kind").and_then(Value::as_str);
    let (field, rule): (&str, Rule) = match kind {
        Some("text") => ("text", Box::new(err(CrewLongText::new))),
        Some("path") => ("path", Box::new(err(CrewPath::new))),
        Some("ref") => ("ref", Box::new(err(CrewHash::new))),
        _ => {
            w.add(
                &format!("{path}.kind"),
                format!("expected one of text, path, ref, as in {shape}"),
            );
            return;
        }
    };
    w.object(path, item, &["kind", field], shape);
    match map.get(field) {
        Some(v) => w.text(&format!("{path}.{field}"), v, rule, "a string"),
        None => w.add(&format!("{path}.{field}"), "missing"),
    }
}

fn walk_key(w: &mut Walk, draft: &Value) {
    let shape = "an object {checks, protected_paths}";
    if !draft.is_object() {
        w.add(
            "answer_key",
            format!("{}, expected {shape}", kind_of(draft)),
        );
        return;
    }
    let Some(map) = w.object("", draft, &["checks", "protected_paths"], shape) else {
        return;
    };
    if let Some(v) = w.field("", map, "checks", "an array of {id, kind, params, proves}") {
        let hint = "an array of {id, kind, params, proves, does_not_prove?}";
        for (i, item) in w
            .list("checks", v, 1, usize::MAX, hint)
            .into_iter()
            .flatten()
            .enumerate()
        {
            walk_check(w, &format!("checks[{i}]"), item);
        }
    }
    if let Some(v) = w.field(
        "",
        map,
        "protected_paths",
        "an array of paths the checks run or read, [] if none",
    ) {
        let items = w.list("protected_paths", v, 0, 256, "an array of paths");
        for (i, item) in items.into_iter().flatten().enumerate() {
            w.text(
                &format!("protected_paths[{i}]"),
                item,
                err(CrewPathPattern::new),
                "a path string",
            );
        }
    }
}

fn walk_check(w: &mut Walk, path: &str, item: &Value) {
    let shape = "{id, kind, params, proves, does_not_prove?}";
    let known = ["id", "kind", "params", "proves", "does_not_prove"];
    let Some(c) = w.object(path, item, &known, shape) else {
        return;
    };
    let before = w.out.len();
    if let Some(v) = w.field(path, c, "id", "a short id such as scope or run-tests") {
        w.text(&format!("{path}.id"), v, err(CrewId::new), "a string id");
    }
    if let Some(v) = w.field(
        path,
        c,
        "kind",
        &format!("one of {}", CHECK_KINDS.join(", ")),
    ) {
        w.one_of(&format!("{path}.kind"), v, &CHECK_KINDS);
    }
    match c.get("params") {
        Some(v) if v.is_object() => {}
        Some(v) => w.add(
            &format!("{path}.params"),
            format!("{}, expected an object shaped by kind", kind_of(v)),
        ),
        None => w.add(
            &format!("{path}.params"),
            "missing, an object shaped by kind",
        ),
    }
    if let Some(v) = w.field(path, c, "proves", "a string saying what the check proves") {
        w.text(
            &format!("{path}.proves"),
            v,
            err(CrewShortText::new),
            "a string",
        );
    }
    if let Some(v) = c.get("does_not_prove").filter(|v| !v.is_null()) {
        w.text(
            &format!("{path}.does_not_prove"),
            v,
            err(CrewShortText::new),
            "a string",
        );
    }
    if w.out.len() == before {
        // The fields are sound; the params may not fit the kind.
        match serde_json::from_value::<CrewCheck>(item.clone()) {
            Ok(check) => {
                if let Err(why) = check.validate() {
                    w.add(&format!("{path}.params"), why.message);
                }
            }
            Err(why) => w.add(path, why),
        }
    }
}

/// Every problem in the two drafts, each as `field path: what is wrong, what is allowed`. Empty when
/// the walk finds none.
pub(super) fn problems(brief: &Value, key: &Value) -> Vec<String> {
    let mut walk = Walk { out: Vec::new() };
    walk_brief(&mut walk, brief);
    let brief_count = walk.out.len();
    for line in &mut walk.out {
        if !line.starts_with("brief") {
            *line = format!("brief.{line}");
        }
    }
    walk_key(&mut walk, key);
    for line in &mut walk.out[brief_count..] {
        if !line.starts_with("answer_key") {
            *line = format!("answer_key.{line}");
        }
    }
    walk.out
}
