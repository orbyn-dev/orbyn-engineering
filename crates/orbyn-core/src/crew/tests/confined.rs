//! A crew worker's process runs under a restriction, or is not started. The fake engine
//! starts no process, so this checks what the run manager hands a driver and what a
//! start says where no restriction exists; `host::confine` runs the real restriction.

use std::sync::Arc;

use serde_json::json;

use super::harness::{Turn, brief, key, world_confined};
#[cfg(not(target_os = "macos"))]
use crate::host::confine::UNSUPPORTED_PLATFORM;

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_worker_run_is_handed_rules_that_deny_the_data_folder_but_not_its_own_run_folder() {
    let w = world_confined(Arc::new(|_, _| Turn::Hang)).await;
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let worker = w.worker(answer["task"].as_str().unwrap());
    w.until("the worker's run started", |w| {
        w.confines.lock().unwrap().iter().any(|(t, _)| *t == worker)
    })
    .await;
    let rules = w
        .confines
        .lock()
        .unwrap()
        .iter()
        .find(|(thread, _)| *thread == worker)
        .and_then(|(_, rules)| rules.clone())
        .expect("a worker's run carries its restriction");
    let data = std::fs::canonicalize(&w.core.config.data_dir).unwrap();
    assert_eq!(rules.data, data);
    assert!(rules.run_dir.starts_with(data.join("runs")), "{rules:?}");
    let repo = std::fs::canonicalize(&w.repo).unwrap();
    assert!(rules.allow_write.contains(&repo), "{rules:?}");
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn a_worker_is_refused_where_no_restriction_exists() {
    let w = world_confined(Arc::new(|_, _| Turn::Hang)).await;
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(error, "{answer}");
    assert!(
        answer.to_string().contains(UNSUPPORTED_PLATFORM),
        "{answer}"
    );
    let (id, state): (String, String) = w
        .core
        .store
        .reader()
        .query_row("SELECT id, state FROM crew_tasks", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(state, "needs_person", "task {id}");
    assert!(
        w.confines.lock().unwrap().is_empty(),
        "no process was asked for"
    );
    assert!(w.prompts.lock().unwrap().is_empty());
}
