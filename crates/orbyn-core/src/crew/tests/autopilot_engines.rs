//! Real Crew admission and engine-event approval completion, without provider processes.
use std::sync::Arc;

use orbyn_contracts::{ActionKind, ModelSelection};
use serde_json::json;

use super::autopilot_support::{events, request, words};
use super::harness::{ApprovalTurn, KEY_MARKER, Turn, World, brief, key, world_tiered};
use crate::agents::driver::ApprovalSpec;
use crate::crew::{Mode, ModeScope};
use crate::governance::approvals::Verdict;

const YES: &str = r#"{"approve":true,"reason":"Fits the allowed effects."}"#;

fn short_spec() -> ApprovalSpec {
    ApprovalSpec {
        run_id: String::new(),
        thread_id: String::new(),
        prompt: "Only the summary.".into(),
        family: "alpha".into(),
        model: "alpha-lead".into(),
        effort: None,
        options: Default::default(),
        engine_version: None,
        deadline: std::time::Instant::now() + std::time::Duration::from_millis(500),
        max_output_tokens: 1024,
        max_reported_tokens: 2048,
        estimated_spend_stop_usd: 0.05,
    }
}

async fn running(engines: &[(&str, bool)]) -> (World, String) {
    let w = world_tiered(true, Arc::new(|_, _| Turn::Hang), engines, true).await;
    w.core
        .store
        .switch_thread_engine(
            &w.lead,
            engines[0].0,
            &ModelSelection {
                model: Some(format!("{}-lead", engines[0].0)),
                effort: None,
                options: Default::default(),
            },
            "",
        )
        .unwrap();
    for (engine, _) in engines {
        w.approval_turns
            .lock()
            .unwrap()
            .insert((*engine).into(), ApprovalTurn::Reply(YES.into()));
    }
    w.core.set_crew_engine_approvers();
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    (w, id)
}

#[tokio::test]
async fn two_live_protocol_runs_approve_without_sharing_tools_key_or_transcript() {
    let (w, id) = running(&[("alpha", true), ("beta", false)]).await;
    let verdict = request(&w, &id, ActionKind::Command, "npm install zod").await;
    assert_eq!(verdict, Verdict::Allow);
    let specs = w.approval_specs.lock().unwrap();
    assert_eq!(
        specs
            .iter()
            .map(|(engine, _)| engine.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "beta"]
    );
    for (engine, spec) in specs.iter() {
        assert_eq!(spec.model, format!("{engine}-lead"));
        assert!(spec.max_reported_tokens > 0 && spec.estimated_spend_stop_usd > 0.0);
        assert!(!spec.prompt.contains(KEY_MARKER));
        assert!(!spec.prompt.contains("run-lead"));
        assert_ne!(spec.thread_id, w.lead);
        assert_eq!(
            w.core.store.run(&spec.run_id).unwrap().state,
            orbyn_contracts::RunState::Completed
        );
        assert!(
            w.core
                .store
                .engine_session(&spec.thread_id)
                .unwrap()
                .is_none()
        );
    }
    assert!(w.steered.lock().unwrap().is_empty());
    assert_eq!(events(&w, &id, "allowed")[0].body["decided_by"], "two_keys");
}

#[tokio::test]
async fn a_missing_second_vendor_denies_and_retains_the_notice_signal() {
    let (w, id) = running(&[("alpha", false)]).await;
    assert!(matches!(
        request(&w, &id, ActionKind::Command, "npm install zod").await,
        Verdict::Deny(_)
    ));
    assert_eq!(
        events(&w, &id, "denied")[0].body["needs_second_engine"],
        true
    );
    assert_eq!(w.task(&id).state, "running");
}

#[tokio::test]
async fn invalid_denied_and_tool_using_engine_responses_fail_closed() {
    for turn in [
        ApprovalTurn::Reply("yes".into()),
        ApprovalTurn::Reply(r#"{"approve":true,"reason":""}"#.into()),
        ApprovalTurn::Reply(r#"{"approve":false,"reason":"Outside the brief."}"#.into()),
        ApprovalTurn::Permission,
        ApprovalTurn::Session,
        ApprovalTurn::OverBudget,
        ApprovalTurn::Fail,
    ] {
        let (w, id) = running(&[("alpha", false), ("beta", false)]).await;
        w.approval_turns.lock().unwrap().insert("beta".into(), turn);
        assert!(matches!(
            request(&w, &id, ActionKind::Command, "npm install zod").await,
            Verdict::Deny(_)
        ));
        assert!(events(&w, &id, "allowed").is_empty());
        assert!(w.core.store.active_runs().unwrap().iter().all(|run| {
            !w.approval_specs
                .lock()
                .unwrap()
                .iter()
                .any(|(_, spec)| spec.run_id == run.id)
        }));
    }
}

#[tokio::test]
async fn a_task_ending_during_an_approval_cancels_it_and_cannot_apply_a_late_answer() {
    let (w, id) = running(&[("alpha", false), ("beta", false)]).await;
    w.approval_turns
        .lock()
        .unwrap()
        .insert("alpha".into(), ApprovalTurn::Hang);
    let decide = request(&w, &id, ActionKind::Command, "npm install zod");
    let end = async {
        w.until("approval admitted", |w| {
            !w.approval_specs.lock().unwrap().is_empty()
        })
        .await;
        w.core.crew_budget_ended(&id);
    };
    let (verdict, ()) = tokio::join!(decide, end);
    assert!(matches!(verdict, Verdict::Deny(_)));
    assert!(events(&w, &id, "allowed").is_empty());
    let denied = events(&w, &id, "denied");
    assert_eq!(denied.len(), 1, "a stale approval leaves one denied row");
    assert_eq!(denied[0].body["rule"], "approval_stale");
    assert_eq!(denied[0].body["decided_by"], "hard_rule");
    let spec = w.approval_specs.lock().unwrap()[0].1.clone();
    assert!(w.core.store.run(&spec.run_id).unwrap().state.is_terminal());
    assert_eq!(w.task(&id).stop_reason.as_deref(), Some("budget"));
}

#[tokio::test]
async fn off_and_hard_rules_start_no_approval_run() {
    let (w, id) = running(&[("alpha", false), ("beta", false)]).await;
    assert!(
        words(&request(&w, &id, ActionKind::Command, "git push origin HEAD:main").await)
            .contains("N5")
    );
    assert!(w.approval_specs.lock().unwrap().is_empty());
    w.core
        .set_crew_autopilot_mode(&ModeScope::Crew(w.task(&id).crew_id), Mode::Off)
        .unwrap();
    // The review boundary returns Person; do not wait on a person's gate prompt.
    let scope = super::autopilot_support::scope(&w, &id);
    assert!(matches!(
        w.core
            .crew_review_action(
                &scope,
                &super::autopilot_support::ask(ActionKind::Command, "cargo fmt", &[])
            )
            .await,
        crate::governance::approvals::Review::Person
    ));
    assert!(w.approval_specs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn timeout_and_requester_drop_close_the_admitted_approval_run() {
    for drop_request in [false, true] {
        let (w, id) = running(&[("alpha", false)]).await;
        w.approval_turns
            .lock()
            .unwrap()
            .insert("alpha".into(), ApprovalTurn::Hang);
        let parent = w.core.store.thread(&w.lead).unwrap();
        let mut spec = short_spec();
        if drop_request {
            spec.deadline += std::time::Duration::from_secs(5);
        }
        let mut pending = Box::pin(w.core.delegated_approval(
            &parent,
            "alpha",
            spec,
            &id,
            w.core.store.crew_run(&id).unwrap().unwrap().attempts,
            || true,
        ));
        if drop_request {
            tokio::select! {
                _ = &mut pending => panic!("approval ended before requester cancellation"),
                () = w.until("approval admitted", |w| !w.approval_specs.lock().unwrap().is_empty()) => {}
            }
            drop(pending);
        } else {
            assert!(pending.await.is_err());
        }
        let spec = w.approval_specs.lock().unwrap()[0].1.clone();
        assert!(w.core.store.run(&spec.run_id).unwrap().state.is_terminal());
    }
}

#[tokio::test]
async fn preview_off_and_unsupported_native_drivers_start_nothing() {
    let w = world_tiered(
        false,
        Arc::new(|_, _| Turn::Hang),
        &[("alpha", false)],
        true,
    )
    .await;
    w.core.set_crew_engine_approvers();
    let parent = w.core.store.thread(&w.lead).unwrap();
    assert!(
        w.core
            .delegated_approval(&parent, "alpha", short_spec(), "no-task", 0, || true)
            .await
            .is_err()
    );
    assert!(w.approval_specs.lock().unwrap().is_empty());
    let driver = w.core.engines.driver("codex").unwrap();
    assert!(!driver.supports_approval());
    let (send, _receive) = tokio::sync::mpsc::channel(1);
    assert!(driver.start_approval(short_spec(), send).await.is_err());
}

#[tokio::test]
async fn approval_turn_reservations_are_idempotent_and_respect_the_existing_task_budget() {
    let (w, id) = running(&[("alpha", false)]).await;
    let attempt = w.core.store.crew_run(&id).unwrap().unwrap().attempts;
    w.core
        .store
        .reserve_crew_approval(&id, attempt, "first", "alpha")
        .unwrap();
    w.core
        .store
        .reserve_crew_approval(&id, attempt, "first", "alpha")
        .unwrap();
    assert_eq!(events(&w, &id, "approval_started").len(), 1);
    assert!(
        w.core
            .store
            .reserve_crew_approval(&id, attempt + 1, "stale", "alpha")
            .is_err()
    );
    for i in 1..19 {
        w.core
            .store
            .reserve_crew_approval(&id, attempt, &format!("more-{i}"), "alpha")
            .unwrap();
    }
    assert!(
        w.core
            .store
            .reserve_crew_approval(&id, attempt, "one-too-many", "alpha")
            .is_err()
    );
    assert_eq!(events(&w, &id, "approval_started").len(), 19);
}

#[tokio::test]
async fn ordinary_notices_persist_and_dismiss_once_without_granting_an_action() {
    let (w, id) = running(&[("alpha", false)]).await;
    assert!(matches!(
        request(&w, &id, ActionKind::Command, "npm install zod").await,
        Verdict::Deny(_)
    ));
    let first = w
        .core
        .autopilot_second_engine_notices(None, None)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].lead_family.as_deref(), Some("alpha"));
    assert_eq!(
        w.core
            .autopilot_second_engine_notices(None, None)
            .await
            .unwrap(),
        first
    );
    w.core
        .autopilot_dismiss_second_engine_notice(first.items[0].id)
        .unwrap();
    w.core
        .autopilot_dismiss_second_engine_notice(first.items[0].id)
        .unwrap();
    assert!(
        w.core
            .autopilot_second_engine_notices(None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(events(&w, &id, "second_engine_dismissed").len(), 1);
    assert!(events(&w, &id, "allowed").is_empty());
    assert_eq!(w.task(&id).state, "running");
}

#[tokio::test]
async fn notice_cursor_is_stable_across_dismissal_and_unrelated_recent_rows() {
    let (w, id) = running(&[("alpha", false)]).await;
    for target in ["first", "second", "third"] {
        w.core.store.add_crew_autopilot_event(&id,&w.task(&id).crew_id,"denied",&json!({"mode":"full","needs_second_engine":true,"action":{"target":target,"lead_family":"alpha","lead_engine":"alpha"}})).unwrap();
    }
    for _ in 0..205 {
        w.core
            .store
            .add_crew_autopilot_event(
                &id,
                &w.task(&id).crew_id,
                "denied",
                &json!({"mode":"full","needs_second_engine":false}),
            )
            .unwrap();
    }
    let page = w
        .core
        .autopilot_second_engine_notices(None, Some(1))
        .await
        .unwrap();
    assert_eq!(page.items[0].target, "third");
    w.core
        .autopilot_dismiss_second_engine_notice(page.items[0].id)
        .unwrap();
    let next = w
        .core
        .autopilot_second_engine_notices(page.next_before, Some(1))
        .await
        .unwrap();
    assert_eq!(next.items[0].target, "second");
    let last = w
        .core
        .autopilot_second_engine_notices(next.next_before, Some(1))
        .await
        .unwrap();
    assert_eq!(last.items[0].target, "first");
    assert!(last.next_before.is_none());
}

#[tokio::test]
async fn an_available_other_vendor_resolves_a_notice_once_without_retrying() {
    let (w, id) = running(&[("alpha", false), ("beta", false)]).await;
    let row=w.core.store.add_crew_autopilot_event(&id,&w.task(&id).crew_id,"denied",&json!({"mode":"full","needs_second_engine":true,"action":{"target":"install zod","lead_family":"alpha","lead_engine":"alpha"}})).unwrap();
    assert!(
        w.core
            .autopilot_second_engine_notices(None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(
        w.core
            .autopilot_second_engine_notices(None, None)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let resolved = events(&w, &id, "second_engine_resolved");
    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].body["notice"], row);
    assert_eq!(resolved[0].body["engine"], "beta");
    assert_eq!(resolved[0].body["family"], "beta");
    assert!(w.approval_specs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn another_instance_of_the_lead_vendor_is_neither_a_second_key_nor_notice_resolution() {
    let (w, id) = running(&[("alpha", false)]).await;
    w.core
        .set_preferences(
            None,
            &std::collections::BTreeMap::from([(
                "provider_instances".into(),
                json!([{"id":"alpha-two","driver":"alpha","enabled":true}]),
            )]),
        )
        .unwrap();
    w.core.engines.instances_changed().await;
    assert!(matches!(
        request(&w, &id, ActionKind::Command, "npm install zod").await,
        Verdict::Deny(_)
    ));
    assert_eq!(
        w.approval_specs
            .lock()
            .unwrap()
            .iter()
            .map(|(engine, _)| engine.clone())
            .collect::<Vec<_>>(),
        ["alpha"]
    );
    assert_eq!(
        w.core
            .autopilot_second_engine_notices(None, None)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(events(&w, &id, "second_engine_resolved").is_empty());
}

#[tokio::test]
async fn an_ordinary_parent_run_stop_also_stops_the_owned_approval_pump() {
    let (w, id) = running(&[("alpha", false)]).await;
    w.approval_turns
        .lock()
        .unwrap()
        .insert("alpha".into(), ApprovalTurn::Hang);
    let pending = request(&w, &id, ActionKind::Command, "cargo fmt");
    let stop = async {
        w.until("approval active", |w| {
            !w.approval_specs.lock().unwrap().is_empty()
        })
        .await;
        let run = w.approval_specs.lock().unwrap()[0].1.run_id.clone();
        w.core.store.request_stop(&run).unwrap();
    };
    let (answer, ()) = tokio::join!(pending, stop);
    assert!(matches!(answer, Verdict::Deny(_)));
    assert!(events(&w, &id, "allowed").is_empty());
}

#[tokio::test]
async fn effective_model_lookup_cannot_cross_run_project_or_engine_identity() {
    use crate::store::Admission;
    use orbyn_contracts::AgUiEvent;
    let (w, _) = running(&[("alpha", true), ("beta", false)]).await;
    let parent = w.core.store.thread(&w.lead).unwrap();
    let run = w
        .core
        .store
        .admit_run(Admission {
            thread_id: &w.lead,
            prompt: "test",
            actor: "person",
        })
        .unwrap();
    w.core.store.mark_starting(&run.id).unwrap();
    w.core.store.mark_running(&run.id).unwrap();
    let read = |thread: &str, run: &str, project: &str, engine: &str| {
        w.core
            .store
            .crew_lead_effective_model(thread, run, project, engine)
            .unwrap()
    };
    assert_eq!(
        read(&w.lead, &run.id, &parent.project_id, &parent.engine),
        None
    );
    let report = |model| AgUiEvent::Custom {
        name: "engine.model".into(),
        value: json!({"engine":"claude", "model":model}),
    };
    w.core
        .store
        .append_run_event(&run.id, &report("claude-opus-5-5"))
        .unwrap();
    assert_eq!(
        read(&w.lead, &run.id, &parent.project_id, &parent.engine).as_deref(),
        Some("claude-opus-5-5")
    );
    assert_eq!(
        read("other", &run.id, &parent.project_id, &parent.engine),
        None
    );
    assert_eq!(
        read(&w.lead, "other", &parent.project_id, &parent.engine),
        None
    );
    assert_eq!(read(&w.lead, &run.id, "other", &parent.engine), None);
    assert_eq!(read(&w.lead, &run.id, &parent.project_id, "other"), None);
    w.core
        .store
        .append_run_event(&run.id, &report("claude-sonnet-5-5"))
        .unwrap();
    assert_eq!(
        read(&w.lead, &run.id, &parent.project_id, &parent.engine),
        None
    );
}
