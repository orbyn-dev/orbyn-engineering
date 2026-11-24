//! The per-run choice between the SDK (the host) and the CLI (the wrapped Rust driver).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::tests::{fast, standin, start};
use super::*;
use crate::agents::driver::{
    EngineDescriptor, EngineEvent, EngineOutcome, EngineProbe, RunHandle, RunSpec,
};
use crate::agents::engines::stand_in::StandIn;
use crate::error::{CoreError, Result};

/// The wrapped Rust driver: records the resume id of every run it is handed and finishes at once.
#[derive(Default)]
struct Recorder {
    resumes: Mutex<Vec<Option<String>>>,
}

impl Recorder {
    fn seen(&self) -> Vec<Option<String>> {
        self.resumes.lock().unwrap().clone()
    }
}

#[async_trait]
impl EngineDriver for Recorder {
    fn descriptor(&self) -> EngineDescriptor {
        StandIn::new("claude", Vec::new(), Arc::default()).descriptor()
    }

    async fn probe(&self) -> EngineProbe {
        EngineProbe::default()
    }

    async fn start_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        self.resumes.lock().unwrap().push(spec.resume.clone());
        let task = tokio::spawn(async move {
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Completed))
                .await;
        });
        Ok(RunHandle {
            cancel: CancellationToken::new(),
            task,
            input: None,
            control: None,
        })
    }
}

fn choice(initial: EnginePath) -> (Arc<Mutex<EnginePath>>, PathSetting) {
    let cell = Arc::new(Mutex::new(initial));
    let read = cell.clone();
    (cell, Arc::new(move || Ok(*read.lock().unwrap())))
}

fn over(pool: &Arc<Pool>, inner: &Arc<Recorder>, path: PathSetting) -> HostDriver {
    HostDriver::new(inner.clone(), pool.clone(), "claude".into()).with_path(path)
}

#[test]
fn the_path_setting_is_sdk_or_cli_and_nothing_else() {
    assert_eq!(EnginePath::default(), EnginePath::Sdk);
    assert_eq!(
        EnginePath::from_preference(&json!("sdk")).unwrap(),
        EnginePath::Sdk
    );
    assert_eq!(
        EnginePath::from_preference(&json!("cli")).unwrap(),
        EnginePath::Cli
    );
    for bad in [
        json!("auto"),
        json!(""),
        json!(1),
        json!(null),
        json!("SDK"),
    ] {
        assert!(EnginePath::from_preference(&bad).is_err(), "{bad}");
    }
}

#[tokio::test]
async fn each_run_reads_the_setting_so_cli_never_touches_the_host_and_sdk_does() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let inner = Arc::new(Recorder::default());
    let (cell, path) = choice(EnginePath::Cli);
    let driver = over(&rig.pool, &inner, path);

    let (_, outcome) = start(&driver, super::tests::spec("one"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert_eq!(inner.seen().len(), 1);
    assert!(
        rig.log().is_empty(),
        "a CLI run sent the host nothing: {:?}",
        rig.log()
    );

    // The person changes the setting: the next run follows it, with no restart.
    *cell.lock().unwrap() = EnginePath::Sdk;
    let (_, outcome) = start(&driver, super::tests::spec("two"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(outcome, EngineOutcome::Completed);
    assert_eq!(
        inner.seen().len(),
        1,
        "the SDK run did not reach the CLI driver"
    );
    assert!(rig.log().iter().any(|r| r["type"] == "send"));

    *cell.lock().unwrap() = EnginePath::Cli;
    start(&driver, super::tests::spec("three"))
        .await
        .unwrap()
        .outcome()
        .await;
    assert_eq!(inner.seen().len(), 2);
    // Late pings and closes may still arrive; what matters is that no second turn was sent.
    let sends = rig.log().iter().filter(|r| r["type"] == "send").count();
    assert_eq!(sends, 1, "and back to the CLI");
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_setting_that_cannot_be_read_fails_the_run_and_picks_neither_path() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let inner = Arc::new(Recorder::default());
    let broken: PathSetting = Arc::new(|| Err(CoreError::Invalid("unreadable".into())));
    let driver = over(&rig.pool, &inner, broken);
    assert!(start(&driver, super::tests::spec("x")).await.is_err());
    assert!(inner.seen().is_empty());
    assert!(rig.log().is_empty());
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_down_host_under_sdk_fails_with_its_reason_and_never_falls_back_to_the_cli() {
    let spec = HostSpec {
        program: "/nonexistent/orbyn-engine-host".into(),
        args: Vec::new(),
        env: Vec::new(),
        instance: "test".into(),
    };
    let pool = Pool::new("claude", spec, fast(), None);
    let inner = Arc::new(Recorder::default());
    let (_, path) = choice(EnginePath::Sdk);
    let driver = over(&pool, &inner, path);
    let error = start(&driver, super::tests::spec("x"))
        .await
        .err()
        .expect("the run fails");
    assert!(
        matches!(error, CoreError::EngineUnavailable(_)),
        "{error:?}"
    );
    assert!(inner.seen().is_empty(), "nothing switched to the CLI");
    pool.shutdown().await;
}

#[tokio::test]
async fn a_thread_moves_between_the_paths_on_the_same_session_id() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    let inner = Arc::new(Recorder::default());
    let (cell, path) = choice(EnginePath::Sdk);
    let driver = over(&rig.pool, &inner, path);

    // A thread started on the SDK names its session; the CLI resumes that same id.
    let started = start(&driver, super::tests::spec("first")).await.unwrap();
    let (events, _) = started.outcome().await;
    let session = events
        .iter()
        .find_map(|event| match event {
            EngineEvent::Session(id) => Some(id.clone()),
            _ => None,
        })
        .expect("the host names the session");
    *cell.lock().unwrap() = EnginePath::Cli;
    let mut resumed = super::tests::spec("second");
    resumed.resume = Some(session.clone());
    start(&driver, resumed).await.unwrap().outcome().await;
    assert_eq!(inner.seen(), vec![Some(session)]);

    // A thread started on the CLI (it has a resume id) is opened by the host as a resume.
    *cell.lock().unwrap() = EnginePath::Sdk;
    let mut from_cli = super::tests::spec("third");
    from_cli.resume = Some("cli-session-9".into());
    start(&driver, from_cli).await.unwrap().outcome().await;
    let open: serde_json::Value = rig
        .raw()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .rfind(|frame| frame["type"] == "open")
        .expect("an open request");
    assert_eq!(open["operation"], "resume");
    assert_eq!(open["engineSessionId"], "cli-session-9");
    rig.pool.shutdown().await;
}

/// A wrapped driver that can warm and counts what it was asked.
#[derive(Default)]
struct Warmer {
    keys: Mutex<usize>,
    starts: Mutex<usize>,
    adopted: Mutex<usize>,
}

#[async_trait]
impl EngineDriver for Warmer {
    fn descriptor(&self) -> EngineDescriptor {
        StandIn::new("claude", Vec::new(), Arc::default()).descriptor()
    }

    async fn probe(&self) -> EngineProbe {
        EngineProbe::default()
    }

    async fn start_run(
        &self,
        _spec: RunSpec,
        _events: mpsc::Sender<EngineEvent>,
    ) -> Result<RunHandle> {
        Err(CoreError::Internal("not started in this test".into()))
    }

    fn can_warm(&self) -> bool {
        true
    }

    fn warm_key(&self, _spec: &RunSpec) -> Option<String> {
        *self.keys.lock().unwrap() += 1;
        Some("key".into())
    }

    async fn start_warm(
        &self,
        _spec: RunSpec,
    ) -> Result<Option<crate::agents::driver::WarmProcess>> {
        *self.starts.lock().unwrap() += 1;
        Ok(None)
    }

    async fn adopt_run(
        &self,
        spec: RunSpec,
        events: mpsc::Sender<EngineEvent>,
        _warm: crate::agents::driver::WarmProcess,
    ) -> Result<RunHandle> {
        *self.adopted.lock().unwrap() += 1;
        Recorder::default().start_run(spec, events).await
    }
}

/// The CLI path warms through the wrapped driver; the SDK path, and every family but Claude, warm nothing.
#[tokio::test]
async fn a_host_wrapped_claude_warms_only_on_the_cli_path_and_reads_the_setting_each_time() {
    let Some(rig) = super::tests::standin_family("claude", &[], fast()) else {
        return;
    };
    let inner = Arc::new(Warmer::default());
    let (cell, path) = choice(EnginePath::Cli);
    let driver = HostDriver::new(inner.clone(), rig.pool.clone(), "claude".into()).with_path(path);
    let spec = super::tests::spec("warm");

    assert!(driver.can_warm());
    assert_eq!(driver.warm_key(&spec).as_deref(), Some("key"));
    driver.start_warm(spec.clone()).await.unwrap();
    assert_eq!(
        (*inner.keys.lock().unwrap(), *inner.starts.lock().unwrap()),
        (1, 1)
    );

    *cell.lock().unwrap() = EnginePath::Sdk;
    assert!(!driver.can_warm());
    assert_eq!(driver.warm_key(&spec), None);
    assert!(driver.start_warm(spec.clone()).await.unwrap().is_none());
    assert_eq!(
        (*inner.keys.lock().unwrap(), *inner.starts.lock().unwrap()),
        (1, 1),
        "the SDK path asked the wrapped driver for nothing"
    );
    rig.pool.shutdown().await;

    // Another family never warms, even on a CLI setting.
    let Some(other) = standin(&[], fast()) else {
        return;
    };
    let (_, path) = choice(EnginePath::Cli);
    let driver = HostDriver::new(inner, other.pool.clone(), "fake".into()).with_path(path);
    assert!(!driver.can_warm());
    other.pool.shutdown().await;
}

/// A CLI run adopted from a warm process invalidates a rewind point noted on the SDK path, as a started CLI run does; on the SDK
/// path the warm process is ended unused and the run is the host's.
#[cfg(unix)]
#[tokio::test]
async fn adopting_a_warm_cli_run_clears_a_pending_rewind_point() {
    let Some(rig) = super::tests::standin_family("claude", &[], fast()) else {
        return;
    };
    let inner = Arc::new(Warmer::default());
    let (_, path) = choice(EnginePath::Cli);
    let driver = HostDriver::new(inner.clone(), rig.pool.clone(), "claude".into()).with_path(path);
    let point = "11111111-2222-4333-8444-555555555555";
    rig.pool
        .note_resume_at("claude", "session-1", point)
        .unwrap();

    let warm = crate::agents::driver::stand_in_warm("key").unwrap();
    let (events, _received) = mpsc::channel(8);
    driver
        .adopt_run(super::tests::spec("adopt"), events, warm)
        .await
        .unwrap();
    assert_eq!(*inner.adopted.lock().unwrap(), 1, "the CLI driver ran it");
    assert_eq!(
        rig.pool.take_resume_at("claude", "session-1"),
        None,
        "the point was cleared"
    );
    rig.pool.shutdown().await;
}
