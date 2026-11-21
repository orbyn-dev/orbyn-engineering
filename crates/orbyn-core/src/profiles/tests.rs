//! Execution profiles: the rules of a save and of an apply. The machine (engines, modes, models) is built by hand, so no test
//! depends on which engines are installed here.

use std::collections::{BTreeMap, BTreeSet};

use orbyn_contracts::{
    ApplyCrewProfile, ApplyProfile, AuthState, AutopilotMode, EngineCapabilities, EngineStatus,
    EngineTier, InstallerKind, ModelInfo, PermissionMode, Profile, ProfileDraft, ProfileGrant,
    ProfileTier, UpdateProfile, VersionSupport,
};
use serde_json::json;

use super::*;
use crate::app::CoreConfig;
use crate::crew::RouterConfig;
use crate::store::{NewProject, NewThread};
use crate::testing::TempDir;

fn status(id: &str, installed: bool, modes: &[PermissionMode]) -> EngineStatus {
    EngineStatus {
        id: id.into(),
        name: id.into(),
        tier: EngineTier::Native,
        capabilities: EngineCapabilities {
            streaming: true,
            approvals: true,
            resume: true,
            mcp: true,
            sandbox: false,
            managed: true,
            skill_delivery: Vec::new(),
            goal_control: false,
            decline_feedback: false,
            permission_modes: modes.to_vec(),
        },
        installed,
        path: None,
        version: None,
        auth: AuthState::LoggedIn,
        installer: InstallerKind::SelfManaged,
        latest: None,
        support: VersionSupport::Tested,
        compatibility: None,
        update_command: None,
        manual_hint: None,
        sign_in_command: None,
        updating: false,
        problem: None,
        inherited_credentials: Vec::new(),
        endpoint_origin: None,
        instance_of: None,
        accent: None,
        account: None,
    }
}

fn model(id: &str) -> ModelInfo {
    ModelInfo {
        id: id.into(),
        label: id.into(),
        description: None,
        is_default: false,
        custom: false,
        options: Vec::new(),
        input_modalities: Vec::new(),
        upgrade: None,
    }
}

/// Claude with all four modes and its three tiers, Codex with its three, and a Pi that offers no Auto.
fn machine() -> Machine {
    let all = PermissionMode::ALL;
    let models = BTreeMap::from([
        (
            "claude".to_owned(),
            vec![model("opus"), model("sonnet"), model("haiku")],
        ),
        (
            "codex".to_owned(),
            vec![model("sol-1"), model("terra-1"), model("luna-1")],
        ),
        ("pi".to_owned(), vec![model("pi-default")]),
    ]);
    Machine::new(
        vec![
            status("claude", true, &all),
            status("codex", true, &all),
            status("gone", false, &all),
            status("pi", true, &permission_modes_pi()),
        ],
        models,
        BTreeSet::new(),
        RouterConfig::default(),
    )
}

fn permission_modes_pi() -> Vec<PermissionMode> {
    crate::agents::permission_modes::offered("pi")
}

struct Fixture {
    _dir: TempDir,
    core: Core,
    thread: String,
}

fn fixture(crew_preview: bool) -> Fixture {
    let dir = TempDir::new();
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.crew_preview = crew_preview;
    let core = Core::open(config).unwrap();
    let project = core
        .store
        .add_project(NewProject {
            path: &dir.path().to_string_lossy(),
            name: "p",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id,
            title: "t".into(),
            engine: "claude".into(),
            model: None,
            cwd: dir.path().to_string_lossy().into_owned(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    Fixture {
        _dir: dir,
        core,
        thread: thread.id,
    }
}

fn draft(name: &str, mode: PermissionMode, autopilot: AutopilotMode) -> ProfileDraft {
    ProfileDraft {
        name: name.into(),
        description: String::new(),
        engine: None,
        model_tier: ProfileTier::Builder,
        permission_mode: mode,
        autopilot_mode: autopilot,
    }
}

fn audit_rows(core: &Core, action: &str) -> Vec<orbyn_contracts::AuditEvent> {
    core.store
        .list_audit(None, 500)
        .unwrap()
        .into_iter()
        .filter(|row| row.action == action)
        .collect()
}

fn code(error: &CoreError) -> (u16, &'static str) {
    (error.status(), error.code())
}

async fn saved(f: &Fixture, name: &str, mode: PermissionMode, autopilot: AutopilotMode) -> Profile {
    f.core
        .create_profile(&draft(name, mode, autopilot))
        .await
        .unwrap()
}

fn apply(
    f: &Fixture,
    profile: &Profile,
    machine: &Machine,
) -> crate::error::Result<orbyn_contracts::ApplyProfileAnswer> {
    let row = f.core.profile_row(&profile.id).unwrap();
    let thread = f.core.store.thread(&f.thread).unwrap();
    let grants = grants_of(&row.settings);
    f.core.apply_thread_with(&thread, &row, grants, machine)
}

fn mode_of(core: &Core, thread: &str) -> String {
    let mode = core
        .store
        .thread(thread)
        .unwrap()
        .lifecycle
        .unwrap()
        .permission_mode;
    serde_json::to_value(mode)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn the_built_ins_are_careful_fast_and_cheap_and_grant_nothing() {
    let all = builtins();
    let names: Vec<_> = all.iter().map(|row| row.settings.name.as_str()).collect();
    assert_eq!(names, ["Careful", "Fast", "Cheap"], "no Hands-off");
    let by = |name: &str| all.iter().find(|row| row.settings.name == name).unwrap();
    assert_eq!(
        by("Careful").settings.permission_mode,
        PermissionMode::Supervised
    );
    assert_eq!(by("Careful").settings.autopilot_mode, AutopilotMode::Off);
    assert_eq!(
        by("Fast").settings.permission_mode,
        PermissionMode::AutoEdits
    );
    assert_eq!(by("Fast").settings.autopilot_mode, AutopilotMode::On);
    assert_eq!(by("Cheap").settings.model_tier, ProfileTier::Scout);
    for row in &all {
        assert_eq!(row.settings.engine, None);
        assert_ne!(row.settings.model_tier, ProfileTier::Lead);
        assert!(
            grants_of(&row.settings).is_empty(),
            "{} grants something",
            row.settings.name
        );
        assert!(row.id.starts_with("builtin:"));
    }
}

#[test]
fn a_read_only_mode_does_not_exist_and_is_refused_by_the_shape() {
    let body = json!({ "name": "R", "model_tier": "builder", "permission_mode": "read_only", "autopilot_mode": "off" });
    assert!(serde_json::from_value::<ProfileDraft>(body).is_err());
}

#[test]
fn an_engine_of_null_clears_it_and_an_absent_one_keeps_it() {
    let clear: UpdateProfile =
        serde_json::from_value(json!({ "seen_revision": 1, "engine": null })).unwrap();
    assert_eq!(clear.engine, Some(None));
    let keep: UpdateProfile = serde_json::from_value(json!({ "seen_revision": 1 })).unwrap();
    assert_eq!(keep.engine, None);
    let set: UpdateProfile =
        serde_json::from_value(json!({ "seen_revision": 1, "engine": "codex" })).unwrap();
    assert_eq!(set.engine, Some(Some("codex".into())));
}

#[tokio::test]
async fn the_list_is_the_built_ins_then_the_saved_ones() {
    let f = fixture(false);
    saved(&f, "Mine", PermissionMode::Auto, AutopilotMode::Off).await;
    let list = f.core.list_profiles().await.unwrap();
    let names: Vec<_> = list.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Careful", "Fast", "Cheap", "Mine"]);
    assert!(list[..3].iter().all(|p| p.builtin) && !list[3].builtin);
    assert!(
        list.iter()
            .all(|p| p.availability.engine == orbyn_contracts::ProfileEngineState::NoEngine)
    );
}

#[tokio::test]
async fn names_are_trimmed_unique_ignoring_case_and_never_a_built_in_s() {
    let f = fixture(false);
    saved(
        &f,
        "  Night shift ",
        PermissionMode::Auto,
        AutopilotMode::Off,
    )
    .await;
    for taken in ["night SHIFT", "careful", "FAST"] {
        let error = f
            .core
            .create_profile(&draft(taken, PermissionMode::Auto, AutopilotMode::Off))
            .await
            .unwrap_err();
        assert_eq!(code(&error), (409, "name_taken"), "{taken}");
    }
    for bad in ["", "   ", "x\u{0007}y", "a\u{202e}b", &"n".repeat(61)] {
        let error = f
            .core
            .create_profile(&draft(bad, PermissionMode::Auto, AutopilotMode::Off))
            .await
            .unwrap_err();
        assert_eq!(code(&error), (400, "invalid_input"));
        assert!(
            error.to_string().starts_with("name:"),
            "names the field, not the value: {error}"
        );
    }
    let mut long = draft("ok", PermissionMode::Auto, AutopilotMode::Off);
    long.description = "d".repeat(201);
    let error = f.core.create_profile(&long).await.unwrap_err();
    assert!(error.to_string().starts_with("description:"));
}

#[tokio::test]
async fn at_most_fifty_profiles_are_saved() {
    let f = fixture(false);
    for n in 0..50 {
        saved(
            &f,
            &format!("p{n}"),
            PermissionMode::Supervised,
            AutopilotMode::Off,
        )
        .await;
    }
    let error = f
        .core
        .create_profile(&draft(
            "one too many",
            PermissionMode::Supervised,
            AutopilotMode::Off,
        ))
        .await
        .unwrap_err();
    assert_eq!(code(&error), (409, "profile_limit"));
    assert_eq!(f.core.store.execution_profiles().unwrap().len(), 50);
    // Deleting one frees a place.
    let first = f.core.store.execution_profiles().unwrap().remove(0);
    f.core.delete_profile(&first.id).unwrap();
    saved(
        &f,
        "fits again",
        PermissionMode::Supervised,
        AutopilotMode::Off,
    )
    .await;
}

#[tokio::test]
async fn a_mode_the_named_engine_does_not_offer_is_refused_at_save_and_a_missing_engine_only_warns()
{
    let f = fixture(false);
    let machine = machine();
    // Pi offers no Auto.
    let settings = f.core.profile_settings(
        Settings {
            name: "pi auto",
            description: "",
            engine: Some("pi"),
            model_tier: ProfileTier::Builder,
            permission_mode: PermissionMode::Auto,
            autopilot_mode: AutopilotMode::Off,
        },
        &machine,
    );
    assert_eq!(code(&settings.unwrap_err()), (400, "invalid_input"));
    // An engine this machine lacks still saves, with a warning.
    let settings = f
        .core
        .profile_settings(
            Settings {
                name: "gone",
                description: "",
                engine: Some("gone"),
                model_tier: ProfileTier::Builder,
                permission_mode: PermissionMode::Supervised,
                autopilot_mode: AutopilotMode::Off,
            },
            &machine,
        )
        .unwrap();
    let warnings = Core::profile_warnings(&settings, &machine);
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        (warnings[0].field.as_str(), warnings[0].code.as_str()),
        ("engine", "engine_unavailable")
    );
    // A tier the engine has no model for warns too.
    let mut pi = settings.clone();
    pi.engine = Some("pi".into());
    pi.model_tier = ProfileTier::Lead;
    let warnings = Core::profile_warnings(&pi, &machine);
    assert_eq!(warnings[0].code, "tier_unmapped");
}

#[tokio::test]
async fn built_ins_cannot_be_edited_or_deleted_and_a_stale_revision_changes_nothing() {
    let f = fixture(false);
    for id in ["builtin:careful", "builtin:fast", "builtin:cheap"] {
        let patch: UpdateProfile =
            serde_json::from_value(json!({ "seen_revision": 1, "name": "Renamed" })).unwrap();
        assert_eq!(
            code(&f.core.update_profile(id, &patch).await.unwrap_err()),
            (403, "builtin_profile")
        );
        assert_eq!(
            code(&f.core.delete_profile(id).unwrap_err()),
            (403, "builtin_profile")
        );
    }
    assert_eq!(
        code(&f.core.delete_profile("builtin:nope").unwrap_err()),
        (404, "not_found")
    );
    let mine = saved(&f, "Mine", PermissionMode::Supervised, AutopilotMode::Off).await;
    let patch: UpdateProfile =
        serde_json::from_value(json!({ "seen_revision": 1, "permission_mode": "auto" })).unwrap();
    let changed = f.core.update_profile(&mine.id, &patch).await.unwrap();
    assert_eq!(
        (changed.revision, changed.permission_mode),
        (2, PermissionMode::Auto)
    );
    let stale = f.core.update_profile(&mine.id, &patch).await.unwrap_err();
    assert_eq!(code(&stale), (409, "revision_conflict"));
    assert_eq!(
        f.core.store.execution_profile(&mine.id).unwrap().revision,
        2
    );
    assert_eq!(audit_rows(&f.core, "profile.created").len(), 1);
    assert_eq!(audit_rows(&f.core, "profile.changed").len(), 1);
    f.core.delete_profile(&mine.id).unwrap();
    assert_eq!(audit_rows(&f.core, "profile.deleted").len(), 1);
}

#[tokio::test]
async fn a_grant_raising_profile_without_the_acknowledgement_applies_nothing() {
    let f = fixture(false);
    let full = saved(&f, "Full", PermissionMode::FullAccess, AutopilotMode::Full).await;
    let before = mode_of(&f.core, &f.thread);
    let attempts = [
        vec![],
        vec![ProfileGrant::FullAccess],
        vec![ProfileGrant::AutopilotFull],
    ];
    for ack in attempts {
        let body = ApplyProfile {
            profile_id: Some(full.id.clone()),
            seen_revision: None,
            ack_grants: ack.clone(),
        };
        let error = f
            .core
            .apply_profile_to_thread(&f.thread, &body)
            .await
            .unwrap_err();
        assert_eq!(code(&error), (409, "confirm_required"));
        let CoreError::Refused { detail, .. } = &error else {
            panic!("{error:?}")
        };
        let missing: Vec<&str> = ["full_access", "autopilot_full"]
            .into_iter()
            .filter(|code| {
                !ack.iter()
                    .any(|grant| serde_json::to_value(grant).unwrap() == *code)
            })
            .collect();
        assert_eq!(
            detail,
            &json!({ "grants": missing }),
            "only what was not covered"
        );
    }
    assert_eq!(mode_of(&f.core, &f.thread), before);
    assert!(
        f.core
            .store
            .profile_application("thread", &f.thread)
            .unwrap()
            .is_none()
    );
    assert!(
        audit_rows(&f.core, "profile.applied").is_empty(),
        "a refused apply leaves no applied row"
    );
}

#[tokio::test]
async fn only_autopilot_full_needs_a_confirm_when_the_mode_is_not_full_access() {
    let f = fixture(false);
    let only_autopilot = saved(&f, "Merge", PermissionMode::Supervised, AutopilotMode::Full).await;
    let body = ApplyProfile {
        profile_id: Some(only_autopilot.id),
        seen_revision: None,
        ack_grants: vec![],
    };
    let error = f
        .core
        .apply_profile_to_thread(&f.thread, &body)
        .await
        .unwrap_err();
    let CoreError::Refused { detail, .. } = &error else {
        panic!("{error:?}")
    };
    assert_eq!(detail, &json!({ "grants": ["autopilot_full"] }));
}

#[tokio::test]
async fn with_the_acknowledgement_it_applies_and_the_audit_row_names_the_grants() {
    let f = fixture(false);
    let full = saved(&f, "Full", PermissionMode::FullAccess, AutopilotMode::Full).await;
    let answer = apply(&f, &full, &machine()).unwrap();
    assert_eq!(mode_of(&f.core, &f.thread), "full_access");
    assert_eq!(
        answer.applied.permission_mode,
        Some(PermissionMode::FullAccess)
    );
    assert_eq!(answer.applied.model.as_deref(), Some("sonnet"));
    assert_eq!(
        answer.ignored,
        ["autopilot_mode"],
        "a plain thread has no Autopilot"
    );
    let application = answer.application.unwrap();
    assert_eq!(
        (
            application.name.as_str(),
            application.revision,
            application.customized
        ),
        ("Full", 1, false)
    );
    let rows = audit_rows(&f.core, "profile.applied");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].detail["grants"],
        json!(["full_access", "autopilot_full"])
    );
    assert_eq!(rows[0].thread_id.as_deref(), Some(f.thread.as_str()));
    // The row holds ids, names and codes: no model id, command or value.
    assert!(rows[0].detail.get("model").is_none());
}

#[tokio::test]
async fn careful_fast_and_cheap_apply_with_no_acknowledgement_and_an_empty_grant_list() {
    let f = fixture(false);
    let machine = machine();
    let expected = [
        ("builtin:careful", "supervised", "sonnet"),
        ("builtin:fast", "auto_edits", "sonnet"),
        ("builtin:cheap", "supervised", "haiku"),
    ];
    for (id, mode, model) in expected {
        let body = ApplyProfile {
            profile_id: Some(id.into()),
            seen_revision: Some(1),
            ack_grants: vec![],
        };
        // The public path reaches the machine only after the grant check, which a built-in passes.
        let row = f.core.profile_row(id).unwrap();
        let thread = f.core.store.thread(&f.thread).unwrap();
        let grants = grants_of(&row.settings);
        assert!(
            grants.is_empty() && body.ack_grants.is_empty(),
            "{id} needs no acknowledgement"
        );
        let answer = f
            .core
            .apply_thread_with(&thread, &row, grants, &machine)
            .unwrap();
        assert_eq!(mode_of(&f.core, &f.thread), mode);
        assert_eq!(answer.applied.model.as_deref(), Some(model));
        assert_eq!(
            f.core.store.thread(&f.thread).unwrap().model.as_deref(),
            Some(model)
        );
    }
    for row in audit_rows(&f.core, "profile.applied") {
        assert_eq!(row.detail["grants"], json!([]));
    }
    assert_eq!(audit_rows(&f.core, "profile.applied").len(), 3);
}

#[tokio::test]
async fn an_apply_is_a_copy_so_a_later_edit_or_delete_changes_no_thread() {
    let f = fixture(false);
    let mine = saved(&f, "Mine", PermissionMode::Supervised, AutopilotMode::Off).await;
    apply(&f, &mine, &machine()).unwrap();
    assert_eq!(mode_of(&f.core, &f.thread), "supervised");
    let patch: UpdateProfile = serde_json::from_value(
        json!({ "seen_revision": 1, "permission_mode": "full_access", "name": "Renamed" }),
    )
    .unwrap();
    f.core.update_profile(&mine.id, &patch).await.unwrap();
    assert_eq!(
        mode_of(&f.core, &f.thread),
        "supervised",
        "editing never raises a running thread"
    );
    let application = f
        .core
        .store
        .profile_application("thread", &f.thread)
        .unwrap()
        .unwrap();
    assert_eq!(
        (application.name.as_str(), application.revision),
        ("Mine", 1),
        "the name and revision at apply"
    );
    f.core.delete_profile(&mine.id).unwrap();
    assert_eq!(mode_of(&f.core, &f.thread), "supervised");
    assert!(
        f.core
            .store
            .profile_application("thread", &f.thread)
            .unwrap()
            .is_some(),
        "the row outlives the profile"
    );
}

#[tokio::test]
async fn a_stale_seen_revision_applies_nothing() {
    let f = fixture(false);
    let mine = saved(&f, "Mine", PermissionMode::Supervised, AutopilotMode::Off).await;
    let body = ApplyProfile {
        profile_id: Some(mine.id.clone()),
        seen_revision: Some(9),
        ack_grants: vec![],
    };
    let error = f
        .core
        .apply_profile_to_thread(&f.thread, &body)
        .await
        .unwrap_err();
    assert_eq!(code(&error), (409, "revision_conflict"));
    assert!(
        f.core
            .store
            .profile_application("thread", &f.thread)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn the_persons_own_change_afterwards_marks_the_application_edited_and_clearing_keeps_the_values()
 {
    let f = fixture(false);
    let mine = saved(&f, "Mine", PermissionMode::Supervised, AutopilotMode::Off).await;
    apply(&f, &mine, &machine()).unwrap();
    let patch = orbyn_contracts::UpdateThread {
        permission_mode: Some(PermissionMode::Auto),
        ..Default::default()
    };
    f.core.update_thread(&f.thread, &patch, "user").unwrap();
    assert!(
        f.core
            .store
            .profile_application("thread", &f.thread)
            .unwrap()
            .unwrap()
            .customized
    );
    let clear = ApplyProfile {
        profile_id: None,
        seen_revision: None,
        ack_grants: vec![],
    };
    let answer = f
        .core
        .apply_profile_to_thread(&f.thread, &clear)
        .await
        .unwrap();
    assert!(answer.application.is_none());
    assert_eq!(
        mode_of(&f.core, &f.thread),
        "auto",
        "clearing only forgets the chip"
    );
    assert!(
        f.core
            .store
            .profile_application("thread", &f.thread)
            .unwrap()
            .is_none()
    );
    assert_eq!(audit_rows(&f.core, "profile.cleared").len(), 1);
}

#[tokio::test]
async fn deleting_a_thread_deletes_its_application_row() {
    let f = fixture(false);
    let mine = saved(&f, "Mine", PermissionMode::Supervised, AutopilotMode::Off).await;
    apply(&f, &mine, &machine()).unwrap();
    f.core.delete_thread(&f.thread, "user").unwrap();
    assert!(
        f.core
            .store
            .profile_application("thread", &f.thread)
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn an_engine_that_cannot_be_used_a_mode_it_lacks_or_a_tier_it_has_no_model_for_refuses_the_apply()
 {
    let f = fixture(false);
    let machine = machine();
    let gone = saved(&f, "Gone", PermissionMode::Supervised, AutopilotMode::Off).await;
    let make = |engine: &str, mode: PermissionMode, tier: ProfileTier| {
        let mut row = f.core.profile_row(&gone.id).unwrap();
        row.settings.engine = Some(engine.into());
        row.settings.permission_mode = mode;
        row.settings.model_tier = tier;
        row
    };
    let thread = f.core.store.thread(&f.thread).unwrap();
    let run = |row: &ProfileRow| {
        f.core
            .apply_thread_with(&thread, row, Vec::new(), &machine)
            .unwrap_err()
    };
    // Not installed, and an id nothing knows: never another engine.
    assert_eq!(
        code(&run(&make(
            "gone",
            PermissionMode::Supervised,
            ProfileTier::Builder
        ))),
        (409, "engine_unavailable")
    );
    assert_eq!(
        code(&run(&make(
            "nobody",
            PermissionMode::Supervised,
            ProfileTier::Builder
        ))),
        (409, "engine_unavailable")
    );
    // Pi has no Auto, and no lead model.
    assert_eq!(
        code(&run(&make(
            "pi",
            PermissionMode::Auto,
            ProfileTier::Builder
        ))),
        (409, "mode_not_offered")
    );
    let unmapped = run(&make("pi", PermissionMode::Supervised, ProfileTier::Lead));
    assert_eq!(code(&unmapped), (409, "tier_unmapped"));
    assert!(unmapped.to_string().contains("pi"), "names the engine");
    // Nothing changed, nothing was audited.
    assert_eq!(mode_of(&f.core, &f.thread), "supervised");
    assert!(
        f.core
            .store
            .profile_application("thread", &f.thread)
            .unwrap()
            .is_none()
    );
    assert!(audit_rows(&f.core, "profile.applied").is_empty());
    // A disabled engine is refused as well.
    let off = Machine::new(
        machine_statuses(),
        BTreeMap::new(),
        BTreeSet::from(["codex".to_owned()]),
        RouterConfig::default(),
    );
    assert_eq!(
        code(
            &f.core
                .apply_thread_with(
                    &thread,
                    &make("codex", PermissionMode::Supervised, ProfileTier::Builder),
                    Vec::new(),
                    &off
                )
                .unwrap_err()
        ),
        (409, "engine_unavailable")
    );
}

fn machine_statuses() -> Vec<EngineStatus> {
    vec![
        status("claude", true, &PermissionMode::ALL),
        status("codex", true, &PermissionMode::ALL),
    ]
}

#[tokio::test]
async fn a_profile_with_another_engine_switches_the_thread_to_it_and_takes_that_engine_s_model() {
    let f = fixture(false);
    let mut row = f.core.profile_row("builtin:careful").unwrap();
    row.id = "x".into();
    row.settings.engine = Some("codex".into());
    row.settings.model_tier = ProfileTier::Scout;
    let thread = f.core.store.thread(&f.thread).unwrap();
    let answer = f
        .core
        .apply_thread_with(&thread, &row, Vec::new(), &machine())
        .unwrap();
    assert_eq!(answer.applied.engine.as_deref(), Some("codex"));
    let after = f.core.store.thread(&f.thread).unwrap();
    assert_eq!(
        (after.engine.as_str(), after.model.as_deref()),
        ("codex", Some("luna-1"))
    );
}

#[tokio::test]
async fn a_crew_apply_sets_the_autopilot_mode_through_the_one_table_and_audits_it() {
    let f = fixture(true);
    let machine = machine();
    let row = f.core.profile_row("builtin:cheap").unwrap();
    f.core
        .store
        .set_crew_autopilot_mode("crew:c1", "full")
        .unwrap();
    let answer = f
        .core
        .apply_crew_with("c1", &row, Vec::new(), &machine)
        .unwrap();
    assert_eq!(
        f.core
            .store
            .crew_autopilot_mode_of("crew:c1")
            .unwrap()
            .as_deref(),
        Some("off")
    );
    assert!(answer.ignored.is_empty());
    assert_eq!(answer.applied.autopilot_mode, Some(AutopilotMode::Off));
    let rows = audit_rows(&f.core, "profile.applied");
    assert_eq!(
        (
            rows.len(),
            rows[0].detail["scope"].clone(),
            rows[0].detail["grants"].clone()
        ),
        (1, json!("crew"), json!([]))
    );
    assert!(
        f.core
            .store
            .profile_application("crew", "c1")
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn a_crew_apply_needs_preview_a_real_crew_and_the_acknowledgement() {
    let off = fixture(false);
    let body = ApplyCrewProfile {
        crew_id: "c1".into(),
        profile_id: "builtin:careful".into(),
        seen_revision: None,
        ack_grants: vec![],
    };
    assert_eq!(
        code(&off.core.apply_profile_to_crew(&body).await.unwrap_err()),
        (404, "not_found")
    );
    let on = fixture(true);
    // No crew with that id.
    assert_eq!(
        code(&on.core.apply_profile_to_crew(&body).await.unwrap_err()),
        (404, "not_found")
    );
    // A grant-raising profile asks first, whatever else is wrong.
    let full = saved(
        &on,
        "Merge",
        PermissionMode::Supervised,
        AutopilotMode::Full,
    )
    .await;
    let body = ApplyCrewProfile {
        crew_id: "c1".into(),
        profile_id: full.id,
        seen_revision: None,
        ack_grants: vec![],
    };
    let error = on.core.apply_profile_to_crew(&body).await.unwrap_err();
    assert_eq!(code(&error), (409, "confirm_required"));
    assert!(audit_rows(&on.core, "profile.applied").is_empty());
    assert_eq!(
        on.core.store.crew_autopilot_mode_of("crew:c1").unwrap(),
        None
    );
}
