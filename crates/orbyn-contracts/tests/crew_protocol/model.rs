//! The reference model the conformance cases run against at slice 0: the content
//! address, the secret scanner, the leak check and the invariants that need no router
//! and no running engine. Slice 3 and 4 run the same cases against the real bus.

use std::collections::HashMap;

use regex::Regex;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::Validator;

/// A refusal: the protocol error code and a human detail.
pub type Fail = (&'static str, String);
pub type R<T> = Result<T, Fail>;

fn fail<T>(code: &'static str, detail: impl Into<String>) -> R<T> {
    Err((code, detail.into()))
}

const ENVELOPE_KEYS: [&str; 7] = [
    "type",
    "v",
    "id",
    "crew_id",
    "author",
    "created_at",
    "run_id",
];
const MEMBER_TOOLS: [&str; 11] = [
    "crew.brief.read",
    "crew.lease.request",
    "crew.lease.release",
    "crew.post",
    "crew.ask",
    "crew.fact.read",
    "crew.fact.write",
    "crew.claim",
    "crew.finding",
    "crew.report",
    "crew.start",
];
const CORE_TOOLS: [&str; 3] = ["crew.verify", "crew.evidence.write", "crew.gate.write"];

/// The canonical form (SPEC.md, "Ids"): compact JSON, keys sorted by UTF-16 code unit,
/// integers only.
pub fn canonical(value: &Value, out: &mut String) -> Result<(), String> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) => out.push_str(&i.to_string()),
            (None, Some(u)) => out.push_str(&u.to_string()),
            _ => return Err(format!("floating-point number {n}")),
        },
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap()),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap());
                out.push(':');
                canonical(&map[key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

/// `sha256:` plus the hash of the canonical object without its `id`.
pub fn object_id(object: &Value) -> Result<String, String> {
    let mut body = object.clone();
    body.as_object_mut().ok_or("not an object")?.remove("id");
    let mut text = String::new();
    canonical(&body, &mut text)?;
    let digest = Sha256::digest(text.as_bytes());
    Ok(format!(
        "sha256:{}",
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

/// The minimum secret classes every implementation must refuse (SPEC.md, "No secrets").
fn secret_patterns() -> Vec<(&'static str, Regex)> {
    [
        ("private key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
        ("api key", r"\bsk-[A-Za-z0-9_-]{20,}"),
        ("github token", r"\bgh[pousr]_[A-Za-z0-9]{36,}"),
        ("github token", r"\bgithub_pat_[A-Za-z0-9_]{22,}"),
        ("aws key", r"\bAKIA[0-9A-Z]{16}\b"),
        ("slack token", r"\bxox[abprs]-[A-Za-z0-9-]{10,}"),
        ("bearer token", r"(?i)\bbearer\s+[A-Za-z0-9._~+/-]{20,}=*"),
        (
            "jwt",
            r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
        ),
        (
            "credential assignment",
            r#"(?i)\b(password|passwd|secret|api[_-]?key|token)["']?\s*[:=]\s*["']?[^\s"']{8,}"#,
        ),
    ]
    .into_iter()
    .map(|(name, pattern)| (name, Regex::new(pattern).unwrap()))
    .collect()
}

fn walk_strings(value: &Value, visit: &mut dyn FnMut(&str)) {
    match value {
        Value::String(s) => visit(s),
        Value::Array(items) => items.iter().for_each(|v| walk_strings(v, visit)),
        Value::Object(map) => {
            for (k, v) in map {
                visit(k);
                if let Value::String(text) = v {
                    // A field named like a credential and holding a long value reads as an assignment.
                    visit(&format!("{k} = {text}"));
                }
                walk_strings(v, visit);
            }
        }
        _ => {}
    }
}

/// Names the first secret class found in any string or key of the object.
pub fn find_secret(value: &Value) -> Option<&'static str> {
    let patterns = secret_patterns();
    let mut found = None;
    walk_strings(value, &mut |s| {
        if found.is_none() {
            found = patterns
                .iter()
                .find(|(_, re)| re.is_match(s))
                .map(|(name, _)| *name);
        }
    });
    found
}

/// The data wrapper a recipient sees (SPEC.md, "Messages are data").
pub fn wrap_message(message: &Value) -> String {
    let who = message["author"]["member"]
        .as_str()
        .or(message["author"]["role"].as_str())
        .unwrap_or("unknown");
    let body = message["body"]
        .as_str()
        .unwrap_or("")
        .replace('&', "&amp;")
        .replace('<', "&lt;");
    format!(
        "<crew-message id=\"{}\" from=\"{}\" kind=\"{}\" trust=\"data\">\n{}\n</crew-message>",
        message["id"].as_str().unwrap_or(""),
        who,
        message["kind"].as_str().unwrap_or(""),
        body
    )
}

fn split_pattern(p: &str) -> (&str, bool) {
    p.strip_suffix("/**").map_or((p, false), |d| (d, true))
}

fn inside(dir: &str, path: &str) -> bool {
    path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'/'
}

/// Whether two path patterns can name the same file.
pub fn overlaps(a: &str, b: &str) -> bool {
    match (split_pattern(a), split_pattern(b)) {
        ((x, false), (y, false)) => x == y,
        ((x, true), (y, true)) => x == y || inside(x, y) || inside(y, x),
        ((x, true), (y, false)) => inside(x, y),
        ((x, false), (y, true)) => inside(y, x),
    }
}

struct Held {
    holder: String,
    paths: Vec<String>,
    write: bool,
}

struct Task {
    id: String,
    state: &'static str,
    brief: Value,
    key: Value,
    required_next: Option<&'static str>,
    last_fingerprint: Option<String>,
    repeat: u32,
    approach_changed: bool,
    failures: u32,
}

pub struct Model<'a> {
    validator: &'a Validator,
    max_exchanges: u32,
    max_posts: u32,
    ended: bool,
    tick: u32,
    posts: HashMap<String, u32>,
    hops: HashMap<(String, String), u32>,
    board: HashMap<String, (u64, Value)>,
    leases: Vec<Held>,
    task: Option<Task>,
    pub lead_notices: usize,
}

fn next_states(from: &str) -> &'static [&'static str] {
    match from {
        "briefed" => &["routed", "cancelled", "needs_person"],
        "routed" => &["running", "cancelled", "needs_person"],
        "running" => &["verifying", "needs_person", "stopped", "cancelled"],
        "verifying" => &["passed", "needs_person", "stopped", "cancelled"],
        "failed" => &["running", "changed_approach", "stopped", "cancelled"],
        "changed_approach" => &["running", "stopped", "cancelled"],
        "needs_person" => &["running", "routed", "stopped", "cancelled"],
        _ => &[],
    }
}

fn intern(state: &str) -> &'static str {
    [
        "briefed",
        "routed",
        "running",
        "verifying",
        "failed",
        "changed_approach",
        "needs_person",
        "passed",
        "stopped",
        "cancelled",
    ]
    .into_iter()
    .find(|s| *s == state)
    .unwrap_or("")
}

impl<'a> Model<'a> {
    pub fn new(validator: &'a Validator, max_exchanges: u32, max_posts: u32) -> Self {
        Model {
            validator,
            max_exchanges,
            max_posts,
            ended: false,
            tick: 0,
            posts: HashMap::new(),
            hops: HashMap::new(),
            board: HashMap::new(),
            leases: Vec::new(),
            task: None,
            lead_notices: 0,
        }
    }

    pub fn task_state(&self) -> Option<&'static str> {
        self.task.as_ref().map(|t| t.state)
    }

    pub fn ended(&self) -> bool {
        self.ended
    }

    pub fn end_run(&mut self) {
        self.ended = true;
    }

    fn lead_told<T>(&mut self, code: &'static str, detail: String) -> R<T> {
        self.lead_notices += 1;
        fail(code, detail)
    }

    /// The first failing check wins, in the order SPEC.md fixes: version, shape, secrets, id.
    pub fn validate(&self, object: &Value) -> R<Value> {
        if object["v"].as_i64().is_some_and(|v| v != 0) {
            return fail("unsupported_version", "v is not 0");
        }
        let kind = object["type"]
            .as_str()
            .ok_or(("invalid_object", "no type".to_owned()))?;
        let stem = kind.replace('_', "-");
        if !self.validator.has(&stem) || stem == "check" || stem == "common" || stem == "object" {
            return fail("invalid_object", format!("unknown type {kind}"));
        }
        let mut text = String::new();
        canonical(object, &mut text).map_err(|e| ("invalid_object", e))?;
        self.validator
            .check(&stem, object)
            .map_err(|e| ("invalid_object", e))?;
        if let Some(class) = find_secret(object) {
            return fail("secret_detected", class);
        }
        let want = object_id(object).map_err(|e| ("invalid_object", e))?;
        if object["id"] != json!(want) {
            return fail("id_mismatch", format!("expected {want}"));
        }
        Ok(json!({ "id": want }))
    }

    fn clock(&mut self) -> String {
        self.tick += 1;
        format!(
            "2026-11-12T09:{:02}:{:02}Z",
            self.tick / 60 % 60,
            self.tick % 60
        )
    }

    /// The core stamps the envelope; a draft that carries any envelope field is forged.
    fn stamp(
        &mut self,
        kind: &str,
        caller: &Value,
        draft: &Value,
        filled: &[(&str, Value)],
        run: bool,
    ) -> R<Value> {
        let map = draft
            .as_object()
            .ok_or(("invalid_object", "draft is not an object".to_owned()))?;
        for key in ENVELOPE_KEYS
            .iter()
            .copied()
            .chain(filled.iter().map(|(k, _)| *k))
        {
            if map.contains_key(key) {
                return fail("invalid_object", format!("a draft must not carry {key}"));
            }
        }
        let mut object: Map<String, Value> = map.clone();
        object.insert("type".into(), json!(kind));
        object.insert("v".into(), json!(0));
        object.insert("crew_id".into(), json!("crew-01"));
        object.insert("author".into(), caller.clone());
        let created = self.clock();
        object.insert("created_at".into(), json!(created));
        if run {
            object.insert("run_id".into(), json!("run-01"));
        }
        for (k, v) in filled {
            object.insert((*k).to_owned(), v.clone());
        }
        let mut object = Value::Object(object);
        object["id"] = json!(object_id(&object).map_err(|e| ("invalid_object", e))?);
        self.validate(&object)?;
        Ok(object)
    }

    pub fn call(&mut self, tool: &str, caller: &Value, args: &Value) -> R<Value> {
        let role = caller["role"].as_str().unwrap_or("");
        let member = caller["member"].as_str().unwrap_or("").to_owned();
        if !MEMBER_TOOLS.contains(&tool) && !CORE_TOOLS.contains(&tool) {
            return fail("unknown_tool", tool);
        }
        if self.ended {
            return fail("run_ended", "the run has ended");
        }
        let is_member = matches!(role, "lead" | "worker" | "reviewer");
        match tool {
            "crew.verify" | "crew.evidence.write" if role != "core" => {
                return fail("not_authorized", tool);
            }
            "crew.gate.write" if !matches!(role, "core" | "person") => {
                return fail("not_authorized", tool);
            }
            "crew.start" if matches!(role, "worker" | "reviewer") => {
                return fail("depth_exceeded", "workers cannot start workers");
            }
            "crew.start" if !matches!(role, "lead" | "person" | "core") => {
                return fail("not_authorized", tool);
            }
            t if MEMBER_TOOLS.contains(&t) && t != "crew.start" && !is_member => {
                return fail("not_authorized", tool);
            }
            _ => {}
        }
        match tool {
            "crew.start" => self.start(caller, args),
            "crew.brief.read" => self.brief_read(),
            "crew.post" | "crew.ask" => self.post(tool, caller, &member, args),
            "crew.lease.request" => self.lease_request(caller, &member, args),
            "crew.lease.release" => self.lease_release(&member, args),
            "crew.fact.read" => Ok(self
                .board
                .get(args["key"].as_str().unwrap_or(""))
                .map_or(Value::Null, |(_, f)| f.clone())),
            "crew.fact.write" => self.fact_write(caller, &member, args),
            "crew.claim" => self
                .stamp("claim", caller, args, &[], true)
                .map(|o| json!({ "id": o["id"] })),
            "crew.finding" => self
                .stamp("finding", caller, args, &[], true)
                .map(|o| json!({ "id": o["id"] })),
            "crew.report" => {
                let brief = self
                    .task
                    .as_ref()
                    .map(|t| t.id.clone())
                    .ok_or(("invalid_object", "no task".to_owned()))?;
                self.stamp("report", caller, args, &[("brief", json!(brief))], true)
                    .map(|o| json!({ "id": o["id"] }))
            }
            "crew.evidence.write" => self
                .stamp("evidence", caller, args, &[], true)
                .map(|o| json!({ "id": o["id"] })),
            "crew.gate.write" => {
                if args["decided_by"] != json!("person")
                    && args["evidence"].as_array().is_none_or(|e| e.is_empty())
                {
                    return fail(
                        "gate_needs_evidence",
                        "a gate decided by code needs evidence",
                    );
                }
                self.stamp("gate", caller, args, &[], true)
                    .map(|o| json!({ "id": o["id"] }))
            }
            _ => Ok(json!({ "ok": true })), // crew.verify: the verifier itself is not modelled at slice 0
        }
    }

    fn start(&mut self, caller: &Value, args: &Value) -> R<Value> {
        if self.task.is_some() {
            return fail("invalid_transition", "this run already has a task");
        }
        let brief = self.stamp("brief", caller, &args["brief"], &[], false)?;
        let brief_id = brief["id"].as_str().unwrap().to_owned();
        let key = self.stamp(
            "answer_key",
            caller,
            &args["answer_key"],
            &[("brief", json!(brief_id))],
            false,
        )?;
        let protected: Vec<&str> = key["protected_paths"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for file in brief["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["mode"] == "write")
        {
            let path = file["path"].as_str().unwrap();
            if let Some(p) = protected.iter().find(|p| overlaps(path, p)) {
                return fail("protected_path_conflict", format!("{path} overlaps {p}"));
            }
        }
        self.task = Some(Task {
            id: brief_id.clone(),
            state: "briefed",
            brief,
            key,
            required_next: None,
            last_fingerprint: None,
            repeat: 0,
            approach_changed: false,
            failures: 0,
        });
        Ok(json!({ "task": brief_id }))
    }

    /// The only way a brief reaches a worker. Nothing from the answer key may travel with it.
    fn brief_read(&self) -> R<Value> {
        let task = self
            .task
            .as_ref()
            .ok_or(("invalid_object", "no task".to_owned()))?;
        let mut needles = vec![task.key["id"].as_str().unwrap().to_owned()];
        for check in task.key["checks"].as_array().unwrap() {
            if let Some(run) = check["params"]["run"].as_str().filter(|r| r.len() >= 8) {
                needles.push(run.to_owned());
            }
        }
        let mut leaked = None;
        walk_strings(&task.brief, &mut |s| {
            leaked = leaked
                .take()
                .or_else(|| needles.iter().find(|n| s.contains(n.as_str())).cloned());
        });
        if let Some(n) = leaked {
            return fail("answer_key_forbidden", format!("the brief repeats {n}"));
        }
        Ok(task.brief.clone())
    }

    fn post(&mut self, tool: &str, caller: &Value, member: &str, args: &Value) -> R<Value> {
        if tool == "crew.ask" && args["kind"] != json!("question") {
            return fail("invalid_object", "crew.ask sends a question");
        }
        let posted = self.posts.get(member).copied().unwrap_or(0);
        if posted >= self.max_posts {
            return self.lead_told(
                "post_limit_exceeded",
                format!("{member} reached {} posts", self.max_posts),
            );
        }
        let message = self.stamp("message", caller, args, &[], true)?;
        let to = message["to"].as_str().unwrap();
        let role = caller["role"].as_str().unwrap();
        if !matches!(to, "lead" | "all") && role != "lead" {
            let pair = if member < to {
                (member.to_owned(), to.to_owned())
            } else {
                (to.to_owned(), member.to_owned())
            };
            if self.hops.get(&pair).copied().unwrap_or(0) >= self.max_exchanges {
                return self.lead_told(
                    "hop_limit_exceeded",
                    format!(
                        "{} and {} reached {} messages",
                        pair.0, pair.1, self.max_exchanges
                    ),
                );
            }
            *self.hops.entry(pair).or_insert(0) += 1;
        }
        if role == "lead" {
            // The lead stepped in: the pairs it addressed start counting again.
            self.hops
                .retain(|(a, b), _| to != "all" && a != to && b != to);
        }
        *self.posts.entry(member.to_owned()).or_insert(0) += 1;
        Ok(json!({ "id": message["id"], "delivered_as": wrap_message(&message) }))
    }

    fn lease_request(&mut self, caller: &Value, member: &str, args: &Value) -> R<Value> {
        let lease = self.stamp(
            "lease",
            caller,
            args,
            &[("holder", json!(member)), ("state", json!("granted"))],
            true,
        )?;
        let paths: Vec<String> = lease["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_owned())
            .collect();
        let write = lease["mode"] == "write";
        let clash = self.leases.iter().find(|held| {
            held.holder != member
                && (held.write || write)
                && held
                    .paths
                    .iter()
                    .any(|a| paths.iter().any(|b| overlaps(a, b)))
        });
        if let Some(held) = clash {
            let holder = held.holder.clone();
            return self.lead_told("lease_conflict", format!("held by {holder}"));
        }
        self.leases.push(Held {
            holder: member.to_owned(),
            paths,
            write,
        });
        Ok(json!({ "id": lease["id"], "state": "granted" }))
    }

    fn lease_release(&mut self, member: &str, args: &Value) -> R<Value> {
        let paths: Vec<&str> = args["paths"]
            .as_array()
            .map(|p| p.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let before = self.leases.len();
        self.leases.retain(|h| {
            !(h.holder == member && h.paths.iter().map(String::as_str).eq(paths.iter().copied()))
        });
        if self.leases.len() == before {
            return fail("lease_not_held", "no such lease held");
        }
        Ok(json!({ "ok": true }))
    }

    fn fact_write(&mut self, caller: &Value, member: &str, args: &Value) -> R<Value> {
        let base = args["base_rev"]
            .as_u64()
            .ok_or(("invalid_object", "base_rev".to_owned()))?;
        let key = args["key"].as_str().unwrap_or("").to_owned();
        let current = self.board.get(&key).map_or(0, |(rev, _)| *rev);
        let fact = self.stamp("fact", caller, args, &[("rev", json!(base + 1))], true)?;
        if base != current {
            return self.lead_told(
                "board_conflict",
                format!("{key} is at revision {current}, {member} wrote on {base}"),
            );
        }
        self.board.insert(key, (base + 1, fact.clone()));
        Ok(json!({ "id": fact["id"], "rev": base + 1 }))
    }

    /// Core-side lifecycle step (SPEC.md, "Task lifecycle").
    pub fn transition(&mut self, to: &str, evidence: usize, person: bool) -> R<Value> {
        let task = self
            .task
            .as_mut()
            .ok_or(("invalid_transition", "no task".to_owned()))?;
        let to = intern(to);
        if to.is_empty() || to == "failed" || !next_states(task.state).contains(&to) {
            return fail("invalid_transition", format!("{} to {to}", task.state));
        }
        if task.state == "failed" && task.required_next.is_some_and(|n| n != to) {
            return fail(
                "invalid_transition",
                format!("the loop guard requires {:?}", task.required_next),
            );
        }
        if to == "passed" && evidence == 0 && !person {
            return fail("gate_needs_evidence", "passed needs evidence or a person");
        }
        task.state = to;
        task.required_next = None;
        if matches!(to, "passed" | "stopped" | "cancelled") {
            self.ended = true;
        }
        Ok(json!({ "state": to }))
    }

    /// A failed verification, with the failure's fingerprint. Applies the loop guard.
    pub fn verify_failed(&mut self, fingerprint: &str) -> R<Value> {
        let task = self
            .task
            .as_mut()
            .ok_or(("invalid_transition", "no task".to_owned()))?;
        if task.state != "verifying" {
            return fail(
                "invalid_transition",
                format!("{} is not verifying", task.state),
            );
        }
        task.failures += 1;
        task.repeat = if task.last_fingerprint.as_deref() == Some(fingerprint) {
            task.repeat + 1
        } else {
            1
        };
        task.last_fingerprint = Some(fingerprint.to_owned());
        let next = if task.repeat < 2 {
            "running"
        } else if !task.approach_changed {
            task.approach_changed = true;
            "changed_approach"
        } else {
            "stopped"
        };
        task.state = "failed";
        task.required_next = Some(next);
        Ok(json!({ "state": "failed", "next": next, "failures": task.failures }))
    }
}
