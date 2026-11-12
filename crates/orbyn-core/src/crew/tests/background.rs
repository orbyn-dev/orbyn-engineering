//! Background crews at rest: validation, redaction, the audit trail, the budget stop and the review it queues. No engine and no
//! trigger: the run functions are driven by hand, as slice 4h will drive them.

use std::sync::Arc;

use orbyn_contracts as wire;
use serde_json::{Value, json};

use crate::crew::{BackgroundEnd, Page};
use crate::error::CoreError;
use crate::store::NewProject;
use crate::testing::TempDir;
use crate::{Core, CoreConfig};

struct Fixture {
    core: Arc<Core>,
    project: String,
    _dir: TempDir,
}

fn fixture(preview: bool) -> Fixture {
    let dir = TempDir::new();
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.crew_preview = preview;
    let core = Arc::new(Core::open(config).unwrap());
    let project = core
        .store
        .add_project(NewProject {
            path: "/background-project",
            name: "background",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    Fixture {
        core,
        project: project.id,
        _dir: dir,
    }
}

fn create_body(project: &str, name: &str) -> wire::BackgroundCrewCreate {
    serde_json::from_value(json!({
        "project_id": project,
        "name": name,
        "template": "fix_failing_ci",
        "trigger": { "kind": "ci_failure", "branch": "main" },
        "budget": { "turns": 10, "minutes": 30 }
    }))
    .unwrap()
}

fn page() -> Page {
    Page::new(None, None).unwrap()
}

fn audit_actions(core: &Core) -> Vec<String> {
    let mut rows = core.store.list_audit(None, 200).unwrap();
    rows.reverse();
    rows.into_iter()
        .map(|row| row.action)
        .filter(|action| action.starts_with("crew.background"))
        .collect()
}

#[test]
fn with_the_flag_off_nothing_exists() {
    let f = fixture(false);
    let not_found = |error: CoreError| assert!(matches!(error, CoreError::NotFound(_)), "{error}");
    not_found(f.core.background_list(None, page()).unwrap_err());
    not_found(f.core.background_get("x").unwrap_err());
    not_found(
        f.core
            .background_create(&create_body(&f.project, "A"))
            .unwrap_err(),
    );
    not_found(f.core.background_set_enabled("x", true).unwrap_err());
    not_found(f.core.background_delete("x").unwrap_err());
    not_found(f.core.background_runs("x", page()).unwrap_err());
    not_found(f.core.crew_background_start_run("x").unwrap_err());
    not_found(f.core.crew_background_spend("x", 1, 1).unwrap_err());
    assert!(audit_actions(&f.core).is_empty());
}

#[test]
fn a_crew_is_created_off_and_audited() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "  Fix CI  "))
        .unwrap();
    assert_eq!(crew.name, "Fix CI");
    assert!(!crew.enabled);
    assert_eq!(crew.state, wire::BackgroundState::Idle);
    assert_eq!(crew.template, wire::BackgroundTemplate::FixFailingCi);
    assert_eq!(crew.trigger.branch.as_deref(), Some("main"));
    assert_eq!(
        crew.budget,
        wire::BackgroundBudget {
            turns: 10,
            minutes: 30
        }
    );
    assert_eq!(f.core.background_get(&crew.id).unwrap(), crew);
    let listed = f.core.background_list(Some(&f.project), page()).unwrap();
    assert_eq!((listed.total, listed.crews.len()), (1, 1));
    assert_eq!(audit_actions(&f.core), ["crew.background.create"]);
}

#[test]
fn input_is_checked_strictly() {
    let f = fixture(true);
    let refused = |value: Value| {
        let body: wire::BackgroundCrewCreate = serde_json::from_value(value).unwrap();
        let error = f.core.background_create(&body).unwrap_err();
        assert!(matches!(error, CoreError::Invalid(_)), "{error}");
    };
    let base = |patch: Value| {
        let mut body = json!({
            "project_id": f.project,
            "name": "Crew",
            "template": "flaky_test",
            "trigger": { "kind": "ci_failure" },
            "budget": { "turns": 5, "minutes": 5 }
        });
        for (key, value) in patch.as_object().unwrap() {
            body[key] = value.clone();
        }
        body
    };
    refused(base(json!({ "name": "" })));
    refused(base(json!({ "name": "   " })));
    refused(base(json!({ "name": "x".repeat(81) })));
    refused(base(json!({ "name": "bad\u{7}name" })));
    refused(base(json!({ "budget": { "turns": 0, "minutes": 5 } })));
    refused(base(json!({ "budget": { "turns": 1001, "minutes": 5 } })));
    refused(base(json!({ "budget": { "turns": 5, "minutes": 0 } })));
    refused(base(json!({ "budget": { "turns": 5, "minutes": 1441 } })));
    refused(base(json!({ "project_id": "../etc" })));
    // Trigger settings: required, foreign and out of range.
    refused(base(json!({ "trigger": { "kind": "issue_label" } })));
    refused(base(json!({ "trigger": { "kind": "schedule" } })));
    refused(base(
        json!({ "trigger": { "kind": "schedule", "every_hours": 0 } }),
    ));
    refused(base(
        json!({ "trigger": { "kind": "schedule", "every_hours": 721 } }),
    ));
    refused(base(
        json!({ "trigger": { "kind": "ci_failure", "label": "bug" } }),
    ));
    refused(base(
        json!({ "trigger": { "kind": "ci_failure", "every_hours": 3 } }),
    ));
    refused(base(
        json!({ "trigger": { "kind": "dependency_update", "branch": "main" } }),
    ));
    refused(base(
        json!({ "trigger": { "kind": "issue_label", "label": "y".repeat(101) } }),
    ));
    refused(base(
        json!({ "trigger": { "kind": "issue_label", "label": " " } }),
    ));
    // A project that is nothing.
    let error = f
        .core
        .background_create(&serde_json::from_value(base(json!({ "project_id": "nope" }))).unwrap())
        .unwrap_err();
    assert!(matches!(error, CoreError::NotFound(_)), "{error}");
    // The shapes the wire refuses before the core sees them: an unknown template, trigger or field.
    for bad in [
        base(json!({ "template": "deploy_to_production" })),
        base(json!({ "trigger": { "kind": "webhook" } })),
        base(json!({ "enabled": true })),
        base(json!({ "trigger": { "kind": "ci_failure", "command": "rm -rf /" } })),
        base(json!({ "budget": { "turns": 5, "minutes": 5, "tokens": 9 } })),
        base(json!({ "budget": { "turns": -1, "minutes": 5 } })),
    ] {
        assert!(serde_json::from_value::<wire::BackgroundCrewCreate>(bad).is_err());
    }
    assert!(
        audit_actions(&f.core).is_empty(),
        "a refusal writes nothing"
    );
    assert_eq!(f.core.background_list(None, page()).unwrap().total, 0);
    // The bounds themselves are accepted.
    for (n, (turns, minutes)) in [(1, 1), (1000, 1440)].into_iter().enumerate() {
        let mut body = create_body(&f.project, &format!("edge-{n}"));
        body.budget = wire::BackgroundBudget { turns, minutes };
        f.core.background_create(&body).unwrap();
    }
    let mut body = create_body(&f.project, &"é".repeat(80));
    body.trigger =
        serde_json::from_value(json!({ "kind": "schedule", "every_hours": 720 })).unwrap();
    f.core.background_create(&body).unwrap();
}

#[test]
fn a_name_is_unique_in_its_project_and_a_project_holds_a_few_crews() {
    let f = fixture(true);
    f.core
        .background_create(&create_body(&f.project, "Same"))
        .unwrap();
    let error = f
        .core
        .background_create(&create_body(&f.project, "SAME"))
        .unwrap_err();
    assert!(matches!(error, CoreError::Invalid(_)), "{error}");
    for n in 1..crate::store::BACKGROUND_PER_PROJECT_MAX {
        f.core
            .background_create(&create_body(&f.project, &format!("crew-{n}")))
            .unwrap();
    }
    let error = f
        .core
        .background_create(&create_body(&f.project, "one too many"))
        .unwrap_err();
    assert!(error.to_string().contains("at most"), "{error}");
}

#[test]
fn free_text_is_redacted_before_it_is_stored() {
    let f = fixture(true);
    let secret = format!("ghp_{}", "q".repeat(36));
    let mut body = create_body(&f.project, &format!("Fix {secret}"));
    body.trigger =
        serde_json::from_value(json!({ "kind": "issue_label", "label": format!("tok {secret}") }))
            .unwrap();
    let crew = f.core.background_create(&body).unwrap();
    assert!(!crew.name.contains(&secret) && crew.name.contains("[redacted]"));
    assert!(!crew.trigger.label.as_deref().unwrap().contains(&secret));
    let stored = f.core.store.background_crew(&crew.id).unwrap().unwrap();
    assert!(!format!("{stored:?}").contains(&secret));
    let audit = serde_json::to_string(&f.core.store.list_audit(None, 50).unwrap()).unwrap();
    assert!(!audit.contains(&secret), "the audit row holds none either");
}

#[test]
fn update_changes_the_name_and_trigger_and_never_the_budget() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "One"))
        .unwrap();
    let other = f
        .core
        .background_create(&create_body(&f.project, "Two"))
        .unwrap();
    let update = |value: Value| serde_json::from_value::<wire::BackgroundCrewUpdate>(value);
    // The budget, template and project have no place in an update.
    for bad in [
        json!({ "budget": { "turns": 999, "minutes": 1 } }),
        json!({ "template": "flaky_test" }),
        json!({ "project_id": "other" }),
        json!({ "enabled": true }),
    ] {
        assert!(update(bad).is_err());
    }
    let error = f
        .core
        .background_update(&crew.id, &update(json!({})).unwrap())
        .unwrap_err();
    assert!(matches!(error, CoreError::Invalid(_)));
    let error = f
        .core
        .background_update(&crew.id, &update(json!({ "name": "two" })).unwrap())
        .unwrap_err();
    assert!(matches!(error, CoreError::Invalid(_)), "a name taken");
    let error = f
        .core
        .background_update(
            &crew.id,
            &update(json!({ "trigger": { "kind": "schedule" } })).unwrap(),
        )
        .unwrap_err();
    assert!(matches!(error, CoreError::Invalid(_)));
    let changed = f
        .core
        .background_update(
            &crew.id,
            &update(
                json!({ "name": "Renamed", "trigger": { "kind": "schedule", "every_hours": 6 } }),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(changed.name, "Renamed");
    assert_eq!(changed.trigger.every_hours, Some(6));
    assert_eq!(changed.trigger.branch, None, "the old settings are gone");
    assert_eq!(changed.budget, crew.budget);
    assert_eq!(changed.template, crew.template);
    assert_eq!(f.core.background_get(&other.id).unwrap().name, "Two");
    let unchanged = f.core.background_get(&crew.id).unwrap();
    assert_eq!(unchanged.budget, crew.budget);
}

#[test]
fn a_run_needs_an_enabled_crew_and_one_at_a_time() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "Run"))
        .unwrap();
    let error = f.core.crew_background_start_run(&crew.id).unwrap_err();
    assert!(
        matches!(error, CoreError::PolicyDenied(_)),
        "off means no run"
    );
    f.core.background_set_enabled(&crew.id, true).unwrap();
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    assert_eq!(run.state, wire::BackgroundRunState::Running);
    assert_eq!(run.budget, crew.budget);
    assert_eq!(
        f.core.background_get(&crew.id).unwrap().state,
        wire::BackgroundState::Running
    );
    let error = f.core.crew_background_start_run(&crew.id).unwrap_err();
    assert!(matches!(error, CoreError::RunsActive), "{error}");
    let finished = f
        .core
        .crew_background_end_run(&run.id, BackgroundEnd::NeedsReview)
        .unwrap();
    assert_eq!(finished.state, wire::BackgroundRunState::NeedsReview);
    assert!(finished.review_waiting && finished.ended_at.is_some());
    let crew_now = f.core.background_get(&crew.id).unwrap();
    assert_eq!(crew_now.state, wire::BackgroundState::NeedsReview);
    assert_eq!(crew_now.reviews_waiting, 1);
    // A second run may begin once the first ended; a closed run takes nothing more.
    f.core.crew_background_start_run(&crew.id).unwrap();
    let error = f.core.crew_background_spend(&run.id, 1, 1).unwrap_err();
    assert!(matches!(error, CoreError::RunClosed), "{error}");
    let runs = f.core.background_runs(&crew.id, page()).unwrap();
    assert_eq!(runs.total, 2);
    assert_eq!(
        runs.runs[0].state,
        wire::BackgroundRunState::Running,
        "newest first"
    );
}

#[test]
fn reaching_the_budget_stops_the_run_and_queues_a_review() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "Budget"))
        .unwrap();
    f.core.background_set_enabled(&crew.id, true).unwrap();
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    let after = f.core.crew_background_spend(&run.id, 4, 10).unwrap();
    assert_eq!(
        (after.state, after.turns_spent, after.minutes_spent),
        (wire::BackgroundRunState::Running, 4, 10),
        "spent is under the budget"
    );
    let stopped = f.core.crew_background_spend(&run.id, 6, 0).unwrap();
    assert_eq!(stopped.state, wire::BackgroundRunState::Stopped);
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Budget)
    );
    assert!(stopped.review_waiting && stopped.review_reason.is_some());
    assert_eq!((stopped.turns_spent, stopped.budget.turns), (10, 10));
    let crew_now = f.core.background_get(&crew.id).unwrap();
    assert_eq!(crew_now.state, wire::BackgroundState::Stopped);
    assert_eq!(crew_now.reviews_waiting, 1);
    let actions = audit_actions(&f.core);
    assert_eq!(
        actions,
        [
            "crew.background.create",
            "crew.background.enable",
            "crew.background.run.start",
            "crew.background.run.spend",
            "crew.background.run.spend",
            "crew.background.run.stop"
        ]
    );
    // The minutes budget stops a run the same way, and a stop for the budget reached by `end` still queues a review.
    let second = f.core.crew_background_start_run(&crew.id).unwrap();
    let stopped = f.core.crew_background_spend(&second.id, 1, 30).unwrap();
    assert_eq!(
        stopped.stop_reason,
        Some(wire::BackgroundStopReason::Budget)
    );
    let third = f.core.crew_background_start_run(&crew.id).unwrap();
    let ended = f
        .core
        .crew_background_end_run(
            &third.id,
            BackgroundEnd::Stopped(wire::BackgroundStopReason::Budget),
        )
        .unwrap();
    assert!(ended.review_waiting);
    // Another reason leaves nothing to review.
    let fourth = f.core.crew_background_start_run(&crew.id).unwrap();
    let ended = f
        .core
        .crew_background_end_run(
            &fourth.id,
            BackgroundEnd::Stopped(wire::BackgroundStopReason::EngineError),
        )
        .unwrap();
    assert!(!ended.review_waiting);
    assert_eq!(f.core.background_get(&crew.id).unwrap().reviews_waiting, 3);
    // Spending nothing, or more than a run could, is refused.
    let fifth = f.core.crew_background_start_run(&crew.id).unwrap();
    assert!(f.core.crew_background_spend(&fifth.id, 0, 0).is_err());
    assert!(f.core.crew_background_spend(&fifth.id, 1001, 0).is_err());
    assert!(f.core.crew_background_spend(&fifth.id, 0, 1441).is_err());
}

#[test]
fn a_budget_stop_adds_no_merge_authority() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "NoMerge"))
        .unwrap();
    f.core.background_set_enabled(&crew.id, true).unwrap();
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    f.core.crew_background_spend(&run.id, 10, 0).unwrap();
    let queue = f.core.autopilot_merge_queue(page()).unwrap();
    assert!(queue.entries.is_empty(), "nothing was queued to merge");
    assert!(
        f.core
            .autopilot_review_queue(0, page())
            .unwrap()
            .items
            .is_empty()
    );
}

#[test]
fn turning_a_crew_off_stops_its_run_and_deleting_needs_it_idle() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "Off"))
        .unwrap();
    f.core.background_set_enabled(&crew.id, true).unwrap();
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    let error = f.core.background_delete(&crew.id).unwrap_err();
    assert!(matches!(error, CoreError::RunsActive), "{error}");
    let off = f.core.background_set_enabled(&crew.id, false).unwrap();
    assert!(!off.enabled);
    assert_eq!(
        off.state,
        wire::BackgroundState::Idle,
        "the person's own switch leaves nothing for them"
    );
    let runs = f.core.background_runs(&crew.id, page()).unwrap();
    assert_eq!(runs.runs[0].id, run.id);
    assert_eq!(
        runs.runs[0].stop_reason,
        Some(wire::BackgroundStopReason::Disabled)
    );
    assert!(!runs.runs[0].review_waiting);
    f.core.background_delete(&crew.id).unwrap();
    assert!(matches!(
        f.core.background_get(&crew.id).unwrap_err(),
        CoreError::NotFound(_)
    ));
    assert_eq!(
        f.core
            .background_runs(&crew.id, page())
            .unwrap_err()
            .to_string(),
        "background crew not found"
    );
    // The audit rows stay: append-only, whatever happened to the crew.
    assert_eq!(
        audit_actions(&f.core),
        [
            "crew.background.create",
            "crew.background.enable",
            "crew.background.run.start",
            "crew.background.disable",
            "crew.background.run.stop",
            "crew.background.delete"
        ]
    );
    let again = f.core.background_delete(&crew.id).unwrap_err();
    assert!(matches!(again, CoreError::NotFound(_)));
}

#[test]
fn lists_are_paged_and_a_repository_file_is_never_written() {
    let f = fixture(true);
    for n in 0..5 {
        f.core
            .background_create(&create_body(&f.project, &format!("crew-{n}")))
            .unwrap();
    }
    let first = f
        .core
        .background_list(None, Page::new(Some(2), Some(0)).unwrap())
        .unwrap();
    assert_eq!(
        (first.crews.len(), first.total, first.next_offset),
        (2, 5, Some(2))
    );
    let last = f
        .core
        .background_list(None, Page::new(Some(2), Some(4)).unwrap())
        .unwrap();
    assert_eq!((last.crews.len(), last.next_offset), (1, None));
    assert_eq!(
        f.core.background_list(Some("other"), page()).unwrap().total,
        0
    );
    // Rule 4: the project's folder is untouched, and nothing named crew.json exists in the data folder.
    assert!(!std::path::Path::new("/background-project/.orbyn").exists());
}

fn audit_rows(core: &Core, action: &str) -> Vec<orbyn_contracts::AuditEvent> {
    let mut rows = core.store.list_audit(None, 200).unwrap();
    rows.reverse();
    rows.into_iter()
        .filter(|row| row.action == action)
        .collect()
}

#[test]
fn hidden_characters_are_refused_in_names_and_trigger_text() {
    let f = fixture(true);
    let hidden = [
        '\u{200b}',
        '\u{200d}',
        '\u{202e}',
        '\u{2066}',
        '\u{2060}',
        '\u{feff}',
        '\u{e0041}',
        '\u{00ad}',
        '\u{3164}',
    ];
    for c in hidden {
        let name = format!("fix{c}ci");
        let error = f
            .core
            .background_create(&create_body(&f.project, &name))
            .unwrap_err();
        assert!(matches!(error, CoreError::Invalid(_)), "{c:?}: {error}");
        let body: wire::BackgroundCrewCreate = serde_json::from_value(json!({
            "project_id": f.project,
            "name": "Trigger",
            "template": "fix_failing_ci",
            "trigger": { "kind": "issue_label", "label": format!("bug{c}") },
            "budget": { "turns": 5, "minutes": 5 }
        }))
        .unwrap();
        assert!(
            matches!(f.core.background_create(&body), Err(CoreError::Invalid(_))),
            "{c:?} in a trigger setting"
        );
    }
    // An update is checked the same way, and an emoji with its variation selector is fine.
    let crew = f
        .core
        .background_create(&create_body(&f.project, "Plain"))
        .unwrap();
    let update: wire::BackgroundCrewUpdate =
        serde_json::from_value(json!({ "name": "re\u{200b}named" })).unwrap();
    assert!(f.core.background_update(&crew.id, &update).is_err());
    f.core
        .background_create(&create_body(&f.project, "Heart \u{2764}\u{fe0f}"))
        .unwrap();
    assert_eq!(
        audit_actions(&f.core).len(),
        2,
        "only the two accepted writes"
    );
}

#[test]
fn look_alike_names_are_not_two_names() {
    let f = fixture(true);
    f.core
        .background_create(&create_body(&f.project, "fix-ci"))
        .unwrap();
    // The zero-width look-alike is refused outright.
    assert!(
        f.core
            .background_create(&create_body(&f.project, "fix\u{200b}-ci"))
            .is_err()
    );
    // Case is folded beyond ASCII.
    f.core
        .background_create(&create_body(&f.project, "Écran"))
        .unwrap();
    let error = f
        .core
        .background_create(&create_body(&f.project, "éCRAN"))
        .unwrap_err();
    assert!(error.to_string().contains("already"), "{error}");
    // A rename onto an existing name is refused, a rename to its own name is not.
    let other = f
        .core
        .background_create(&create_body(&f.project, "other"))
        .unwrap();
    let onto = |name: &str| -> wire::BackgroundCrewUpdate {
        serde_json::from_value(json!({ "name": name })).unwrap()
    };
    assert!(
        f.core
            .background_update(&other.id, &onto("FIX-CI"))
            .is_err()
    );
    f.core.background_update(&other.id, &onto("OTHER")).unwrap();
}

#[test]
fn a_spend_is_audited_and_a_person_s_disable_is_logged_as_the_person() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "Audit"))
        .unwrap();
    f.core.background_set_enabled(&crew.id, true).unwrap();
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    f.core.crew_background_spend(&run.id, 3, 7).unwrap();
    let spends = audit_rows(&f.core, "crew.background.run.spend");
    assert_eq!(spends.len(), 1);
    assert_eq!(
        (spends[0].actor.as_str(), spends[0].source.as_str()),
        ("crew", "core")
    );
    assert_eq!(spends[0].target, crew.id);
    assert_eq!(spends[0].detail["run"], run.id);
    assert_eq!(
        (
            spends[0].detail["turns"].as_u64(),
            spends[0].detail["minutes_spent"].as_u64()
        ),
        (Some(3), Some(7))
    );
    f.core.background_set_enabled(&crew.id, false).unwrap();
    let disable = audit_rows(&f.core, "crew.background.disable");
    let stop = audit_rows(&f.core, "crew.background.run.stop");
    assert_eq!(disable[0].actor, "user");
    assert_eq!(
        (stop[0].actor.as_str(), stop[0].source.as_str()),
        ("user", "user"),
        "the stop the person caused is the person's"
    );
    assert_eq!(stop[0].detail["reason"], "disabled");
    // A stop the crew caused stays the crew's.
    f.core.background_set_enabled(&crew.id, true).unwrap();
    let second = f.core.crew_background_start_run(&crew.id).unwrap();
    f.core.crew_background_spend(&second.id, 10, 0).unwrap();
    let stop = audit_rows(&f.core, "crew.background.run.stop");
    assert_eq!(stop[1].actor, "crew");
}

#[test]
fn a_delete_records_what_it_removes_and_is_refused_with_a_review_waiting() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "Gone"))
        .unwrap();
    f.core.background_set_enabled(&crew.id, true).unwrap();
    let first = f.core.crew_background_start_run(&crew.id).unwrap();
    f.core.crew_background_spend(&first.id, 10, 0).unwrap();
    let error = f.core.background_delete(&crew.id).unwrap_err();
    assert!(error.to_string().contains("review"), "{error}");
    assert!(
        f.core.background_get(&crew.id).is_ok(),
        "nothing was removed"
    );
    assert!(audit_rows(&f.core, "crew.background.delete").is_empty());
    // Runs with nothing waiting go with the crew, counted in the audit row.
    let fresh = f
        .core
        .background_create(&create_body(&f.project, "Counted"))
        .unwrap();
    f.core.background_set_enabled(&fresh.id, true).unwrap();
    for _ in 0..2 {
        let run = f.core.crew_background_start_run(&fresh.id).unwrap();
        f.core
            .crew_background_end_run(
                &run.id,
                BackgroundEnd::Stopped(wire::BackgroundStopReason::EngineError),
            )
            .unwrap();
    }
    f.core.background_delete(&fresh.id).unwrap();
    let deletes = audit_rows(&f.core, "crew.background.delete");
    assert_eq!(deletes.len(), 1);
    assert_eq!(deletes[0].detail["runs_removed"], 2);
    assert_eq!(deletes[0].detail["reviews_waiting_removed"], 0);
}

#[test]
fn idle_means_nothing_running_nothing_waiting_and_nothing_left_for_the_person() {
    let f = fixture(true);
    let crew = f
        .core
        .background_create(&create_body(&f.project, "State"))
        .unwrap();
    let state = || f.core.background_get(&crew.id).unwrap().state;
    assert_eq!(state(), wire::BackgroundState::Idle, "never ran");
    f.core.background_set_enabled(&crew.id, true).unwrap();
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    assert_eq!(state(), wire::BackgroundState::Running);
    f.core
        .crew_background_end_run(&run.id, BackgroundEnd::NeedsReview)
        .unwrap();
    assert_eq!(state(), wire::BackgroundState::NeedsReview);
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    f.core
        .crew_background_end_run(
            &run.id,
            BackgroundEnd::Stopped(wire::BackgroundStopReason::EngineError),
        )
        .unwrap();
    assert_eq!(
        state(),
        wire::BackgroundState::Stopped,
        "a failure needs the person"
    );
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    f.core.crew_background_spend(&run.id, 10, 0).unwrap();
    assert_eq!(
        state(),
        wire::BackgroundState::Stopped,
        "so does the budget"
    );
    // Turned off while a run was going: the person's own switch leaves nothing, but the older reviews still wait.
    let run = f.core.crew_background_start_run(&crew.id).unwrap();
    f.core.background_set_enabled(&crew.id, false).unwrap();
    let _ = run;
    let now = f.core.background_get(&crew.id).unwrap();
    assert_eq!(
        (now.state, now.reviews_waiting),
        (wire::BackgroundState::NeedsReview, 2)
    );
}
