//! The person's stop and restart of a family's host, and the rule that a host with background work
//! open is not stopped unless the person's stop says so.

use serde_json::{Value, json};

use super::*;
use crate::agents::claude_history::project_folder_name;
use crate::agents::driver::host::connection::ClosedReason;

#[tokio::test]
async fn a_stop_ends_the_host_and_its_open_run_and_the_next_run_starts_it_again() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let started = start(&driver, spec("[hang]")).await.unwrap();
    let connection = rig.pool.acquire().await.unwrap();
    rig.pool.stop(false).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Stopped);
    assert_eq!(connection.closed().await, ClosedReason::Stopped);
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    let (_, outcome) = start(&driver, spec("again")).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    // The stop was not a crash.
    let status = rig.pool.status().await;
    assert_eq!((status.restart_count, status.last_crash), (0, None));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_host_with_background_work_open_is_not_stopped_unless_the_stop_says_so() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let (_, outcome) = start(&rig.driver(), spec("[bg]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert_eq!(
        rig.pool.stop(false).await,
        Err(Refusal::BackgroundOpen(1)),
        "the task is still running"
    );
    assert_eq!(rig.pool.state(), HostState::Ready, "nothing was stopped");
    assert_eq!(
        rig.pool.restart_now(false).await,
        Err(Refusal::BackgroundOpen(1))
    );
    assert_eq!(rig.pool.state(), HostState::Ready);
    rig.pool.stop(true).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Stopped);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_stop_with_no_host_running_does_nothing() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    rig.pool.stop(false).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Idle);
}

#[tokio::test]
async fn a_restart_gives_a_family_that_gave_up_a_clean_slate() {
    let dir = crate::testing::TempDir::new();
    let ready = dir.path().join("ready");
    let ready_text = ready.to_string_lossy().into_owned();
    let Some(rig) = standin(&[("STANDIN_NEEDS_FILE", &ready_text)], fast()) else {
        return;
    };
    assert!(rig.pool.acquire().await.is_err());
    wait_for(
        &rig.pool,
        |s| matches!(s, HostState::Unavailable { .. }),
        "unavailable",
    )
    .await;
    assert_eq!(rig.pool.status().await.restart_count, 5);
    // The fault is fixed; the person asks again.
    std::fs::write(&ready, "").unwrap();
    rig.pool.restart_now(false).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Ready);
    // The tally since the app started stays; the backoff and the give-up count are fresh.
    let status = rig.pool.status().await;
    assert_eq!(status.restart_count, 5);
    let (_, outcome) = start(&rig.driver(), spec("[crash]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert!(
        matches!(outcome, EngineOutcome::FailedWith(_)),
        "{outcome:?}"
    );
    match rig.pool.state() {
        HostState::Restarting { attempt, .. } => {
            assert_eq!(attempt, 1, "the old crashes still count")
        }
        other => assert_eq!(other, HostState::Ready, "already back up"),
    }
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_restart_replaces_a_running_host() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let first = rig.pool.acquire().await.unwrap();
    rig.pool.restart_now(false).await.unwrap();
    assert_eq!(first.closed().await, ClosedReason::Stopped);
    let second = rig.pool.acquire().await.unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(rig.pool.state(), HostState::Ready);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_version_mismatch_stays_until_an_update() {
    let Some(rig) = standin(&[("STANDIN_PROTOCOL", "2")], fast()) else {
        return;
    };
    assert!(rig.pool.acquire().await.is_err());
    assert_eq!(rig.pool.stop(true).await, Err(Refusal::VersionMismatch));
    assert_eq!(
        rig.pool.restart_now(true).await,
        Err(Refusal::VersionMismatch)
    );
}

#[tokio::test]
async fn the_hub_names_a_family_that_does_not_use_the_host_or_does_not_exist() {
    let config = HostConfig::from_env(Some("/x/orbyn-engine-host".into()), Some("0")).unwrap();
    let hub = HostHub::new(&config);
    assert_eq!(
        hub.stop("claude", false).await.unwrap_err(),
        ControlError::Refused(Refusal::NotUsed)
    );
    assert_eq!(
        hub.restart("claude", false).await.unwrap_err(),
        ControlError::Refused(Refusal::NotUsed)
    );
    assert_eq!(
        hub.stop("nope", false).await.unwrap_err(),
        ControlError::UnknownFamily
    );
}

#[tokio::test]
async fn the_status_names_the_threads_working_and_the_kind_of_the_last_failure() {
    use crate::agents::driver::host::ErrorKind;
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    let running = start(&driver, spec("[hang]")).await.unwrap();
    assert_eq!(rig.pool.status().await.threads, vec!["thread-1".to_owned()]);
    running.handle.cancel.cancel();
    running.outcome().await;
    assert!(rig.pool.status().await.threads.is_empty());

    start(&driver, spec("[lost]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(
        rig.pool.status().await.last_error,
        Some(ErrorKind::SessionLost)
    );
    start(&driver, spec("[limit]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(
        rig.pool.status().await.last_error,
        Some(ErrorKind::RateLimit)
    );
    // A run that completes clears it: the screen never shows a stale failure.
    start(&driver, spec("fine")).await.unwrap().outcome().await;
    assert_eq!(rig.pool.status().await.last_error, None);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_run_the_person_cancelled_leaves_the_last_error_alone_whatever_the_engine_called_its_end()
{
    use crate::agents::driver::host::ErrorKind;
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let driver = rig.driver();
    start(&driver, spec("[limit]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(
        rig.pool.status().await.last_error,
        Some(ErrorKind::RateLimit)
    );
    // The SDK ends an interrupted turn as a failure; the person asked for it, so it is not shown as one.
    let started = start(&driver, spec("[hang] [failcancel]")).await.unwrap();
    started.handle.cancel.cancel();
    let (_, outcome) = started.outcome().await;
    assert_eq!(outcome, EngineOutcome::Cancelled);
    assert_eq!(
        rig.pool.status().await.last_error,
        Some(ErrorKind::RateLimit)
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_stop_while_the_host_waits_to_restart_ends_the_restarts() {
    let slow = Timings {
        backoff_base: Duration::from_millis(500),
        backoff_cap: Duration::from_millis(500),
        ..fast()
    };
    let Some(rig) = standin(&[], slow) else {
        return;
    };
    let (_, outcome) = start(&rig.driver(), spec("[crash]"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert!(
        matches!(outcome, EngineOutcome::FailedWith(_)),
        "{outcome:?}"
    );
    assert!(matches!(rig.pool.state(), HostState::Restarting { .. }));
    rig.pool.stop(false).await.unwrap();
    assert_eq!(rig.pool.state(), HostState::Stopped);
    // The wait is over and nothing started the host.
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(rig.pool.state(), HostState::Stopped);
    rig.pool.shutdown().await;
}

const SESSION: &str = "0f3c2b1a-1111-4222-8333-444455556666";
const PROMPT: &str = "a1a1a1a1-1111-4222-8333-444455556666";
const HEAD: &str = "b2b2b2b2-1111-4222-8333-444455556666";

fn user(uuid: &str) -> Value {
    json!({"type":"user","uuid":uuid,"sessionId":SESSION,"message":{"role":"user","content":"hi"}})
}

fn assistant(uuid: &str) -> Value {
    json!({"type":"assistant","uuid":uuid,"sessionId":SESSION,"message":{"role":"assistant","content":"answer"}})
}

fn transcript(projects: &Path, cwd: &str, lines: &[Value]) {
    let folder = projects.join(project_folder_name(cwd));
    std::fs::create_dir_all(&folder).unwrap();
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    std::fs::write(folder.join(format!("{SESSION}.jsonl")), text).unwrap();
}

fn write_raw(projects: &Path, cwd: &str, text: &str) {
    let folder = projects.join(project_folder_name(cwd));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(format!("{SESSION}.jsonl")), text).unwrap();
}

fn opens(rig: &Rig) -> Vec<Value> {
    rig.raw()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|frame| frame["type"] == "open")
        .collect()
}

#[tokio::test]
async fn a_claude_rewind_reads_the_session_file_and_resumes_once_at_the_previous_assistant() {
    let Some(node) = node() else {
        return;
    };
    let Some(rig) = standin_family("claude", &[], fast()) else {
        return;
    };
    let history = TempDir::new();
    let projects = history.path().join("projects");
    let library = history
        .path()
        .join("library")
        .to_string_lossy()
        .into_owned();
    let checkout = history.path().join("repo").to_string_lossy().into_owned();
    // A native Claude thread starts in the library; the thread keeps the checkout folder.
    transcript(
        &projects,
        &library,
        &[
            user("u0"),
            assistant("old"),
            json!({"type":"assistant","uuid":"sub-1","sessionId":SESSION,"isSidechain":true,"message":{}}),
            json!({"type":"assistant","uuid":"sub-2","sessionId":SESSION,"parent_tool_use_id":"toolu_1","message":{}}),
            json!({"type":"user","uuid":"tool-1","sessionId":SESSION,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"ok"}]}}),
            assistant(HEAD),
            json!({"type":"assistant","uuid":"sub-3","sessionId":SESSION,"isSidechain":true,"message":{}}),
            user(PROMPT),
        ],
    );
    let driver = driver_on_node(&rig.pool, &node).with_history_root(projects);
    driver
        .revert_session(Path::new(&checkout), SESSION, PROMPT)
        .await
        .unwrap();
    assert!(
        rig.log().is_empty(),
        "a rewind reads the file and asks the host nothing: {:?}",
        rig.log()
    );

    let mut next = spec("after the rewind");
    next.resume = Some(SESSION.into());
    next.cwd = PathBuf::from(&library);
    next.checkout = PathBuf::from(&checkout);
    let (events, outcome) = start(&driver, next).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::Session(id) if id == "eng-run-1")),
        "the host named its session: {events:?}"
    );
    // The open consumed the point; the session frame leaves nothing for a later run.
    assert_eq!(rig.pool.take_resume_at("claude", SESSION), None);
    let first = opens(&rig);
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0]["operation"], "resume");
    assert_eq!(first[0]["engineSessionId"], SESSION);
    assert_eq!(first[0]["resumeAt"], HEAD);

    let mut again = spec("again");
    again.resume = Some(SESSION.into());
    again.cwd = PathBuf::from(&library);
    again.checkout = PathBuf::from(&checkout);
    let (_, outcome) = start(&driver, again).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let both = opens(&rig);
    assert_eq!(both.len(), 2, "{both:?}");
    assert!(
        both[1].get("resumeAt").is_none(),
        "one shot, not every resume: {:?}",
        both[1]
    );
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_claude_rewind_without_a_usable_point_is_refused_so_the_handoff_stays() {
    let Some(node) = node() else {
        return;
    };
    let Some(rig) = standin_family("claude", &[], fast()) else {
        return;
    };
    let history = TempDir::new();
    let projects = history.path().join("projects");
    let cwd = history.path().join("repo").to_string_lossy().into_owned();
    let driver = driver_on_node(&rig.pool, &node)
        .with_history_root(projects.clone())
        .with_path(Arc::new(|| Ok(EnginePath::Sdk)));

    // A missing file.
    assert!(
        driver
            .revert_session(Path::new(&cwd), SESSION, PROMPT)
            .await
            .is_err()
    );

    // No assistant before the prompt.
    transcript(&projects, &cwd, &[user(PROMPT)]);
    assert!(
        driver
            .revert_session(Path::new(&cwd), SESSION, PROMPT)
            .await
            .is_err()
    );

    // Only a subagent assistant before it.
    transcript(
        &projects,
        &cwd,
        &[
            json!({"type":"assistant","uuid":"sub","sessionId":SESSION,"isSidechain":true,"message":{}}),
            user(PROMPT),
        ],
    );
    assert!(
        driver
            .revert_session(Path::new(&cwd), SESSION, PROMPT)
            .await
            .is_err()
    );

    // A prompt uuid the file does not hold.
    transcript(&projects, &cwd, &[assistant(HEAD), user("another-prompt")]);
    assert!(
        driver
            .revert_session(Path::new(&cwd), SESSION, PROMPT)
            .await
            .is_err()
    );

    // The host's synthetic turn id is not a prompt uuid.
    transcript(&projects, &cwd, &[assistant(HEAD), user(PROMPT)]);
    assert!(
        driver
            .revert_session(Path::new(&cwd), SESSION, "turn-1")
            .await
            .is_err()
    );

    // A changed or truncated file.
    write_raw(&projects, &cwd, "{\"type\":\"assistant\",\"uuid\":\"cut");
    assert!(
        driver
            .revert_session(Path::new(&cwd), SESSION, PROMPT)
            .await
            .is_err()
    );

    assert_eq!(rig.pool.take_resume_at("claude", SESSION), None);
    assert!(rig.log().is_empty(), "nothing was asked of the host");
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_claude_rewind_on_the_cli_path_is_refused() {
    let Some(node) = node() else {
        return;
    };
    let Some(rig) = standin_family("claude", &[], fast()) else {
        return;
    };
    let history = TempDir::new();
    let projects = history.path().join("projects");
    let cwd = history.path().join("repo").to_string_lossy().into_owned();
    transcript(&projects, &cwd, &[assistant(HEAD), user(PROMPT)]);
    let driver = driver_on_node(&rig.pool, &node)
        .with_history_root(projects)
        .with_path(Arc::new(|| Ok(EnginePath::Cli)));
    assert!(
        driver
            .revert_session(Path::new(&cwd), SESSION, PROMPT)
            .await
            .is_err(),
        "the CLI path has no resumeAt"
    );
    assert_eq!(rig.pool.take_resume_at("claude", SESSION), None);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn resume_at_is_never_sent_on_create_or_for_another_family() {
    let Some(node) = node() else {
        return;
    };
    let Some(rig) = standin_family("claude", &[], fast()) else {
        return;
    };
    let driver = driver_on_node(&rig.pool, &node);
    rig.pool.note_resume_at("claude", "session-1", HEAD);
    let (_, outcome) = start(&driver, spec("create"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let created = opens(&rig);
    assert_eq!(created[0]["operation"], "create");
    assert!(created[0].get("resumeAt").is_none());
    assert_eq!(
        rig.pool.take_resume_at("claude", "session-1"),
        Some(HEAD.to_owned()),
        "a create did not consume it"
    );
    rig.pool.shutdown().await;

    let Some(other) = standin(&[], fast()) else {
        return;
    };
    let other_driver = driver_on_node(&other.pool, &node);
    other.pool.note_resume_at("claude", "eng-run-1", HEAD);
    let mut resumed = spec("resume");
    resumed.resume = Some("eng-run-1".into());
    let (_, outcome) = start(&other_driver, resumed).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let open = opens(&other);
    assert_eq!(open[0]["operation"], "resume");
    assert!(open[0].get("resumeAt").is_none());
    assert_eq!(
        other.pool.take_resume_at("claude", "eng-run-1"),
        Some(HEAD.to_owned()),
        "another family did not consume it"
    );
    other.pool.shutdown().await;
}
