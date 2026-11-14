//! Fake engines and briefs. An engine is a plain `EngineView` the way the registry would
//! describe it; nothing is installed or signed in unless a test says so.

use orbyn_contracts::{
    AuthState, Brief, CrewId, CrewMember, CrewTimestamp, CrewToken, EngineCapabilities, EngineTier,
    ModelInfo, PermissionMode, RouteDecision,
};
use serde_json::{Value, json};

use crate::crew::{
    EngineView, EvalHistory, RouteFailure, RouteInput, RouteRefusal, RouterConfig, Stamper, route,
};

pub fn caps() -> EngineCapabilities {
    EngineCapabilities {
        streaming: true,
        approvals: true,
        resume: true,
        mcp: true,
        sandbox: true,
        managed: true,
        skill_delivery: Vec::new(),
        goal_control: false,
        decline_feedback: false,
        permission_modes: PermissionMode::ALL.to_vec(),
    }
}

pub fn model(id: &str, label: &str, default: bool) -> ModelInfo {
    ModelInfo {
        id: id.into(),
        label: label.into(),
        description: None,
        is_default: default,
        custom: false,
        options: Vec::new(),
        input_modalities: vec!["text".into()],
        upgrade: None,
    }
}

pub fn with_image(mut model: ModelInfo) -> ModelInfo {
    model.input_modalities.push("image".into());
    model
}

/// A signed-in, installed, managed engine with the models its family would list.
pub fn engine(id: &str) -> EngineView {
    let models = match id {
        "claude" => vec![
            model("claude-opus-4", "Opus 4", false),
            model("claude-sonnet-4", "Sonnet 4", true),
            model("claude-haiku-4", "Haiku 4", false),
        ],
        "codex" => vec![
            model("sol-1", "Sol", false),
            model("terra-1", "Terra", true),
            model("luna-1", "Luna", false),
        ],
        _ => vec![model("mini-1", "Mini", true), model("big-1", "Big", false)],
    };
    EngineView {
        id: CrewToken::new(id).unwrap(),
        name: match id {
            "claude" => "Claude".to_owned(),
            "codex" => "Codex".to_owned(),
            "opencode" => "OpenCode".to_owned(),
            other => other.to_owned(),
        },
        family: id.to_owned(),
        tier: if matches!(id, "claude" | "codex") {
            EngineTier::Native
        } else {
            EngineTier::Acp
        },
        turned_off: false,
        installed: true,
        auth: if matches!(id, "claude" | "codex") {
            AuthState::LoggedIn
        } else {
            AuthState::NotApplicable
        },
        broken: None,
        capabilities: caps(),
        models: Some(models),
        used_percent: None,
    }
}

pub fn stamper() -> Stamper {
    Stamper {
        crew_id: CrewId::new("crew-01").unwrap(),
        engine: CrewToken::new("claude").unwrap(),
        member: CrewMember::new("lead-01").unwrap(),
        now: CrewTimestamp::new("2026-11-12T09:00:00Z").unwrap(),
    }
}

/// An implement brief that writes, the common case. `extra` replaces or adds fields.
pub fn draft(extra: Value) -> Value {
    let mut draft = json!({
        "kind": "implement",
        "outcome": "Add a parse_duration function.",
        "inputs": [],
        "files": [{ "path": "crates/demo/src/**", "mode": "write" }],
        "allowed_effects": ["read_repo", "write_leased", "run_checks"],
        "stop_when": ["The function is added."],
        "budget": { "turns": 20, "minutes": 30 }
    });
    for (key, value) in extra.as_object().cloned().unwrap_or_default() {
        draft[key] = value;
    }
    draft
}

pub fn brief(extra: Value) -> Brief {
    stamper().brief(&draft(extra)).unwrap()
}

/// A read-only brief.
pub fn reading(extra: Value) -> Brief {
    let mut base = json!({ "files": [], "allowed_effects": ["read_repo"] });
    for (key, value) in extra.as_object().cloned().unwrap_or_default() {
        base[key] = value;
    }
    brief(base)
}

/// What a test varies besides the brief and the engines.
#[derive(Default)]
pub struct Ask {
    pub role: Option<orbyn_contracts::RouteRole>,
    pub lane: Option<&'static str>,
    pub author: Option<&'static str>,
    pub default_engine: Option<&'static str>,
    pub history: EvalHistory,
}

pub fn run(
    brief: &Brief,
    engines: &[EngineView],
    ask: &Ask,
) -> Result<RouteDecision, RouteFailure> {
    route(
        &RouteInput {
            brief,
            run_id: CrewId::new("run-01").unwrap(),
            created_at: CrewTimestamp::new("2026-11-12T09:00:00Z").unwrap(),
            role: ask.role,
            lane: ask.lane.map(|l| CrewToken::new(l).unwrap()),
            author_engine: ask.author.map(|a| CrewToken::new(a).unwrap()),
            default_engine: ask.default_engine.map(str::to_owned),
            classes: None,
        },
        engines,
        &RouterConfig::default(),
        &ask.history,
    )
}

pub fn routed(brief: &Brief, engines: &[EngineView], ask: &Ask) -> RouteDecision {
    match run(brief, engines, ask) {
        Ok(decision) => decision,
        Err(other) => panic!("expected a route, got {other:?}"),
    }
}

pub fn refused(brief: &Brief, engines: &[EngineView], ask: &Ask) -> RouteRefusal {
    match run(brief, engines, ask) {
        Err(RouteFailure::Refused(refusal)) => refusal,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// Every decision must be a valid protocol object whose id is its hash.
pub fn verified(decision: &RouteDecision) {
    let value = orbyn_contracts::CrewObject::RouteDecision(decision.clone()).to_value();
    orbyn_contracts::CrewObject::parse(&value).expect("a valid route decision");
}
