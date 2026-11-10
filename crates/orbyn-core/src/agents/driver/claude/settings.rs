use std::path::Path;

use serde_json::{Value, json};

use crate::agents::driver::{MCP_SERVER_NAME, RunSpec};
use crate::agents::selection::FAST_MODE;

pub(super) fn run_settings_path(spec: &RunSpec) -> std::path::PathBuf {
    spec.run_dir.join("settings.json")
}

/// Tools that write, run commands or reach the network. Each is forced to ask in a run's own `--settings`, because
/// an `ask` rule beats an `allow` from any other source (checked live on 2.1.318: `allow Write` alone skipped the
/// prompt tool, `allow` plus `ask` reached it). Read-only tools are left alone.
pub(super) const GUARDED_TOOLS: &[&str] = &["Bash", "Edit", "Write", "NotebookEdit", "WebFetch"];

/// The library settings a run or a console takes over. Never `permissions`, `hooks`, `apiKeyHelper`,
/// `statusLine` or anything else that decides a call or runs a command.
const LIBRARY_SETTING_KEYS: &[&str] = &["fastMode", "model", "effortLevel"];
const LIBRARY_ENV_KEYS: &[&str] = &[
    "CLAUDE_CODE_DISABLE_FAST_MODE",
    "CLAUDE_CODE_MAX_CONCURRENT_SUBAGENTS",
    "CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH",
];

/// What every Orbyn-started Claude gets in `--settings`, whatever the checkout's own settings say: the allowlisted
/// library settings, then the two protections. Hooks are off, because a `PreToolUse` hook that answers allow is
/// decided before the permission tool is asked, and every guarded tool asks, so a project `allow` rule cannot
/// skip the prompt. These come last so nothing before them can override them.
pub(super) fn base_settings(library: &Path) -> serde_json::Map<String, Value> {
    let mut settings = serde_json::Map::new();
    if let Some(from_library) = library_settings(library) {
        for key in LIBRARY_SETTING_KEYS {
            if let Some(value) = from_library.get(*key) {
                settings.insert((*key).into(), value.clone());
            }
        }
        if let Some(Value::Object(env)) = from_library.get("env") {
            let kept: serde_json::Map<String, Value> = env
                .iter()
                .filter(|(name, value)| {
                    LIBRARY_ENV_KEYS.contains(&name.as_str()) && value.is_string()
                })
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            if !kept.is_empty() {
                settings.insert("env".into(), Value::Object(kept));
            }
        }
    }
    guard(&mut settings);
    settings
}

pub(super) fn guard(settings: &mut serde_json::Map<String, Value>) {
    settings.insert("disableAllHooks".into(), json!(true));
    settings.insert("permissions".into(), json!({ "ask": GUARDED_TOOLS }));
}

/// The settings a run passes with `--settings`. Fast mode reaches only a model that reports `supportsFastMode`,
/// because admission refuses the option anywhere else.
pub(super) fn run_settings(spec: &RunSpec, window: Option<u32>) -> Option<Value> {
    let mut settings = base_settings(&spec.library);
    if let Some(fast) = spec.options.get(FAST_MODE) {
        settings.insert("fastMode".into(), json!(fast == "on"));
    }
    if let Some(window) = window {
        settings.insert("autoCompactWindow".into(), json!(window));
    }
    Some(Value::Object(settings))
}

/// The library's own Claude settings, as the library tracks them.
fn library_settings(library: &Path) -> Option<serde_json::Map<String, Value>> {
    let text = read_capped(&library.join(".claude/settings.json"), MAX_SETTINGS_BYTES)?;
    match serde_json::from_str::<Value>(&text).ok()? {
        Value::Object(map) if !map.is_empty() => Some(map),
        _ => None,
    }
}

const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
pub(super) const MAX_AGENTS: usize = 64;
const MAX_AGENT_FILE_BYTES: u64 = 64 * 1024;
const MAX_AGENTS_BYTES: usize = 256 * 1024;
/// The effort levels `--agents` accepts (checked on 2.1.318, which rejects any other value).
const AGENT_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

fn read_capped(path: &Path, max: u64) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > max {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// A front matter's top-level `key: value` pairs. Values may be quoted, continue on indented lines (a plain
/// multi-line scalar folds to one line), be a `|` or `>` block, or be a `- item` list (joined with commas).
fn front_matter(front: &str) -> std::collections::BTreeMap<String, String> {
    let lines: Vec<&str> = front.lines().collect();
    let mut fields = std::collections::BTreeMap::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        at += 1;
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let Some((key, value)) = line.split_once(':').filter(|_| !indented) else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            continue;
        }
        let mut rest: Vec<&str> = Vec::new();
        while at < lines.len()
            && (lines[at].starts_with(' ')
                || lines[at].starts_with('\t')
                || lines[at].trim().is_empty())
        {
            rest.push(lines[at].trim());
            at += 1;
        }
        while rest.last().is_some_and(|line| line.is_empty()) {
            rest.pop();
        }
        let value = value.trim();
        let text = if value.starts_with('|') {
            rest.join("\n")
        } else if value.starts_with('>') {
            rest.iter()
                .copied()
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        } else if rest.iter().any(|line| line.starts_with("- ")) && value.is_empty() {
            rest.iter()
                .filter_map(|line| line.strip_prefix("- "))
                .map(|item| unquote(item.trim()))
                .collect::<Vec<_>>()
                .join(",")
        } else {
            let mut folded = vec![value];
            folded.extend(rest.iter().copied());
            unquote(
                &folded
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        };
        fields.insert(key.to_owned(), text);
    }
    fields
}

fn unquote(text: &str) -> String {
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        text[1..text.len() - 1].to_owned()
    } else {
        text.to_owned()
    }
}

/// The library's subagents (`.claude/agents/*.md`) as the `--agents` JSON, since `--add-dir` loads skills and
/// nothing else. Only the fields that option takes are carried over. At most 64 agents of 64 KiB each, 256 KiB of
/// prompts in all; anything past that is left out rather than failing the run.
pub(super) fn library_agents(library: &Path) -> Option<Value> {
    let mut files: Vec<_> = std::fs::read_dir(library.join(".claude/agents"))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    files.sort();
    let mut agents = serde_json::Map::new();
    let mut total = 0usize;
    for path in files {
        if agents.len() >= MAX_AGENTS {
            break;
        }
        let Some(text) = read_capped(&path, MAX_AGENT_FILE_BYTES) else {
            continue;
        };
        let text = text.replace("\r\n", "\n");
        let Some(rest) = text.strip_prefix("---\n") else {
            continue;
        };
        let Some((front, body)) = rest
            .split_once("\n---\n")
            .or_else(|| rest.strip_suffix("\n---").map(|front| (front, "")))
        else {
            continue;
        };
        let fields = front_matter(front);
        let stem = path.file_stem().and_then(|stem| stem.to_str());
        let Some(name) = fields.get("name").map(String::as_str).or(stem) else {
            continue;
        };
        let prompt = body.trim();
        if total + prompt.len() > MAX_AGENTS_BYTES {
            break;
        }
        total += prompt.len();
        let mut agent = serde_json::Map::new();
        agent.insert(
            "description".into(),
            json!(
                fields
                    .get("description")
                    .map(String::as_str)
                    .unwrap_or(name)
            ),
        );
        agent.insert("prompt".into(), json!(prompt));
        if let Some(model) = fields.get("model") {
            agent.insert("model".into(), json!(model));
        }
        if let Some(effort) = fields
            .get("effort")
            .filter(|effort| AGENT_EFFORTS.contains(&effort.as_str()))
        {
            agent.insert("effort".into(), json!(effort));
        }
        for key in ["tools", "disallowedTools"] {
            if let Some(list) = fields.get(key) {
                let names: Vec<&str> = list
                    .split(',')
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .collect();
                agent.insert(key.into(), json!(names));
            }
        }
        agents.insert(name.to_owned(), Value::Object(agent));
    }
    (!agents.is_empty()).then_some(Value::Object(agents))
}

/// MCP servers other than our own that a Claude run has seen, by the name its init frame
/// gave them. Remembered for the life of the service so the next run's approval guard
/// can name them; the wildcard in the guard covers a server not yet seen.
pub(super) fn seen_servers() -> &'static std::sync::Mutex<std::collections::BTreeSet<String>> {
    static SEEN: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeSet<String>>> =
        std::sync::OnceLock::new();
    SEEN.get_or_init(Default::default)
}

/// Most server names remembered for the guard; the `mcp__*` rule covers the rest.
pub(super) const MAX_SEEN_SERVERS: usize = 64;

/// A server's name as it appears in tool names and permission rules: anything but a
/// letter, digit, `_` or `-` becomes `_`, so `claude.ai Vercel` is `claude_ai_Vercel`.
pub(super) fn rule_name(server: &str) -> String {
    server
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Run settings plus the approval guard. The guard is `permissions.ask` for every MCP
/// server but our own, because an `ask` rule outranks an `allow` rule, so a connector
/// tool the person allowed in their own settings still reaches our permission tool
/// instead of running unseen. `mcp__*` covers any server, including one not yet seen
/// (checked live on 2.1.318, as were the per-server and `mcp__claude_ai_*` forms); the
/// per-server rules cover a CLI that does not read the wildcard. Our own tools answer
/// themselves in `approve`, so they pass through the guard unchanged.
pub(super) fn launch_settings(spec: &RunSpec, window: Option<u32>) -> Option<Value> {
    let mut settings = match run_settings(spec, window) {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let mut ask: Vec<String> = vec!["mcp__*".into()];
    ask.extend(
        seen_servers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|name| **name != MCP_SERVER_NAME)
            .map(|name| format!("mcp__{name}")),
    );
    let permissions = settings
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()?;
    let existing = permissions
        .entry("ask")
        .or_insert_with(|| json!([]))
        .as_array_mut()?;
    for rule in ask {
        if !existing.iter().any(|entry| entry == &json!(rule)) {
            existing.push(json!(rule));
        }
    }
    Some(Value::Object(settings))
}
