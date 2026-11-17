//! Background-crew triggers and templates in `.orbyn/crew.json`, read as policy and checked without starting anything
//! (docs/crew.md, "Background crews"). A repository may say which crews it would like Orbyn to run: what would start
//! each one, which template it follows, the engines allowed to take it, the budget it may spend and a sentence for the
//! person. This module only parses and validates that section. **Nothing here starts a run, calls a model, spawns a
//! process, writes a file or adds a route:** the CI-failure listener, the schedule, the manual start and the templates'
//! prompts and checks are later slices.
//!
//! The file is policy, never authority. Orbyn reads it and never writes it (architecture rule 4); a failure to read it
//! refuses the triggers rather than falling back to none, and text in it is data: it is carried into a report, never
//! followed, expanded or run. The always-denied list stays where it is enforced, in code
//! ([`autopilot::RULES`](super::autopilot::RULES)), and a template in this file cannot grant a command. Hidden
//! characters and instruction-shaped text are refused with the Autopilot tripwire, and problems are reported all at
//! once, each naming its field and the shape it wants. The tripwire's hidden set is the narrow one it uses for text a
//! repository or an agent writes; the stored-crew validator keeps a wider one for input a person types, so the two
//! differ on purpose.
//!
//! Limits: a key written twice is refused with a problem naming it (at most 20 are named); problems past 50 are counted,
//! not listed; at most 64 triggers and 16 templates are read. Names are compared after trimming, lower-casing and
//! collapsing spaces, not after Unicode normalisation, so two names built from different code points can still look
//! alike. A schedule's minimum and the budget caps are constants here, not a project setting yet. The parse reads text only; it is not a sandbox for what a later slice
//! does with a trigger it accepts.

use std::cell::RefCell;
use std::fmt;

use orbyn_contracts::CrewToken;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use super::background;
use super::{injection_in, redact};

/// The largest policy file read, in bytes: the same limit the classes reader uses.
pub const FILE_MAX: usize = 64 * 1024;
/// The most triggers in one file.
pub const TRIGGERS_MAX: usize = 64;
/// The most templates a file may declare.
pub const TEMPLATES_MAX: usize = 16;
/// The most engines one trigger may name.
pub const ENGINES_MAX: usize = 16;
/// The longest trigger name, in characters.
pub const NAME_MAX: usize = 80;
/// The longest human description, in characters.
pub const DESCRIPTION_MAX: usize = 280;
/// The longest branch, workflow or template id, in characters.
pub const SETTING_MAX: usize = background::SETTING_MAX;
/// The most turns one run may be given.
pub const TURNS_MAX: u32 = background::TURNS_MAX;
/// The most minutes one run may be given.
pub const MINUTES_MAX: u32 = background::MINUTES_MAX;
/// The closest a schedule may run, in minutes: one hour, the stored crew's smallest `every_hours`.
pub const SCHEDULE_MIN_MINUTES: u32 = 60;
/// A schedule is a whole number of hours: its minutes are a multiple of this.
pub const SCHEDULE_STEP_MINUTES: u32 = 60;
/// The most problems one refusal lists; the rest are counted in a final line.
pub const PROBLEMS_MAX: usize = 50;
/// The widest gap of a schedule, in minutes: 30 days, the stored crew's largest `every_hours` (720).
pub const SCHEDULE_MAX_MINUTES: u32 = 43_200;
/// The names of the trigger kinds, in the order a refusal lists them.
pub const KIND_NAMES: [&str; 3] = ["ci_failed", "manual", "schedule"];
/// The template ids code knows. A file may declare more, but it cannot grant them commands.
pub const TEMPLATE_IDS: [&str; 4] = [
    "dependency_bump",
    "fix_failing_ci",
    "flaky_test",
    "security_advisory",
];

/// What would start a background crew. Nothing fires in this slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    /// A workflow on a branch failed.
    CiFailed,
    /// A fixed interval passed.
    Schedule,
    /// A person starts it.
    Manual,
}

impl TriggerKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::CiFailed => "ci_failed",
            Self::Schedule => "schedule",
            Self::Manual => "manual",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "ci_failed" => Some(Self::CiFailed),
            "schedule" => Some(Self::Schedule),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    /// The settings this kind takes besides the ones every trigger has.
    fn settings(self) -> &'static [&'static str] {
        match self {
            Self::CiFailed => &["branch", "workflow"],
            Self::Schedule => &["every_minutes"],
            Self::Manual => &[],
        }
    }
}

/// What one run may spend, set by a person in the file and never defaulted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    pub turns: u32,
    pub minutes: u32,
}

/// One checked trigger: what would start a crew, and the bounds it may run within.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerDef {
    pub name: String,
    pub kind: TriggerKind,
    pub template: String,
    pub budget: Budget,
    /// Engine ids allowed to take the run. Each is a token; none may be empty.
    pub engines: Vec<String>,
    pub description: String,
    /// `schedule` only: the minutes between runs, a multiple of 60 from [`SCHEDULE_MIN_MINUTES`] to
    /// [`SCHEDULE_MAX_MINUTES`], so each maps to a stored `every_hours` of 1 to 720.
    pub every_minutes: Option<u32>,
    /// `ci_failed` only: the branch watched, when the file names one.
    pub branch: Option<String>,
    /// `ci_failed` only: the workflow watched, when the file names one.
    pub workflow: Option<String>,
}

/// One problem with the file, in the `field: what is wrong, what is allowed` shape `crew_start` uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub field: String,
    pub reason: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{}: {}", self.field, self.reason)
    }
}

fn problem(out: &mut Vec<Problem>, field: &str, reason: impl Into<String>) {
    out.push(Problem {
        field: field.to_owned(),
        reason: reason.into(),
    });
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

/// Cuts and redacts a value quoted back in a problem, so a refusal never repeats a credential, a whole file, a control
/// character or a hidden character that would break the one-line `field: reason` shape.
fn quoted(text: &str) -> String {
    let cut: String = redact(text)
        .chars()
        .filter(|c| !c.is_control() && injection_in(&c.to_string()).is_none())
        .take(80)
        .collect();
    cut.replace('`', "'")
}

/// A key of the file as it may appear in a problem's field: cut, redacted and free of control and hidden characters, or
/// withheld when it reads as an instruction. A repository chooses its keys, so they get the treatment its values get.
fn safe_key(key: &str) -> String {
    let cleaned = quoted(key);
    // Hidden and control characters can glue a phrase to the text before it once stripped, so the key is also read with
    // each of them as a space, and with `_` and `-` read as spaces.
    let spaced: String = key
        .chars()
        .take(4096)
        .map(|c| {
            if c.is_control() || injection_in(&c.to_string()).is_some() || c == '_' || c == '-' {
                ' '
            } else {
                c
            }
        })
        .collect();
    if injection_in(&cleaned).is_some() || injection_in(&spaced).is_some() {
        "(key withheld: instruction-like)".to_owned()
    } else {
        cleaned
    }
}

/// An object at `field`, with every key it carries other than `known` reported.
fn object<'a>(
    out: &mut Vec<Problem>,
    field: &str,
    value: &'a Value,
    known: &[&str],
    shape: &str,
) -> Option<&'a Map<String, Value>> {
    let Some(map) = value.as_object() else {
        problem(out, field, format!("{}, expected {shape}", kind_of(value)));
        return None;
    };
    for key in map.keys() {
        if !known.contains(&key.as_str()) {
            problem(
                out,
                &join(field, &safe_key(key)),
                format!("not a field; allowed: {}", known.join(", ")),
            );
        }
    }
    Some(map)
}

fn join(field: &str, key: &str) -> String {
    if field.is_empty() {
        key.to_owned()
    } else {
        format!("{field}.{key}")
    }
}

/// A field a person typed: trimmed, 1 to `max` characters, no control characters and none that hide text or read as an
/// instruction (the Autopilot tripwire's own checks). The text is returned as written, never expanded or unescaped
/// beyond JSON's own decoding.
fn text(out: &mut Vec<Problem>, field: &str, value: &Value, max: usize) -> Option<String> {
    let Some(raw) = value.as_str() else {
        problem(
            out,
            field,
            format!(
                "{}, expected a string 1 to {max} characters",
                kind_of(value)
            ),
        );
        return None;
    };
    // Length first: an oversize value is refused before it is trimmed, scanned or redacted.
    if raw.len() > max.saturating_mul(4) {
        problem(
            out,
            field,
            format!(
                "{} bytes, expected a string 1 to {max} characters",
                raw.len()
            ),
        );
        return None;
    }
    let value = raw.trim();
    let count = value.chars().count();
    if !(1..=max).contains(&count) {
        problem(
            out,
            field,
            format!("{count} characters, expected 1 to {max}"),
        );
        return None;
    }
    if value.chars().any(char::is_control) {
        problem(out, field, "has a control character, which cannot be shown");
        return None;
    }
    match injection_in(value) {
        Some("hidden characters") => {
            problem(
                out,
                field,
                "has a hidden character (zero-width, bidirectional or tag block)",
            );
            return None;
        }
        Some(class) => {
            problem(
                out,
                field,
                format!(
                    "reads as instruction-like text ({class}); a repository file is data, never an instruction"
                ),
            );
            return None;
        }
        None => {}
    }
    // The route stores this text redacted (`background::text`), so text that changes under redaction is refused.
    if redact(value) != value {
        problem(
            out,
            field,
            "looks like a credential; it would be redacted, so it is refused",
        );
        return None;
    }
    Some(value.to_owned())
}

fn one_of<'a>(
    out: &mut Vec<Problem>,
    field: &str,
    value: &'a Value,
    allowed: &[&str],
) -> Option<&'a str> {
    match value.as_str() {
        Some(text) if allowed.contains(&text) => Some(text),
        Some(text) => {
            problem(
                out,
                field,
                format!("`{}` is not one of {}", quoted(text), allowed.join(", ")),
            );
            None
        }
        None => {
            problem(
                out,
                field,
                format!("{}, expected one of {}", kind_of(value), allowed.join(", ")),
            );
            None
        }
    }
}

fn whole(
    out: &mut Vec<Problem>,
    field: &str,
    value: &Value,
    min: u32,
    max: u32,
    what: &str,
) -> Option<u32> {
    match value.as_u64() {
        Some(number) if (u64::from(min)..=u64::from(max)).contains(&number) => Some(number as u32),
        Some(number) => {
            problem(
                out,
                field,
                format!("{number} is out of range; a person sets {what} from {min} to {max}"),
            );
            None
        }
        None => {
            problem(
                out,
                field,
                format!(
                    "{}, expected a whole number {what} from {min} to {max}",
                    kind_of(value)
                ),
            );
            None
        }
    }
}

fn token(out: &mut Vec<Problem>, field: &str, value: &Value, what: &str) -> Option<String> {
    let Some(text) = value.as_str() else {
        problem(out, field, format!("{}, expected {what}", kind_of(value)));
        return None;
    };
    if CrewToken::new(text.to_owned()).is_err() {
        problem(
            out,
            field,
            format!(
                "`{}` is not {what} (1 to 64 characters, starting with a lowercase letter or digit, then lowercase letters, digits, `.`, `_` or `-`)",
                quoted(text)
            ),
        );
        return None;
    }
    Some(text.to_owned())
}

/// The name as compared for collisions: trimmed, lower-cased and with runs of spaces collapsed. Not Unicode-normalised,
/// so two spellings of one letter can still be two names; the docs say so.
pub fn normalized(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn walk_template(out: &mut Vec<Problem>, declared: &mut Vec<String>, at: usize, item: &Value) {
    let field = format!("background.templates[{at}]");
    let Some(map) = item.as_object() else {
        problem(
            out,
            &field,
            format!("{}, expected {{id, description}}", kind_of(item)),
        );
        return;
    };
    for key in map.keys() {
        if matches!(
            key.as_str(),
            "commands" | "allowed_commands" | "allow" | "run" | "scripts" | "shell" | "tools"
        ) {
            problem(
                out,
                &join(&field, &safe_key(key)),
                "a template cannot allow commands: the always-denied list is enforced in code and nothing in this file can widen it",
            );
        } else if key != "id" && key != "description" {
            problem(
                out,
                &join(&field, &safe_key(key)),
                "not a field; allowed: id, description",
            );
        }
    }
    let id = match map.get("id").filter(|value| !value.is_null()) {
        Some(value) => token(out, &join(&field, "id"), value, "a template id"),
        None => {
            problem(
                out,
                &join(&field, "id"),
                "missing, a template id such as fix_failing_ci",
            );
            None
        }
    };
    if let Some(id) = &id {
        if TEMPLATE_IDS.contains(&id.as_str()) {
            problem(
                out,
                &join(&field, "id"),
                format!("`{id}` is built in, so it is not declared again"),
            );
        } else if declared.contains(id) {
            problem(
                out,
                &join(&field, "id"),
                format!("`{id}` is declared twice"),
            );
        } else {
            declared.push(id.clone());
        }
    }
    match map.get("description").filter(|value| !value.is_null()) {
        Some(value) => {
            text(out, &join(&field, "description"), value, DESCRIPTION_MAX);
        }
        None => problem(
            out,
            &join(&field, "description"),
            format!("missing, a sentence for the person, 1 to {DESCRIPTION_MAX} characters"),
        ),
    }
}

fn walk_budget(out: &mut Vec<Problem>, field: &str, value: &Value) -> Option<Budget> {
    let map = object(out, field, value, &["turns", "minutes"], "{turns, minutes}")?;
    let turns = match map.get("turns").filter(|value| !value.is_null()) {
        Some(value) => whole(out, &join(field, "turns"), value, 1, TURNS_MAX, "turns"),
        None => {
            problem(
                out,
                &join(field, "turns"),
                "missing, a person sets the turns in the file; there is no default",
            );
            None
        }
    };
    let minutes = match map.get("minutes").filter(|value| !value.is_null()) {
        Some(value) => whole(
            out,
            &join(field, "minutes"),
            value,
            1,
            MINUTES_MAX,
            "minutes",
        ),
        None => {
            problem(
                out,
                &join(field, "minutes"),
                "missing, a person sets the minutes in the file; there is no default",
            );
            None
        }
    };
    Some(Budget {
        turns: turns?,
        minutes: minutes?,
    })
}

fn walk_engines(out: &mut Vec<Problem>, field: &str, value: &Value) -> Option<Vec<String>> {
    let Some(items) = value.as_array() else {
        problem(
            out,
            field,
            format!("{}, expected an array of engine ids", kind_of(value)),
        );
        return None;
    };
    if items.is_empty() {
        problem(out, field, "needs at least one engine id");
        return None;
    }
    if items.len() > ENGINES_MAX {
        problem(
            out,
            field,
            format!("{} engine ids; the most is {ENGINES_MAX}", items.len()),
        );
        return None;
    }
    let mut engines: Vec<String> = Vec::new();
    let mut fine = true;
    for (index, item) in items.iter().enumerate() {
        let field = format!("{field}[{index}]");
        match token(out, &field, item, "an engine id") {
            Some(engine) if engines.contains(&engine) => {
                problem(out, &field, format!("`{engine}` is named twice"));
                fine = false;
            }
            Some(engine) => engines.push(engine),
            None => fine = false,
        }
    }
    fine.then_some(engines)
}

fn walk_trigger(
    out: &mut Vec<Problem>,
    declared: &[String],
    names: &mut Vec<(String, String)>,
    at_index: usize,
    item: &Value,
) -> Option<TriggerDef> {
    const KNOWN: [&str; 9] = [
        "name",
        "on",
        "template",
        "budget",
        "engines",
        "description",
        "every_minutes",
        "branch",
        "workflow",
    ];
    let field = format!("background.triggers[{at_index}]");
    let map = object(
        out,
        &field,
        item,
        &KNOWN,
        "{name, on, template, budget, engines, description}",
    )?;
    let before = out.len();

    let name = match map.get("name").filter(|value| !value.is_null()) {
        Some(value) => text(out, &join(&field, "name"), value, NAME_MAX),
        None => {
            problem(
                out,
                &join(&field, "name"),
                format!("missing, a name of 1 to {NAME_MAX} characters"),
            );
            None
        }
    };
    if let Some(name) = &name {
        let key = normalized(name);
        match names.iter().find(|(seen, _)| *seen == key) {
            Some((_, first)) => {
                problem(
                    out,
                    &join(&field, "name"),
                    format!(
                        "`{}` is the same as `{}` once case and spacing are ignored; names must differ",
                        quoted(name),
                        quoted(first)
                    ),
                );
            }
            None => names.push((key, name.clone())),
        }
    }

    let kind = match map.get("on").filter(|value| !value.is_null()) {
        Some(value) => {
            one_of(out, &join(&field, "on"), value, &KIND_NAMES).and_then(TriggerKind::parse)
        }
        None => {
            problem(
                out,
                &join(&field, "on"),
                format!("missing, one of {}", KIND_NAMES.join(", ")),
            );
            None
        }
    };

    let template = match map.get("template").filter(|value| !value.is_null()) {
        Some(value) => {
            let field = join(&field, "template");
            let template = text(out, &field, value, SETTING_MAX);
            if let Some(template) = &template {
                let known =
                    TEMPLATE_IDS.iter().any(|id| id == template) || declared.contains(template);
                if !known {
                    problem(
                        out,
                        &field,
                        format!(
                            "`{}` is not a template; use {} or one this file declares",
                            quoted(template),
                            TEMPLATE_IDS.join(", ")
                        ),
                    );
                    None
                } else {
                    Some(template.clone())
                }
            } else {
                None
            }
        }
        None => {
            problem(
                out,
                &join(&field, "template"),
                format!(
                    "missing, one of {} or one this file declares",
                    TEMPLATE_IDS.join(", ")
                ),
            );
            None
        }
    };

    let budget = match map.get("budget").filter(|value| !value.is_null()) {
        Some(value) => walk_budget(out, &join(&field, "budget"), value),
        None => {
            problem(
                out,
                &join(&field, "budget"),
                "missing, a person sets the turns and minutes in the file; there is no default",
            );
            None
        }
    };

    let engines = match map.get("engines").filter(|value| !value.is_null()) {
        Some(value) => walk_engines(out, &join(&field, "engines"), value),
        None => {
            problem(
                out,
                &join(&field, "engines"),
                "missing, the engine ids allowed to take the run",
            );
            None
        }
    };

    let description = match map.get("description").filter(|value| !value.is_null()) {
        Some(value) => text(out, &join(&field, "description"), value, DESCRIPTION_MAX),
        None => {
            problem(
                out,
                &join(&field, "description"),
                format!("missing, a sentence for the person, 1 to {DESCRIPTION_MAX} characters"),
            );
            None
        }
    };

    // Every setting a file may carry is shape-checked, whatever `on` says, so one bad file reports all it can.
    let every_value = map.get("every_minutes").filter(|value| !value.is_null());
    let branch_value = map.get("branch").filter(|value| !value.is_null());
    let workflow_value = map.get("workflow").filter(|value| !value.is_null());
    let every = every_value.map(|value| {
        let at = join(&field, "every_minutes");
        let minutes = whole(
            out,
            &at,
            value,
            SCHEDULE_MIN_MINUTES,
            SCHEDULE_MAX_MINUTES,
            "minutes between runs",
        )?;
        if minutes % SCHEDULE_STEP_MINUTES != 0 {
            problem(
                out,
                &at,
                format!(
                    "{minutes} is not a whole number of hours; use a multiple of {SCHEDULE_STEP_MINUTES} from {SCHEDULE_MIN_MINUTES} to {SCHEDULE_MAX_MINUTES}"
                ),
            );
            return None;
        }
        Some(minutes)
    });
    let branch_checked =
        branch_value.map(|value| text(out, &join(&field, "branch"), value, SETTING_MAX));
    let workflow_checked =
        workflow_value.map(|value| text(out, &join(&field, "workflow"), value, SETTING_MAX));

    let mut every_minutes = None;
    let mut branch = None;
    let mut workflow = None;
    if let Some(kind) = kind {
        let takes = kind.settings();
        for key in ["every_minutes", "branch", "workflow"] {
            if map.get(key).filter(|value| !value.is_null()).is_some() && !takes.contains(&key) {
                problem(
                    out,
                    &join(&field, key),
                    format!("a {} trigger has no {key} setting", kind.name()),
                );
            }
        }
        match kind {
            TriggerKind::Schedule => {
                every_minutes = match every {
                    Some(every) => every,
                    None => {
                        problem(
                            out,
                            &join(&field, "every_minutes"),
                            format!(
                                "missing, the minutes between runs, a multiple of {SCHEDULE_STEP_MINUTES} from {SCHEDULE_MIN_MINUTES} to {SCHEDULE_MAX_MINUTES}"
                            ),
                        );
                        None
                    }
                };
            }
            TriggerKind::CiFailed => {
                branch = branch_checked.flatten();
                workflow = workflow_checked.flatten();
            }
            TriggerKind::Manual => {}
        }
    }

    if out.len() > before {
        return None;
    }
    Some(TriggerDef {
        name: name?,
        kind: kind?,
        template: template?,
        budget: budget?,
        engines: engines?,
        description: description?,
        every_minutes,
        branch,
        workflow,
    })
}

/// Reads the `background` section of `.orbyn/crew.json`: every trigger it defines, checked, or every problem at once.
///
/// `Ok` with no triggers covers a file with no `background` section: another slice's sections are not this parser's to
/// refuse. An empty or oversized file, invalid JSON, an unknown field, trigger kind or template, a missing or over-cap
/// budget, a schedule closer than [`SCHEDULE_MIN_MINUTES`], a name that collides with an earlier one after
/// [`normalized`], hidden, credential-shaped or instruction-shaped text, a key written twice and a template that tries to
/// allow commands are all refused. At most [`PROBLEMS_MAX`] problems are listed, then one line counts the rest.
pub fn parse_and_validate(text: &str) -> Result<Vec<TriggerDef>, Vec<Problem>> {
    check(text).map_err(|mut problems| {
        if problems.len() > PROBLEMS_MAX {
            let more = problems.len() - PROBLEMS_MAX;
            problems.truncate(PROBLEMS_MAX);
            problem(
                &mut problems,
                "background",
                format!("and {more} more problems, not shown"),
            );
        }
        problems
    })
}

/// Where a key was met, for naming a duplicate.
struct Path<'a> {
    parent: Option<&'a Path<'a>>,
    seg: Seg<'a>,
}

enum Seg<'a> {
    Key(&'a str),
    Index(usize),
}

fn render(at: Option<&Path<'_>>, key: &str) -> String {
    let mut segs = Vec::new();
    let mut next = at;
    while let Some(path) = next {
        segs.push(&path.seg);
        next = path.parent;
    }
    let mut field = String::new();
    for seg in segs.into_iter().rev() {
        match seg {
            Seg::Key(key) => field = join(&field, &safe_key(key)),
            Seg::Index(at) => field.push_str(&format!("[{at}]")),
        }
    }
    join(&field, &safe_key(key))
}

/// Parses JSON into a [`Value`] the way `serde_json` does, and notes each key an object writes twice (which a plain
/// parse would silently resolve to the last one).
#[derive(Clone, Copy)]
struct Checked<'a> {
    at: Option<&'a Path<'a>>,
    dups: &'a RefCell<Vec<String>>,
}

const DUPLICATES_MAX: usize = 20;

impl<'de> DeserializeSeed<'de> for Checked<'_> {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<Value, D::Error> {
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Checked<'_> {
    type Value = Value;
    fn expecting(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str("any JSON value")
    }
    fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }
    fn visit_f64<E>(self, value: f64) -> Result<Value, E> {
        Ok(Number::from_f64(value).map_or(Value::Null, Value::Number))
    }
    fn visit_str<E>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }
    fn visit_string<E>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }
    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = {
            let child = Path {
                parent: self.at,
                seg: Seg::Index(items.len()),
            };
            seq.next_element_seed(Checked {
                at: Some(&child),
                dups: self.dups,
            })?
        } {
            items.push(item);
        }
        Ok(Value::Array(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut map = Map::new();
        while let Some(key) = access.next_key::<String>()? {
            if map.contains_key(&key) && self.dups.borrow().len() < DUPLICATES_MAX {
                self.dups.borrow_mut().push(render(self.at, &key));
            }
            let value = {
                let child = Path {
                    parent: self.at,
                    seg: Seg::Key(&key),
                };
                access.next_value_seed(Checked {
                    at: Some(&child),
                    dups: self.dups,
                })?
            };
            map.insert(key, value);
        }
        Ok(Value::Object(map))
    }
}

fn check(text: &str) -> Result<Vec<TriggerDef>, Vec<Problem>> {
    let mut out = Vec::new();
    // Length before trim: an oversize file is refused without being scanned.
    if text.len() > FILE_MAX {
        problem(
            &mut out,
            "background",
            format!(
                "the file is {} bytes; the most is {FILE_MAX}, so it is not read",
                text.len()
            ),
        );
        return Err(out);
    }
    if text.trim().is_empty() {
        problem(
            &mut out,
            "background",
            "the file is empty; it needs a JSON object with a `background` section",
        );
        return Err(out);
    }
    let dups = RefCell::new(Vec::new());
    let mut de = serde_json::Deserializer::from_str(text);
    let parsed = Checked {
        at: None,
        dups: &dups,
    }
    .deserialize(&mut de)
    .and_then(|value| de.end().map(|()| value));
    let value: Value = match parsed {
        Ok(value) => value,
        Err(error) => {
            problem(
                &mut out,
                "background",
                format!(
                    "is not valid JSON (line {}, column {})",
                    error.line(),
                    error.column()
                ),
            );
            return Err(out);
        }
    };
    for key in dups.into_inner() {
        problem(
            &mut out,
            &key,
            "is written twice; JSON keeps only the last, so the file is refused",
        );
    }
    let Value::Object(top) = value else {
        problem(
            &mut out,
            "background",
            format!("{}, expected a JSON object", kind_of(&value)),
        );
        return Err(out);
    };
    let Some(section) = top.get("background") else {
        return Ok(Vec::new());
    };
    let Some(section) = object(
        &mut out,
        "background",
        section,
        &["version", "templates", "triggers"],
        "an object {version, templates, triggers}",
    ) else {
        return Err(out);
    };
    match section.get("version").filter(|value| !value.is_null()) {
        Some(Value::Number(version)) if version.as_u64() == Some(1) => {}
        Some(Value::Number(version)) => problem(
            &mut out,
            "background.version",
            format!("{version} is not supported; this version reads 1"),
        ),
        Some(value) => problem(
            &mut out,
            "background.version",
            format!("{}, expected 1", kind_of(value)),
        ),
        None => problem(&mut out, "background.version", "missing, expected 1"),
    }

    let mut declared: Vec<String> = Vec::new();
    if let Some(value) = section.get("templates") {
        match value.as_array() {
            Some(items) => {
                if items.len() > TEMPLATES_MAX {
                    problem(
                        &mut out,
                        "background.templates",
                        format!("{} templates; the most is {TEMPLATES_MAX}", items.len()),
                    );
                }
                for (at_index, item) in items.iter().enumerate().take(TEMPLATES_MAX) {
                    walk_template(&mut out, &mut declared, at_index, item);
                }
            }
            None => problem(
                &mut out,
                "background.templates",
                format!(
                    "{}, expected an array of {{id, description}}",
                    kind_of(value)
                ),
            ),
        }
    }

    let mut defs: Vec<TriggerDef> = Vec::new();
    let mut names: Vec<(String, String)> = Vec::new();
    match section.get("triggers") {
        Some(Value::Array(items)) => {
            if items.len() > TRIGGERS_MAX {
                problem(
                    &mut out,
                    "background.triggers",
                    format!("{} triggers; the most is {TRIGGERS_MAX}", items.len()),
                );
            }
            for (at_index, item) in items.iter().enumerate().take(TRIGGERS_MAX) {
                if let Some(def) = walk_trigger(&mut out, &declared, &mut names, at_index, item) {
                    defs.push(def);
                }
            }
        }
        Some(value) => problem(
            &mut out,
            "background.triggers",
            format!("{}, expected an array of triggers", kind_of(value)),
        ),
        None => {}
    }

    if out.is_empty() { Ok(defs) } else { Err(out) }
}
