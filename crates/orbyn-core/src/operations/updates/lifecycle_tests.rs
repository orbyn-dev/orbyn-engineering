//! Fake-shell boundary checks: no providers, network or installer.
use super::*;
use crate::testing::TempDir;
use crate::{Core, CoreConfig};
fn fixture() -> (TempDir, Core, mpsc::UnboundedReceiver<Value>) {
    let dir = TempDir::new();
    let mut config = CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    let core = Core::open(config).unwrap();
    let lines = core.updates.attach_shell();
    core.updates
        .on_shell_line(r#"{"type":"app_update_available","release_id":"r1","version":"99.0.0"}"#);
    (dir, core, lines)
}
fn request(id: &str, operation: u32) -> AppUpdateRequest {
    AppUpdateRequest {
        release_id: id.into(),
        operation,
    }
}
fn say(updates: &UpdateCenter, kind: &str, extra: Value) {
    let mut line = json!({"type":kind,"release_id":"r1","operation":1});
    line.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    updates.on_shell_line(&line.to_string());
}
fn phase(updates: &UpdateCenter) -> AppUpdatePhase {
    lock(&updates.known).phase
}

/// The refusal an update call answers: the exact variant and the sentence the update screen shows.
fn unavailable(error: CoreError) -> String {
    match error {
        CoreError::UpdateUnavailable(message) => message,
        other => panic!("expected UpdateUnavailable, got {other:?}"),
    }
}

/// The refusal of a run whose desktop shell went away mid-operation.
fn update_failed(error: CoreError) -> String {
    match error {
        CoreError::UpdateFailed(message) => message,
        other => panic!("expected UpdateFailed, got {other:?}"),
    }
}

/// The sentences the refusals show; changing one is a product change and must fail these tests.
const BUSY: &str = "An app update is already running.";
const CHANGED: &str = "The available update changed. Review the new version before downloading.";
const SHELL_GONE: &str = "The desktop shell is gone.";

#[tokio::test]
async fn complete_consent_waits_for_readiness_and_duplicate_requests_keep_one_operation() {
    let (_dir, core, mut lines) = fixture();
    let updates = &core.updates;
    updates
        .start_app("person", request("r1", 0), AppIntent::Complete)
        .unwrap();
    let download = lines.try_recv().unwrap();
    assert_eq!(
        download,
        json!({"type":"download_app_update","release_id":"r1","operation":1})
    );
    updates
        .start_app("person", request("r1", 0), AppIntent::Complete)
        .unwrap();
    assert!(lines.try_recv().is_err());
    say(
        updates,
        "app_update_progress",
        json!({"downloaded":100,"total":100}),
    );
    assert_eq!(phase(updates), AppUpdatePhase::Downloading);
    assert!(
        lines.try_recv().is_err(),
        "complete bytes cannot authorize installation"
    );
    say(updates, "app_update_preparing", json!({}));
    assert_eq!(phase(updates), AppUpdatePhase::Preparing);
    updates
        .start_app("person", request("r1", 1), AppIntent::Complete)
        .unwrap();
    assert!(lines.try_recv().is_err());
    say(updates, "app_update_ready", json!({}));
    assert_eq!(phase(updates), AppUpdatePhase::Installing);
    let install = lines.try_recv().unwrap();
    assert_eq!(
        install,
        json!({"type":"install_app_update","release_id":"r1","operation":1})
    );
    for operation in [0, 1] {
        updates
            .start_app("person", request("r1", operation), AppIntent::Complete)
            .unwrap();
    }
    assert!(
        lines.try_recv().is_err(),
        "duplicate Complete never starts another effect"
    );
}

#[tokio::test]
async fn complete_cannot_upgrade_a_plain_download_that_won_admission() {
    let (_dir, core, mut lines) = fixture();
    let updates = &core.updates;
    updates
        .download_app("person", request("r1", 0))
        .await
        .unwrap();
    assert_eq!(lines.try_recv().unwrap()["type"], "download_app_update");
    for preparing in [false, true] {
        if preparing {
            say(updates, "app_update_preparing", json!({}));
        }
        for operation in [0, 1] {
            assert_eq!(
                unavailable(
                    updates
                        .start_app("person", request("r1", operation), AppIntent::Complete)
                        .unwrap_err()
                ),
                BUSY,
                "preparing: {preparing}"
            );
        }
        assert!(!lock(&updates.known).install_after_download);
        assert!(lines.try_recv().is_err());
    }
    say(updates, "app_update_ready", json!({}));
    assert_eq!(phase(updates), AppUpdatePhase::Ready);
    assert!(
        lines.try_recv().is_err(),
        "plain download must remain install-free"
    );
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
}

#[tokio::test]
async fn complete_refuses_changed_release_and_operation_without_any_effect() {
    let (_dir, core, mut lines) = fixture();
    let updates = &core.updates;
    updates
        .on_shell_line(r#"{"type":"app_update_available","release_id":"r2","version":"100.0.0"}"#);
    assert_eq!(
        unavailable(
            updates
                .start_app("person", request("r1", 0), AppIntent::Complete)
                .unwrap_err()
        ),
        CHANGED
    );
    assert_eq!(
        unavailable(
            updates
                .start_app("person", request("r2", u32::MAX), AppIntent::Complete)
                .unwrap_err()
        ),
        CHANGED
    );
    assert!(lines.try_recv().is_err());
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
    assert_eq!(phase(updates), AppUpdatePhase::Available);
}
#[tokio::test]
async fn consent_downloads_once_and_verified_readiness_is_independent_of_bytes() {
    let (_dir, core, mut lines) = fixture();
    let updates = &core.updates;
    assert!(lines.try_recv().is_err());
    updates
        .download_app("person", request("r1", 0))
        .await
        .unwrap();
    assert_eq!(lines.try_recv().unwrap()["type"], "download_app_update");
    updates
        .download_app("person", request("r1", 0))
        .await
        .unwrap();
    assert!(lines.try_recv().is_err());
    say(
        updates,
        "app_update_progress",
        json!({"downloaded":100,"total":100}),
    );
    assert_eq!(phase(updates), AppUpdatePhase::Downloading);
    assert_eq!(
        unavailable(
            updates
                .install_app("person", request("r1", 1))
                .await
                .unwrap_err()
        ),
        BUSY
    );
    say(updates, "app_update_preparing", json!({}));
    assert_eq!(phase(updates), AppUpdatePhase::Preparing);
    say(
        updates,
        "app_update_failed",
        json!({"message":"signature rejected"}),
    );
    assert_eq!(phase(updates), AppUpdatePhase::Available);
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
    updates
        .download_app("person", request("r1", 1))
        .await
        .unwrap();
    updates.on_shell_line(r#"{"type":"app_update_ready","release_id":"r1","operation":2}"#);
    assert_eq!(phase(updates), AppUpdatePhase::Ready);
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
    assert_eq!(lines.try_recv().unwrap()["type"], "download_app_update");
    assert!(
        lines.try_recv().is_err(),
        "explicit download never installs"
    );
}
#[tokio::test]
async fn new_checks_and_old_callbacks_cannot_retarget_staged_consent() {
    let (_dir, core, mut lines) = fixture();
    let updates = &core.updates;
    updates
        .download_app("person", request("r1", 0))
        .await
        .unwrap();
    updates.on_shell_line(
        r#"{"type":"app_update_available","release_id":"r2","version":"100.0.0","changed":true}"#,
    );
    assert_eq!(phase(updates), AppUpdatePhase::Available);
    say(updates, "app_update_ready", json!({}));
    say(
        updates,
        "app_update_failed",
        json!({"message":"late r1 failure"}),
    );
    let shown = updates.app_status(&lock(&updates.known), false);
    assert_eq!(shown.latest.as_deref(), Some("100.0.0"));
    assert_eq!(shown.release_id.as_deref(), Some("r2"));
    assert_eq!(shown.failed_phase, None);
    assert!(
        shown
            .update_error
            .as_deref()
            .unwrap()
            .starts_with("The available update changed.")
    );
    assert_eq!(
        unavailable(
            updates
                .install_app("person", request("r1", 1))
                .await
                .unwrap_err()
        ),
        CHANGED
    );
    assert_eq!(
        unavailable(
            updates
                .install_app("person", request("r2", 1))
                .await
                .unwrap_err()
        ),
        CHANGED
    );
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
    assert_eq!(lines.try_recv().unwrap()["type"], "download_app_update");
    assert!(lines.try_recv().is_err());
    updates
        .download_app("person", request("r2", 1))
        .await
        .unwrap();
    say(updates, "app_update_ready", json!({}));
    assert_eq!(
        phase(updates),
        AppUpdatePhase::Downloading,
        "r1 cannot finish r2's new operation"
    );
    updates.on_shell_line(r#"{"type":"app_update_ready","release_id":"r2","operation":2}"#);
    assert_eq!(phase(updates), AppUpdatePhase::Ready);
}

#[tokio::test]
async fn stale_consent_or_a_gone_shell_never_leave_admission_held() {
    let (_dir, core, lines) = fixture();
    let updates = &core.updates;
    updates
        .on_shell_line(r#"{"type":"app_update_available","release_id":"r2","version":"99.0.0"}"#);
    assert_eq!(
        unavailable(
            updates
                .download_app("person", request("r1", 0))
                .await
                .unwrap_err()
        ),
        CHANGED
    );
    drop(lines);
    assert_eq!(
        update_failed(
            updates
                .download_app("person", request("r2", 0))
                .await
                .unwrap_err()
        ),
        SHELL_GONE
    );
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
    assert_eq!(phase(updates), AppUpdatePhase::Available);
}
#[tokio::test]
async fn automatic_intent_waits_for_verification_and_keeps_admission() {
    let (_dir, core, mut lines) = fixture();
    let updates = &core.updates;
    updates
        .start_app("automatic", request("r1", 0), AppIntent::Complete)
        .unwrap();
    assert!(matches!(
        core.workspace.admit_with(|| Ok(())),
        Err(CoreError::Updating)
    ));
    say(
        updates,
        "app_update_progress",
        json!({"downloaded":100,"total":100}),
    );
    assert_eq!(phase(updates), AppUpdatePhase::Downloading);
    assert_eq!(lines.try_recv().unwrap()["type"], "download_app_update");
    assert!(lines.try_recv().is_err());
    say(updates, "app_update_ready", json!({}));
    assert_eq!(phase(updates), AppUpdatePhase::Installing);
    assert!(
        core.store
            .list_audit(None, 20)
            .unwrap()
            .iter()
            .any(|row| row.action == "app.update.install" && row.actor == "automatic")
    );
    assert_eq!(lines.try_recv().unwrap()["type"], "install_app_update");
    assert!(matches!(
        core.workspace.admit_with(|| Ok(())),
        Err(CoreError::Updating)
    ));
    say(
        updates,
        "app_update_failed",
        json!({"message":"failed","retained":false}),
    );
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
}

#[tokio::test]
async fn active_runs_refuse_both_download_and_ready_install() {
    use crate::store::{Admission, NewProject, NewThread};
    let (dir, core, mut lines) = fixture();
    let path = dir.path().to_str().unwrap();
    let project = core
        .store
        .add_project(NewProject {
            path,
            name: "fake",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    core.store.trust_project(&project.id, "person").unwrap();
    let thread = core
        .store
        .create_thread(NewThread {
            project_id: project.id,
            title: "fake".into(),
            engine: "fake".into(),
            model: None,
            cwd: path.into(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let admit = || {
        core.store
            .admit_run(Admission {
                thread_id: &thread.id,
                prompt: "fake",
                actor: "person",
            })
            .unwrap()
    };
    let run = admit();
    assert!(matches!(
        core.updates.download_app("person", request("r1", 0)).await,
        Err(CoreError::RunsActive)
    ));
    assert!(lines.try_recv().is_err());
    core.store
        .finish_run(&run.id, orbyn_contracts::RunState::Cancelled, None)
        .unwrap();
    core.updates
        .download_app("person", request("r1", 0))
        .await
        .unwrap();
    say(&core.updates, "app_update_ready", json!({}));
    assert_eq!(lines.try_recv().unwrap()["type"], "download_app_update");
    let _run = admit();
    assert!(matches!(
        core.updates.install_app("person", request("r1", 1)).await,
        Err(CoreError::RunsActive)
    ));
    assert_eq!(phase(&core.updates), AppUpdatePhase::Ready);
    assert!(lines.try_recv().is_err());
}

#[tokio::test]
async fn a_withdrawn_preflight_returns_to_no_update_without_transfer_or_install() {
    let (_dir, core, mut lines) = fixture();
    let updates = &core.updates;
    updates
        .download_app("person", request("r1", 0))
        .await
        .unwrap();
    assert_eq!(lines.try_recv().unwrap()["type"], "download_app_update");
    say(updates, "app_update_none", json!({}));
    let shown = updates.app_status(&lock(&updates.known), false);
    assert_eq!(shown.phase, AppUpdatePhase::Idle);
    assert!(shown.latest.is_none());
    assert!(shown.release_id.is_none());
    assert!(shown.downloaded.is_none() && shown.total.is_none());
    assert!(shown.update_error.is_none());
    assert!(!shown.can_apply);
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
    assert!(
        lines.try_recv().is_err(),
        "preflight ended before any install command"
    );
    say(updates, "app_update_ready", json!({}));
    assert_eq!(
        phase(updates),
        AppUpdatePhase::Idle,
        "late completion cannot restore withdrawn readiness"
    );
}

#[tokio::test]
async fn r1_failure_after_r2_check_names_the_attempt_without_retargeting_consent() {
    let (_dir, core, mut lines) = fixture();
    let updates = &core.updates;
    updates
        .download_app("person", request("r1", 0))
        .await
        .unwrap();
    say(
        updates,
        "app_update_failed",
        json!({"message":"transfer failed","retained":false}),
    );
    assert_eq!(
        updates
            .app_status(&lock(&updates.known), false)
            .failed_version
            .as_deref(),
        Some("99.0.0")
    );
    updates.on_shell_line(
        r#"{"type":"app_update_available","release_id":"r2","version":"100.0.0","changed":true}"#,
    );
    say(
        updates,
        "app_update_failed",
        json!({"message":"late r1 transfer failed","retained":false}),
    );
    let shown = updates.app_status(&lock(&updates.known), false);
    assert_eq!(shown.latest.as_deref(), Some("100.0.0"));
    assert_eq!(shown.release_id.as_deref(), Some("r2"));
    assert_eq!(shown.failed_phase, None);
    assert!(
        shown
            .update_error
            .as_deref()
            .unwrap()
            .starts_with("The available update changed.")
    );
    assert_eq!(shown.phase, AppUpdatePhase::Available);
    assert_eq!(
        unavailable(
            updates
                .download_app("person", request("r1", 1))
                .await
                .unwrap_err()
        ),
        CHANGED
    );
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
    assert_eq!(lines.try_recv().unwrap()["type"], "download_app_update");
    assert!(lines.try_recv().is_err());
}

#[tokio::test]
async fn fresh_release_revokes_ready_stage_but_a_failed_check_retains_it() {
    let (_dir, core, _lines) = fixture();
    let updates = &core.updates;
    updates
        .download_app("person", request("r1", 0))
        .await
        .unwrap();
    say(updates, "app_update_ready", json!({}));
    updates.on_shell_line(r#"{"type":"app_update_check_failed","message":"offline"}"#);
    assert_eq!(phase(updates), AppUpdatePhase::Ready);
    updates.on_shell_line(
        r#"{"type":"app_update_available","release_id":"r2","version":"100.0.0","changed":true}"#,
    );
    assert_eq!(phase(updates), AppUpdatePhase::Available);
    assert_eq!(
        unavailable(
            updates
                .install_app("person", request("r1", 1))
                .await
                .unwrap_err()
        ),
        CHANGED
    );
    assert_eq!(
        unavailable(
            updates
                .install_app("person", request("r2", 1))
                .await
                .unwrap_err()
        ),
        CHANGED
    );
    assert!(core.workspace.admit_with(|| Ok(())).is_ok());
}
