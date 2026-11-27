//! Tests that drive real host processes: a scripted stand-in (`tests/standin.mjs`, which
//! misbehaves on purpose) and the engine host's own `fake` family. They need `node`; without
//! it a test says so loudly and returns, it never passes silently.

mod acp;
mod asks;
mod control;
mod cursor;
mod pool;
mod real;
mod runs;
mod secrets;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;

use super::*;
use crate::agents::driver::claude::ClaudeDriver;
use crate::agents::driver::{EngineEvent, EngineOutcome, RunHandle, RunSpec};
use crate::error::Result;
use crate::testing::TempDir;

/// Short times, so a crash, a stall or an idle stop is seen in tens of milliseconds.
pub(super) fn fast() -> Timings {
    Timings {
        hello: Duration::from_millis(2500),
        first_hello: Duration::from_millis(2500),
        init: Duration::from_millis(2500),
        backoff_base: Duration::from_millis(20),
        backoff_cap: Duration::from_millis(80),
        restart_window: Duration::from_secs(60),
        ping_every: Duration::from_millis(50),
        exit_grace: Duration::from_millis(1500),
        idle_stop: None,
        cancel_grace: Duration::from_millis(1500),
        ..Timings::default()
    }
}

/// What a host test does when node is missing: skip, or fail when the run says node must be there.
/// The build box sets `ORBYN_REQUIRE_NODE=1`, so a missing node there is a red run, not a quiet pass.
fn node_or_skip(found: Option<PathBuf>, required: bool) -> Option<PathBuf> {
    if found.is_none() {
        assert!(
            !required,
            "ORBYN_REQUIRE_NODE=1 and node was not found, so no engine host process can run here"
        );
        eprintln!("SKIPPED: node was not found, so no engine host process can run here");
    }
    found
}

fn node() -> Option<PathBuf> {
    let found = crate::host::process::resolve("node").map(|command| command.program);
    node_or_skip(
        found,
        std::env::var("ORBYN_REQUIRE_NODE").is_ok_and(|v| v == "1"),
    )
}

#[test]
#[should_panic(expected = "ORBYN_REQUIRE_NODE=1")]
fn a_missing_node_fails_the_run_when_the_run_requires_it() {
    node_or_skip(None, true);
}

#[test]
fn a_missing_node_skips_when_it_is_not_required_and_a_found_one_is_used() {
    assert_eq!(node_or_skip(None, false), None);
    let found = PathBuf::from("/usr/bin/node");
    assert_eq!(node_or_skip(Some(found.clone()), true), Some(found.clone()));
    assert_eq!(node_or_skip(Some(found.clone()), false), Some(found));
}

/// A pool over the stand-in, with its request log in a private folder.
pub(super) struct Rig {
    pub pool: Arc<Pool>,
    dir: TempDir,
}

impl Rig {
    pub fn log(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.path().join("requests.log"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Every line the host received, exactly as it arrived.
    pub fn raw(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("requests.log.raw")).unwrap_or_default()
    }

    pub fn driver(&self) -> HostDriver {
        driver(&self.pool)
    }
}

pub(super) fn driver(pool: &Arc<Pool>) -> HostDriver {
    HostDriver::new(
        Arc::new(ClaudeDriver::new("claude".into(), Duration::from_secs(1))),
        pool.clone(),
        "claude".into(),
    )
}

/// A host driver whose Claude program is `node`, so a test of the real `claude` family
/// never needs the person's engine binary. The stand-in is the host either way.
pub(super) fn driver_on_node(pool: &Arc<Pool>, node: &Path) -> HostDriver {
    HostDriver::new(
        Arc::new(ClaudeDriver::new("claude".into(), Duration::from_secs(1))),
        pool.clone(),
        node.to_string_lossy().into_owned(),
    )
}

pub(super) fn standin(env: &[(&str, &str)], timings: Timings) -> Option<Rig> {
    standin_with(env, timings, None)
}

pub(super) fn standin_with(
    env: &[(&str, &str)],
    timings: Timings,
    secrets: Option<SecretSource>,
) -> Option<Rig> {
    standin_family_with("fake", env, timings, secrets)
}

/// The stand-in as another family, so its script can be the family it stands in for.
pub(super) fn standin_family(family: &str, env: &[(&str, &str)], timings: Timings) -> Option<Rig> {
    standin_family_with(family, env, timings, None)
}

fn standin_family_with(
    family: &str,
    env: &[(&str, &str)],
    timings: Timings,
    secrets: Option<SecretSource>,
) -> Option<Rig> {
    let program = node()?;
    let dir = TempDir::new();
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/agents/driver/host/tests/standin.mjs");
    let mut vars: Vec<(String, String)> = env
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    vars.push(("STANDIN_FAMILY".into(), family.to_owned()));
    vars.push((
        "STANDIN_LOG".into(),
        dir.path()
            .join("requests.log")
            .to_string_lossy()
            .into_owned(),
    ));
    let spec = HostSpec {
        program,
        args: vec![script.into_os_string()],
        env: vars,
        instance: "test".into(),
    };
    Some(Rig {
        pool: Pool::new(family, spec, timings, secrets),
        dir,
    })
}

/// The engine host's own `fake` family, when this checkout has the host source.
pub(super) fn real_fake(timings: Timings) -> Option<Arc<Pool>> {
    let program = node()?;
    let main = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../engine-host/src/main.mjs");
    if !main.is_file() {
        eprintln!("SKIPPED: engine-host/src/main.mjs is not in this checkout");
        return None;
    }
    let spec = HostSpec {
        program,
        args: vec![main.into_os_string(), "fake".into()],
        env: Vec::new(),
        instance: "test".into(),
    };
    Some(Pool::new("fake", spec, timings, None))
}

pub(super) fn spec(prompt: &str) -> RunSpec {
    let mut spec = support::run_spec();
    spec.user_prompt = prompt.to_owned();
    spec
}

pub(super) struct Started {
    pub handle: RunHandle,
    pub events: mpsc::Receiver<EngineEvent>,
}

pub(super) async fn start(driver: &HostDriver, spec: RunSpec) -> Result<Started> {
    use crate::agents::driver::EngineDriver;
    let (tx, events) = mpsc::channel(256);
    let handle = driver.start_run(spec, tx).await?;
    Ok(Started { handle, events })
}

impl Started {
    /// Reads until the channel closes; returns every event and how many `Finished` there were.
    pub async fn finish(mut self) -> (Vec<EngineEvent>, Vec<EngineOutcome>) {
        let mut events = Vec::new();
        let mut finished = Vec::new();
        let read = async {
            while let Some(event) = self.events.recv().await {
                match event {
                    EngineEvent::Finished(outcome) => finished.push(outcome),
                    other => events.push(other),
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(15), read)
            .await
            .expect("the run never ended");
        (events, finished)
    }

    /// Runs to the end and returns the one outcome.
    pub async fn outcome(self) -> (Vec<EngineEvent>, EngineOutcome) {
        let (events, mut finished) = self.finish().await;
        assert_eq!(finished.len(), 1, "exactly one Finished: {finished:?}");
        (events, finished.remove(0))
    }
}

pub(super) async fn wait_for(pool: &Arc<Pool>, want: impl Fn(&HostState) -> bool, what: &str) {
    let mut changes = pool.subscribe();
    let waited = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if want(&changes.borrow_and_update()) {
                return;
            }
            changes.changed().await.unwrap();
        }
    })
    .await;
    assert!(
        waited.is_ok(),
        "never reached {what}; state is {:?}",
        pool.state()
    );
}
