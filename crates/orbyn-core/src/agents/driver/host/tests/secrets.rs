//! The engine's key reaches the host's spawn environment and nowhere else: not argv, not a
//! frame, not a log line, not an error. The source is the person's opt-in, held by the core.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zeroize::Zeroizing;

use super::*;
use crate::credentials::{Credentials, DEV_SERVICE, Failure, SecretBackend};

pub(super) const KEY: &str = "sk-live-0123456789abcdef";

#[derive(Default)]
struct Memory(Mutex<HashMap<(String, String), Vec<u8>>>);

impl SecretBackend for Memory {
    fn get(&self, service: &str, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, Failure> {
        let items = self.0.lock().unwrap();
        Ok(items
            .get(&(service.into(), account.into()))
            .cloned()
            .map(Zeroizing::new))
    }
    fn set(&self, service: &str, account: &str, bytes: &[u8]) -> Result<(), Failure> {
        self.0
            .lock()
            .unwrap()
            .insert((service.into(), account.into()), bytes.to_vec());
        Ok(())
    }
    fn delete(&self, service: &str, account: &str) -> Result<(), Failure> {
        self.0
            .lock()
            .unwrap()
            .remove(&(service.into(), account.into()));
        Ok(())
    }
}

/// A source that hands out `KEY` for `cursor`, as the core's would after an opt-in.
pub(super) fn source() -> SecretSource {
    let memory = Arc::new(Memory::default());
    let credentials = Credentials::new(memory.clone(), DEV_SERVICE);
    credentials.save("cursor", true, KEY).unwrap();
    Arc::new(move || credentials.take_for_spawn("cursor", true).ok().flatten())
}

fn spawn_record(rig: &Rig) -> Value {
    rig.log()
        .into_iter()
        .find(|line| line["type"] == "spawn")
        .expect("the host logged its spawn")
}

#[tokio::test]
async fn the_key_is_in_the_spawn_environment_and_never_in_argv_a_frame_or_a_log() {
    let Some(rig) = standin_with(&[], fast(), Some(source())) else {
        return;
    };
    let driver = rig.driver();
    let (events, outcome) = start(&driver, spec("hi")).await.unwrap().outcome().await;
    assert_eq!(outcome, EngineOutcome::Completed);
    let spawn = spawn_record(&rig);
    assert_eq!(spawn["keyInEnv"], true, "the host did not get its key");
    assert_eq!(spawn["keyInArgv"], false, "the key was on the command line");
    assert!(!rig.raw().contains(KEY), "the key was sent in a frame");
    assert!(
        !serde_json::to_string(&rig.log()).unwrap().contains(KEY),
        "the key reached the request log"
    );
    let status = format!("{:?}", rig.pool.status().await);
    assert!(!status.contains(KEY), "{status}");
    assert!(!format!("{events:?}").contains(KEY));
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn no_source_means_no_key_in_the_environment() {
    let Some(rig) = standin(&[], fast()) else {
        return;
    };
    rig.pool.acquire().await.unwrap();
    assert_eq!(spawn_record(&rig)["keyInEnv"], false);
    rig.pool.shutdown().await;
}

#[tokio::test]
async fn a_host_that_prints_the_key_as_it_dies_leaves_it_out_of_the_crash_and_the_error() {
    let Some(rig) = standin_with(
        &[
            ("STANDIN_LEAK_KEY", "1"),
            ("STANDIN_EXIT_BEFORE_HELLO", "1"),
        ],
        fast(),
        Some(source()),
    ) else {
        return;
    };
    let error = rig.pool.acquire().await.unwrap_err();
    assert!(!error.0.contains(KEY), "{}", error.0);
    let status = rig.pool.status().await;
    let (crash, _) = status.last_crash.clone().expect("a crash was recorded");
    assert!(!crash.stderr_tail.contains(KEY), "{}", crash.stderr_tail);
    assert!(!crash.message.contains(KEY));
    assert!(!format!("{status:?}").contains(KEY));
    rig.pool.shutdown().await;
}
