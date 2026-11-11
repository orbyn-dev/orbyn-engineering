//! `crew_route` through the real MCP service of a real `Core`, on scripted engine drivers.
//! The drivers count every run they are asked to start; a dry run must start none.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use orbyn_contracts::{
    AuthState, CrewObject, EngineTier, ModelInfo, PermissionMode, RouteDecision,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::fake::{caps, draft, model};
use crate::agents::driver::{
    EngineDescriptor, EngineDriver, EngineEvent, EngineProbe, ModelCatalog, RunHandle, RunSpec,
};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::governance::approvals::RunScope;
use crate::testing::TempDir;
use crate::tools::mcp::McpBinding;
use crate::{CoreConfig, store::NewProject, store::NewThread};

struct Scripted {
    id: &'static str,
    installed: bool,
    auth: AuthState,
    models: Vec<ModelInfo>,
    used_percent: Option<f64>,
    starts: Arc<AtomicUsize>,
}

fn scripted(id: &'static str, starts: &Arc<AtomicUsize>) -> Scripted {
    Scripted {
        id,
        installed: true,
        auth: AuthState::NotApplicable,
        models: vec![model(&format!("{id}-model"), id, true)],
        used_percent: None,
        starts: starts.clone(),
    }
}

#[async_trait]
impl EngineDriver for Scripted {
    fn descriptor(&self) -> EngineDescriptor {
        let mut capabilities = caps();
        capabilities.permission_modes = PermissionMode::ALL.to_vec();
        EngineDescriptor {
            id: self.id.into(),
            name: self.id.into(),
            tier: EngineTier::Acp,
            capabilities,
        }
    }

    async fn probe(&self) -> EngineProbe {
        EngineProbe {
            path: self.installed.then(|| "/bin/sh".into()),
            version: Some("1.0.0".into()),
            auth: Some(self.auth),
            ..EngineProbe::default()
        }
    }

    async fn models(&self) -> Result<Option<ModelCatalog>> {
        Ok(Some(ModelCatalog::engine(self.models.clone())))
    }

    async fn limits(&self) -> Result<Option<Value>> {
        Ok(self.used_percent.map(|used| {
            json!({ "windows": [{ "id": "w", "label": "5h", "used_percent": used,
                "window_minutes": 300, "resets_at": null }] })
        }))
    }

    async fn start_run(&self, _: RunSpec, _: mpsc::Sender<EngineEvent>) -> Result<RunHandle> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Err(CoreError::Invalid("a dry run starts nothing".into()))
    }
}

struct Fixture {
    _dir: TempDir,
    core: Arc<Core>,
    token: String,
    starts: Arc<AtomicUsize>,
    project: String,
}

fn fixture(preview: bool, build: impl FnOnce(&Arc<AtomicUsize>) -> Vec<Scripted>) -> Fixture {
    let dir = TempDir::new();
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    // The real Claude and Codex are not on this test's machine, whatever the developer has.
    config.engines.claude_program = "orbyn-test-no-such-claude".into();
    config.engines.codex_program = "orbyn-test-no-such-codex".into();
    config.engines.known_engine_ids = Some(Vec::new());
    config.crew_preview = preview;
    let core = Arc::new(Core::open(config).unwrap());
    core.mcp.bind_host(&core);
    let starts = Arc::new(AtomicUsize::new(0));
    for driver in build(&starts) {
        core.engines.register(Arc::new(driver));
    }
    let project = core
        .store
        .add_project(NewProject {
            path: "/project",
            name: "project",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "test").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id.clone(),
            title: "lead".into(),
            engine: "alpha".into(),
            model: None,
            cwd: "/project".into(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let token = core
        .mcp
        .register(McpBinding {
            scope: RunScope {
                run_id: "run-lead".into(),
                thread_id: thread.id,
                engine: "alpha".into(),
                family: "alpha".into(),
                write_scope: None,
                mode: PermissionMode::Supervised,
                plan: false,
            },
            permission_prompt: false,
            skills: Vec::new(),
        })
        .unwrap();
    Fixture {
        _dir: dir,
        core,
        token,
        starts,
        project: project.id,
    }
}

impl Fixture {
    async fn rpc(&self, method: &str, params: Value) -> Value {
        let message = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        self.core
            .mcp
            .handle(&self.token, &message)
            .await
            .unwrap()
            .unwrap()
    }

    /// Calls `crew_route`; returns the tool result's JSON and whether it is an error.
    async fn route(&self, arguments: Value) -> (Value, bool) {
        let reply = self
            .rpc(
                "tools/call",
                json!({ "name": "crew_route", "arguments": arguments }),
            )
            .await;
        let result = &reply["result"];
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{reply}"));
        (
            serde_json::from_str(text).unwrap(),
            result["isError"] == json!(true),
        )
    }

    fn count(&self, table: &str) -> i64 {
        self.core
            .store
            .reader()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}

fn three(starts: &Arc<AtomicUsize>) -> Vec<Scripted> {
    let mut signed_out = scripted("bravo", starts);
    signed_out.auth = AuthState::LoggedOut;
    let mut absent = scripted("charlie", starts);
    absent.installed = false;
    vec![scripted("alpha", starts), signed_out, absent]
}

fn names(reply: &Value) -> Vec<String> {
    reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn with_the_preview_flag_off_the_tool_does_not_exist() {
    let f = fixture(false, three);
    assert!(!f.core.crew_enabled());
    assert!(!names(&f.rpc("tools/list", json!({})).await).contains(&"crew_route".to_owned()));
    let reply = f
        .rpc(
            "tools/call",
            json!({ "name": "crew_route", "arguments": { "brief": draft(json!({})) } }),
        )
        .await;
    assert_eq!(reply["error"]["message"], "Unknown tool crew_route.");
    assert!(!CoreConfig::new("/x".into(), "http://127.0.0.1:1".into()).crew_preview);
}

#[tokio::test]
async fn the_dry_run_routes_from_the_machines_own_engines_and_starts_nothing() {
    let f = fixture(true, three);
    assert!(names(&f.rpc("tools/list", json!({})).await).contains(&"crew_route".to_owned()));
    let threads = f
        .core
        .store
        .list_threads(Some(&f.project), false)
        .unwrap()
        .len();

    let (answer, error) = f.route(json!({ "brief": draft(json!({})) })).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["dry_run"], true);
    assert_eq!(answer["routed"], true);
    let decision = &answer["decision"];
    assert_eq!(decision["engine"], "alpha");
    assert_eq!(decision["model"], "alpha-model");
    assert_eq!(decision["effort"], "medium");
    // Valid protocol object whose id is its hash, written by the core.
    assert!(matches!(
        CrewObject::parse(decision),
        Ok(CrewObject::RouteDecision(_))
    ));
    assert_eq!(decision["author"]["role"], "core");
    // Engines the machine has not got are rejected with reasons; none is routed to.
    let rejected: Vec<(String, String)> = decision["rejected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["engine"].as_str().unwrap().into(),
                r["why"].as_str().unwrap().into(),
            )
        })
        .collect();
    for pair in [
        ("bravo", "not_signed_in"),
        ("charlie", "not_installed"),
        ("claude", "not_installed"),
        ("codex", "not_installed"),
    ] {
        assert!(
            rejected.contains(&(pair.0.into(), pair.1.into())),
            "{rejected:?}"
        );
    }
    assert_eq!(answer["alternatives"], json!([]));

    // Nothing started, nothing stored.
    assert_eq!(f.starts.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.core
            .store
            .list_threads(Some(&f.project), false)
            .unwrap()
            .len(),
        threads
    );
    assert_eq!(f.count("crew_tasks"), 0);
    assert_eq!(f.count("crew_route_decisions"), 0);
}

#[tokio::test]
async fn an_explicit_engine_is_checked_and_a_refusal_says_why() {
    let f = fixture(true, three);
    let ask = |id: &str| json!({ "brief": draft(json!({ "engine": { "id": id, "assigned_by": "person", "tag": format!("@{id}") } })) });
    let (answer, error) = f.route(ask("bravo")).await;
    assert!(!error);
    assert_eq!(answer["routed"], false);
    assert_eq!(answer["refusal"]["message"], "bravo isn't signed in.");
    assert_eq!(answer["refusal"]["requested"]["engine"], "bravo");
    assert_eq!(answer["refusal"]["rejected"][0]["why"], "not_signed_in");

    let (answer, _) = f.route(ask("cursor")).await;
    assert_eq!(answer["routed"], false);
    assert_eq!(answer["refusal"]["rejected"][0]["why"], "not_installed");

    let (answer, _) = f.route(ask("alpha")).await;
    assert_eq!(answer["decision"]["assigned_by"], "person");
    assert_eq!(f.starts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn usage_the_default_engine_and_turned_off_engines_come_from_the_machine() {
    let f = fixture(true, |starts| {
        let mut full = scripted("alpha", starts);
        full.used_percent = Some(100.0);
        vec![full, scripted("bravo", starts), scripted("delta", starts)]
    });
    let (answer, _) = f.route(json!({ "brief": draft(json!({})) })).await;
    let why = |engine: &str| {
        answer["decision"]["rejected"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["engine"] == engine)
            .map(|r| r["why"].clone())
    };
    assert_eq!(why("alpha"), Some(json!("quota_exhausted")));
    // Two equal engines: the id decides, until the person's default engine is set.
    assert_eq!(answer["decision"]["engine"], "bravo");
    f.core
        .set_preferences(
            None,
            &[("default_engine".to_owned(), json!("delta"))].into(),
        )
        .unwrap();
    let (answer, _) = f.route(json!({ "brief": draft(json!({})) })).await;
    assert_eq!(answer["decision"]["engine"], "delta");
    assert_eq!(answer["alternatives"].as_array().unwrap().len(), 1);

    // Turned off for this project: rejected with that reason.
    f.core
        .set_preferences(
            Some(&f.project),
            &[("disabled_engines".to_owned(), json!(["delta"]))].into(),
        )
        .unwrap();
    let (answer, _) = f.route(json!({ "brief": draft(json!({})) })).await;
    assert_eq!(answer["decision"]["engine"], "bravo");
    let off = answer["decision"]["rejected"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["engine"] == "delta")
        .unwrap();
    assert!(off["text"].as_str().unwrap().contains("turned off"));
}

#[tokio::test]
async fn bad_input_is_refused_with_the_protocols_error_codes() {
    let f = fixture(true, three);
    let code = |answer: &Value| answer["code"].as_str().unwrap().to_owned();
    // An answer key (or any unknown field) in the draft.
    let (answer, error) = f
        .route(json!({ "brief": draft(json!({ "answer_key": { "checks": [] } })) }))
        .await;
    assert!(error);
    assert_eq!(code(&answer), "invalid_object");
    // An envelope field the core stamps.
    let (answer, error) = f
        .route(json!({ "brief": draft(json!({ "id": "sha256:x" })) }))
        .await;
    assert!(error);
    assert_eq!(code(&answer), "invalid_object");
    // A key or other argument the tool does not take.
    let (answer, _) = f
        .route(json!({ "brief": draft(json!({})), "answer_key": {} }))
        .await;
    assert_eq!(code(&answer), "invalid_object");
    // A secret anywhere in the draft is refused without echoing it.
    let secret = "sk-abcdefghijklmnopqrstuvwxyz0123";
    let (answer, error) = f
        .route(json!({ "brief": draft(json!({ "outcome": format!("Use {secret}") })) }))
        .await;
    assert!(error);
    assert_eq!(code(&answer), "secret_detected");
    assert!(!answer.to_string().contains(secret));
    // A role the router does not have.
    let (answer, _) = f
        .route(json!({ "brief": draft(json!({})), "role": "boss" }))
        .await;
    assert_eq!(code(&answer), "invalid_object");
    assert_eq!(f.starts.load(Ordering::SeqCst), 0);
}

fn example_brief() -> orbyn_contracts::Brief {
    let text =
        include_str!("../../../../../docs/crew/protocol/schemas/examples/brief.valid.basic.json");
    match CrewObject::parse(&serde_json::from_str(text).unwrap()).unwrap() {
        CrewObject::Brief(brief) => brief,
        _ => unreachable!(),
    }
}

fn example_key() -> orbyn_contracts::AnswerKey {
    let text = include_str!(
        "../../../../../docs/crew/protocol/schemas/examples/answer-key.valid.basic.json"
    );
    match CrewObject::parse(&serde_json::from_str(text).unwrap()).unwrap() {
        CrewObject::AnswerKey(key) => key,
        _ => unreachable!(),
    }
}

/// The example brief without the engine and needs it asks for, so a fake can take it.
fn test_brief() -> orbyn_contracts::Brief {
    let mut brief = example_brief();
    brief.engine = None;
    brief.needs = None;
    brief.id = CrewObject::Brief(brief.clone()).computed_id().unwrap();
    brief
}

fn test_key(brief: &orbyn_contracts::Brief) -> orbyn_contracts::AnswerKey {
    let mut key = example_key();
    key.brief = brief.id.clone();
    key.id = CrewObject::AnswerKey(key.clone()).computed_id().unwrap();
    key
}

fn decide(brief: &orbyn_contracts::Brief, model_id: &str) -> RouteDecision {
    let mut view = super::fake::engine("zed");
    view.models.as_mut().unwrap()[0].id = model_id.into();
    super::fake::routed(brief, &[view], &super::fake::Ask::default())
}

#[test]
fn a_decision_is_stored_with_its_task_and_shown_later() {
    let dir = TempDir::new();
    let core = Core::open(CoreConfig::new(
        dir.path().join("data"),
        "http://127.0.0.1:1".into(),
    ))
    .unwrap();
    let brief = test_brief();
    let task = core
        .store
        .create_crew_task(&brief, &test_key(&brief), "run-01")
        .unwrap();
    let decision = decide(&brief, "mini-1");
    assert_eq!(decision.brief, brief.id);

    core.store_route_decision(&task.id, &decision).unwrap();
    core.store_route_decision(&task.id, &decision).unwrap();
    assert_eq!(
        core.store.crew_route_decisions(&task.id).unwrap(),
        vec![decision.clone()]
    );
    // Storing does not move the lifecycle: that is the next slice's.
    assert_eq!(
        core.store.crew_task(&task.id).unwrap().unwrap().state,
        "briefed"
    );

    // A decision that would carry the key's text is refused, and nothing is stored for it.
    let leaky = decide(&brief, "cargo test -p crew-demo");
    let error = core.store_route_decision(&task.id, &leaky).unwrap_err();
    assert!(error.to_string().contains("AnswerKeyForbidden"), "{error}");
    assert_eq!(core.store.crew_route_decisions(&task.id).unwrap().len(), 1);

    // A decision for another brief is not part of this task.
    let mut other = test_brief();
    other.outcome = "Something else entirely.".into();
    other.id = CrewObject::Brief(other.clone()).computed_id().unwrap();
    let stray = decide(&other, "mini-1");
    assert!(core.store_route_decision(&task.id, &stray).is_err());
}
