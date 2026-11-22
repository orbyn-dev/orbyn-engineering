//! The ACP family on the engine host: the agent and options `open` carries, the
//! `session/update` mapping, asks, run ends, sessions and models. The stand-in speaks the
//! host protocol; the checks here are the core's side of the wire.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::AgUiEvent;
use serde_json::json;

use super::*;
use crate::agents::driver::acp::{AcpDriver, UpdateMapper};
use crate::agents::driver::host::{
    HostAcpAgent, HostConfig, HostDriver, HostHub, HostState, NotUsed,
};
use crate::agents::driver::{
    EngineDriver, EngineEvent, EngineOutcome, Failure, PermissionRequest, RunSpec, SteerMessage,
    ThreadDisposition,
};
use crate::conversation::questions::Answers;
use crate::error::CoreError;
use crate::governance::approvals::Verdict;
use crate::testing::TempDir;
use orbyn_contracts::{CustomEngine, EngineTier};

fn probe_program() -> String {
    std::env::current_exe()
        .expect("the test binary has a path")
        .to_string_lossy()
        .into_owned()
}

fn engine(id: &str) -> CustomEngine {
    CustomEngine {
        id: id.into(),
        name: "ACP test agent".into(),
        tier: EngineTier::Acp,
        target: probe_program(),
        args: Vec::new(),
    }
}

fn inner(id: &str) -> Arc<dyn crate::agents::driver::EngineDriver> {
    Arc::new(AcpDriver::new(engine(id)))
}

fn agent() -> HostAcpAgent {
    HostAcpAgent::new("default", probe_program(), Vec::new(), Vec::new(), None)
}

fn acp_driver(rig: &Rig, program: &str, options_agent: HostAcpAgent) -> HostDriver {
    HostDriver::for_acp(
        Arc::new(AcpDriver::new(CustomEngine {
            id: program.into(),
            name: "ACP test agent".into(),
            tier: EngineTier::Acp,
            target: probe_program(),
            args: Vec::new(),
        })),
        rig.pool.clone(),
        options_agent,
    )
}

fn acp_rig(env: &[(&str, &str)]) -> Option<Rig> {
    standin_family("acp", env, fast())
}

fn acp_spec(prompt: &str) -> RunSpec {
    let mut spec = spec(prompt);
    spec.model = Some("alpha".into());
    spec.effort = Some("high".into());
    spec.options
        .insert(crate::agents::selection::MODE.into(), "ask".into());
    spec
}

#[tokio::test]
async fn the_flag_off_keeps_the_rust_driver_and_reports_the_family_as_not_used() {
    let config = HostConfig::from_env(Some("/x/orbyn-engine-host".into()), None).unwrap();
    assert!(!config.acp);
    let hub = HostHub::new(&config);
    let rust = inner("opencode");
    let wrapped = hub.wrap_acp(rust.clone(), agent());
    assert!(
        Arc::ptr_eq(&wrapped, &rust),
        "a flag-off ACP family is not wrapped"
    );
    let status = hub
        .statuses()
        .await
        .into_iter()
        .find(|status| status.family == "acp")
        .expect("the acp family is listed");
    assert_eq!(status.state, HostState::NotUsed(NotUsed::FlagOff));
    // The host wants exactly `1`, as the other family flags do.
    let on = config.clone().with_acp(Some("true"));
    assert!(!on.acp);
    let on = config.with_acp(Some("1"));
    assert!(on.acp);
    let hub = HostHub::new(&on);
    if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        return;
    }
    let hosted = hub.wrap_acp(rust.clone(), agent());
    assert!(!Arc::ptr_eq(&hosted, &rust), "the flag wraps ACP engines");
}

#[tokio::test]
async fn open_carries_the_agent_options_and_the_host_then_maps_its_updates() {
    let updates = json!([
        { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Hel" } },
        { "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "lo" } },
        { "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "think" } },
        { "sessionUpdate": "tool_call", "toolCallId": "tool-1", "title": "Read", "rawInput": { "path": "a.txt" } },
        { "sessionUpdate": "tool_call_update", "toolCallId": "tool-1", "status": "completed", "rawOutput": "done" },
        { "sessionUpdate": "plan", "entries": [{ "content": "step", "status": "pending" }] }
    ]);
    let fixture = TempDir::new();
    let file = fixture.path().join("updates.json");
    std::fs::write(&file, updates.to_string()).unwrap();
    let updates_file = file.to_string_lossy().into_owned();
    let Some(rig) = acp_rig(&[("STANDIN_ACP_UPDATES", &updates_file)]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let (events, outcome) = start(&driver, acp_spec("[acp-updates]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);

    let open = rig
        .log()
        .into_iter()
        .find(|line| line["type"] == "open")
        .expect("open went to the host");
    let options = &open["options"];
    assert_eq!(options["agent"]["id"], "default");
    assert_eq!(options["agent"]["args"], json!([]));
    assert!(
        options["agent"]["command"]
            .as_str()
            .is_some_and(|path| { PathBuf::from(path).is_file() })
    );
    assert_eq!(options["model"], "alpha");
    assert_eq!(options["effort"], "high");
    assert_eq!(options["mode"], "ask");
    assert!(options["mcpServers"].is_array());

    // The stand-in replays the recorded sequence; the core must map it exactly as
    // the native ACP mapper does, including one id per stream.
    let updates = updates.as_array().unwrap().clone();
    let mut native = UpdateMapper::default();
    let mut expected = Vec::new();
    for update in &updates {
        expected.extend(native.on_update(update));
    }
    expected.extend(native.close());
    let expected: Vec<AgUiEvent> = expected
        .into_iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(event) => Some(event),
            _ => None,
        })
        .collect();
    let actual: Vec<AgUiEvent> = events
        .into_iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(event) => Some(event),
            _ => None,
        })
        .collect();
    assert_eq!(tags(&actual), tags(&expected), "{actual:?}");
    assert_eq!(contents(&actual), contents(&expected), "{actual:?}");
    let text_ids: Vec<&str> = actual
        .iter()
        .filter_map(|event| match event {
            AgUiEvent::TextMessageContent { message_id, .. } => Some(message_id.as_str()),
            _ => None,
        })
        .collect();
    assert!(text_ids.len() > 1);
    assert!(text_ids.iter().all(|id| *id == text_ids[0]), "{text_ids:?}");
    let reasoning_ids: Vec<&str> = actual
        .iter()
        .filter_map(|event| match event {
            AgUiEvent::ReasoningMessageContent { message_id, .. } => Some(message_id.as_str()),
            _ => None,
        })
        .collect();
    assert!(reasoning_ids.iter().all(|id| *id == reasoning_ids[0]));
    rig.pool.shutdown().await;
}

fn tags(events: &[AgUiEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| {
            serde_json::to_value(event).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

fn contents(events: &[AgUiEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            AgUiEvent::TextMessageContent { delta, .. }
            | AgUiEvent::ReasoningMessageContent { delta, .. } => Some(delta.clone()),
            _ => None,
        })
        .collect()
}

async fn permission(driver: &HostDriver, prompt: &str) -> (Started, PermissionRequest) {
    let mut started = start(driver, acp_spec(prompt)).await.unwrap();
    let ask = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match started.events.recv().await {
                Some(EngineEvent::Permission(ask)) => return ask,
                Some(_) => {}
                None => panic!("the run ended before asking"),
            }
        }
    })
    .await
    .expect("the permission never arrived");
    (started, ask)
}

#[tokio::test]
async fn a_permission_allow_round_trips_and_the_host_picks_allow_once() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let (started, ask) = permission(&driver, "[acp-perm]").await;
    assert_eq!(ask.tool, "Edit");
    assert_eq!(ask.input["toolCall"]["title"], "Edit");
    let options = ask.options.as_ref().unwrap().as_array().unwrap();
    assert!(options.iter().any(|option| option["kind"] == "allow_once"));
    assert!(options.iter().any(|option| option["kind"] == "reject_once"));
    ask.reply.send(Verdict::Allow).unwrap();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let log = rig.log();
    assert_eq!(
        log.iter().find(|line| line["type"] == "respond").unwrap()["decision"]["behavior"],
        "allow"
    );
    assert_eq!(
        log.iter()
            .find(|line| line["type"] == "acp-select")
            .unwrap()["selected"],
        "allow_once"
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_permission_deny_round_trips_and_the_host_picks_reject_once() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let (started, ask) = permission(&driver, "[acp-perm]").await;
    ask.reply.send(Verdict::Deny("no".into())).unwrap();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let log = rig.log();
    assert_eq!(
        log.iter().find(|line| line["type"] == "respond").unwrap()["decision"]["behavior"],
        "deny"
    );
    assert_eq!(
        log.iter()
            .find(|line| line["type"] == "acp-select")
            .unwrap()["selected"],
        "reject_once"
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn an_orbyn_tool_is_allowed_without_an_approval() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let (events, outcome) = start(&driver, acp_spec("[acp-own]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, EngineEvent::Permission(_)))
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_question_round_trips_through_the_desk_and_a_skip_is_a_denial() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let mut started = start(&driver, acp_spec("[acp-question]")).await.unwrap();
    let ask = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match started.events.recv().await {
                Some(EngineEvent::Question(ask)) => return ask,
                Some(_) => {}
                None => panic!("the run ended before asking"),
            }
        }
    })
    .await
    .expect("the question never arrived");
    assert_eq!(ask.questions[0].question, "Which one?");
    let mut answers = Answers::new();
    answers.insert("Which one?".into(), vec!["A".into()]);
    ask.reply.send(Some(answers)).unwrap();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let log = rig.log();
    assert_eq!(
        log.iter().find(|line| line["type"] == "respond").unwrap()["decision"]["answers"]["Which one?"],
        "A"
    );

    let (started, ask) = {
        let mut started = start(&driver, acp_spec("[acp-question]")).await.unwrap();
        let ask = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match started.events.recv().await {
                    Some(EngineEvent::Question(ask)) => return ask,
                    Some(_) => {}
                    None => panic!("the run ended before asking"),
                }
            }
        })
        .await
        .expect("the second question never arrived");
        (started, ask)
    };
    ask.reply.send(None).unwrap();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let log = rig.log();
    assert_eq!(
        log.iter()
            .rev()
            .find(|line| line["type"] == "respond")
            .unwrap()["decision"]["behavior"],
        "deny"
    );
    rig.pool.shutdown().await;
}

fn failure(outcome: EngineOutcome) -> Failure {
    match outcome {
        EngineOutcome::FailedWith(failure) => failure,
        other => panic!("not a failure: {other:?}"),
    }
}

#[tokio::test]
async fn the_run_done_reasons_map_as_the_contract_says() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    for (reason, expected) in [
        ("completed", EngineOutcome::Completed),
        ("cancelled", EngineOutcome::Cancelled),
    ] {
        let (_, outcome) = start(&driver, acp_spec(&format!("[acp-reason:{reason}]")))
            .await
            .unwrap()
            .outcome()
            .await;
        assert_eq!(outcome, expected, "{reason}");
    }
    let (_, outcome) = start(&driver, acp_spec("[acp-reason:failed]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(failure(outcome).message, "boom");
    for (reason, sentence) in [
        (
            "maxTurns",
            "The engine stopped because it reached its turn limit.",
        ),
        ("refused", "The engine declined to continue this request."),
    ] {
        let (_, outcome) = start(&driver, acp_spec(&format!("[acp-reason:{reason}]")))
            .await
            .unwrap()
            .outcome()
            .await;
        assert_eq!(failure(outcome).message, sentence, "{reason}");
    }
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn an_auth_failure_says_to_sign_in_in_the_cores_own_words() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let (_, outcome) = start(&driver, acp_spec("[acp-auth]"))
        .await
        .unwrap()
        .outcome()
        .await;
    let failure = failure(outcome);
    assert_eq!(
        failure.message,
        crate::agents::driver::diagnostics::failure_message("authentication required", None)
    );
    assert!(!failure.message.contains("sk-secret"));
    assert!(!failure.message.contains("agent stderr"));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_broken_thread_disposition_is_kept() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let (_, outcome) = start(&driver, acp_spec("[acp-broken]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(failure(outcome).disposition, ThreadDisposition::Broken);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_run_lost_by_the_engine_is_session_lost() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let (_, outcome) = start(&driver, acp_spec("[acp-lost]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::SessionLost);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn models_without_a_session_probes_with_the_agent_and_a_fresh_folder() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let catalog = driver.models().await.unwrap().expect("a catalog");
    assert_eq!(
        catalog
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta"]
    );
    assert!(catalog.models[0].is_default);
    let effort = &catalog.models[0].options[0];
    assert_eq!(effort.id, "effort");
    assert_eq!(
        effort
            .values
            .iter()
            .map(|value| value.id.as_str())
            .collect::<Vec<_>>(),
        ["low", "high"]
    );
    assert!(effort.values[1].default);
    assert!(catalog.models[1].options.is_empty());
    let request = rig
        .log()
        .into_iter()
        .find(|line| line["type"] == "models")
        .expect("models went to the host");
    assert!(request.get("sessionId").is_none());
    assert_eq!(request["agent"]["id"], "default");
    assert!(
        request["cwd"]
            .as_str()
            .is_some_and(|cwd| cwd.contains("orbyn-probe-"))
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn models_with_an_open_session_names_it() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let mut started = start(&driver, acp_spec("[acp-hang]")).await.unwrap();
    let session = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match started.events.recv().await {
                Some(EngineEvent::Session(id)) => return id,
                Some(_) => {}
                None => panic!("the run ended before naming its session"),
            }
        }
    })
    .await
    .expect("the session never arrived");
    driver.models().await.unwrap().expect("a catalog");
    let request = rig
        .log()
        .into_iter()
        .find(|line| line["type"] == "models")
        .expect("models went to the host");
    assert_eq!(request["sessionId"].as_str(), Some(session.as_str()));
    started.handle.cancel.cancel();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn an_agent_that_cannot_resume_gives_the_native_cannot_resume_error() {
    let Some(rig) = acp_rig(&[("STANDIN_ACP_NO_RESUME", "1")]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let mut resume = acp_spec("hi");
    resume.resume = Some("saved-session".into());
    let error = start(&driver, resume)
        .await
        .err()
        .expect("resume must fail");
    assert!(matches!(error, CoreError::Unsupported(_)), "{error:?}");
    assert_eq!(
        error.to_string(),
        "This agent cannot resume its saved session."
    );
    assert!(!rig.log().iter().any(|line| line["type"] == "send"));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_delta_before_the_prompt_is_accepted_never_counts_as_its_reply() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let (events, outcome) = start(&driver, acp_spec("[acp-pre]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert_eq!(texts(&events), "", "{events:?}");
    assert_eq!(leftover_texts(&events), "pre-accept", "{events:?}");
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_steer_reaches_the_host_and_a_refusal_is_recorded() {
    let Some(rig) = acp_rig(&[]) else {
        return;
    };
    let driver = acp_driver(&rig, "opencode", agent());
    let mut started = start(&driver, acp_spec("[acp-hang]")).await.unwrap();
    let input = started.handle.input.as_ref().unwrap();
    input
        .send(SteerMessage {
            message_id: "m-ok".into(),
            prompt: "fine".into(),
        })
        .unwrap();
    input
        .send(SteerMessage {
            message_id: "m-bad".into(),
            prompt: "please reject".into(),
        })
        .unwrap();
    let failed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(EngineEvent::SteerFailed(steer)) = started.events.recv().await {
                return steer;
            }
        }
    })
    .await
    .expect("no SteerFailed");
    assert_eq!(failed.message_id, "m-bad");
    assert!(
        rig.log()
            .iter()
            .any(|line| { line["type"] == "steer" && line["text"] == "please reject" })
    );
    started.handle.cancel.cancel();
    rig.pool.shutdown().await;
}

fn texts(events: &[EngineEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Agui(AgUiEvent::TextMessageContent { delta, .. }) => Some(delta.as_str()),
            _ => None,
        })
        .collect()
}

fn leftover_texts(events: &[EngineEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            EngineEvent::Leftover(inner) => match &**inner {
                EngineEvent::Agui(AgUiEvent::TextMessageContent { delta, .. }) => {
                    Some(delta.as_str())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}
