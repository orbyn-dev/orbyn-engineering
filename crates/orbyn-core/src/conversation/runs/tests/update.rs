//! The engine updater while runs are live: an update is refused during a run and an
//! admission during an update, a hung updater is stopped with its children, and an
//! update ends the process warming for the next turn. They need a shell, so unix only.

use super::*;

#[cfg(unix)]
#[tokio::test]
async fn update_is_refused_during_a_run_and_admission_during_an_update() {
    let script: Script = Arc::new(|_, events, cancel| {
        tokio::spawn(async move {
            cancel.cancelled().await;
            let _ = events
                .send(EngineEvent::Finished(EngineOutcome::Cancelled))
                .await;
        })
    });
    let f = fixture(script).await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    until_state(&f.core, &run.id, RunState::Running).await;
    let refused = f.core.engines.update("script", "t").await;
    assert!(matches!(refused, Err(CoreError::EngineBusy(_))));
    let audit = f.core.store.list_audit(None, 50).unwrap();
    assert!(audit.iter().all(|row| row.action != "engine.update"));
    f.core.runs.cancel(&run.id).unwrap();
    until_terminal(&f.core, &run.id).await;

    assert!(f.core.engines.update("script", "t").await.is_ok());
    let audit = f.core.store.list_audit(None, 50).unwrap();
    let positions: Vec<_> = audit
        .iter()
        .map(|row| row.action.as_str())
        .filter(|action| action.starts_with("engine.update"))
        .collect();
    assert_eq!(positions, ["engine.update.result", "engine.update"]);
}

#[cfg(unix)]
#[tokio::test]
async fn an_updater_that_changed_nothing_is_a_failed_update() {
    let f = fixture_with(finished(), "echo no change", Duration::from_secs(30)).await;
    match f.core.engines.update("script", "t").await {
        Err(CoreError::UpdateFailed(message)) => assert!(message.contains("1.0.0"), "{message}"),
        other => panic!("{:?}", other.map(|status| status.version)),
    }
}

#[cfg(unix)]
async fn until_warm(core: &Core, thread: &str, wanted: bool) {
    for _ in 0..2000 {
        if core.engines.has_warm(thread) == wanted {
            return;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    panic!("warm stayed {}", !wanted);
}

/// Warming is off by default; once on, an update start ends the waiting process so
/// it cannot hold the binary being replaced.
#[cfg(unix)]
#[tokio::test]
async fn an_update_ends_the_process_kept_for_the_next_turn() {
    let f = fixture_full(
        finished(),
        "echo updated",
        Duration::from_secs(30),
        None,
        capabilities(),
        true,
    )
    .await;
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!f.core.engines.has_warm(&f.thread), "off by default");

    f.core
        .set_preferences(
            None,
            &std::collections::BTreeMap::from([("warm_next_turn".into(), json!(true))]),
        )
        .unwrap();
    let run = f.core.runs.admit(&f.thread, "go", "t").unwrap();
    assert_eq!(until_terminal(&f.core, &run.id).await, RunState::Completed);
    until_warm(&f.core, &f.thread, true).await;
    assert!(f.core.engines.update("script", "t").await.is_ok());
    assert!(!f.core.engines.has_warm(&f.thread));
    // Nothing can be started while an update runs.
    assert!(f.core.engines.begin_warm().is_some());
}

/// The provider card's recommended-version action needs a signed recommendation.
#[cfg(unix)]
#[tokio::test]
async fn install_recommended_needs_a_signed_recommendation() {
    let f = fixture(finished()).await;
    let refused = f.core.engines.install_recommended("script", "t").await;
    assert!(
        matches!(refused, Err(CoreError::UpdateUnavailable(_))),
        "{refused:?}"
    );
    let audit = f.core.store.list_audit(None, 50).unwrap();
    assert!(
        audit
            .iter()
            .all(|row| !row.action.starts_with("engine.update"))
    );
}

#[cfg(unix)]
#[tokio::test]
async fn failed_update_keeps_its_output_and_clears_the_lock() {
    let f = fixture_with(
        finished(),
        "echo broken registry >&2; exit 3",
        Duration::from_secs(30),
    )
    .await;
    let failed = f.core.engines.update("script", "t").await;
    match failed {
        Err(CoreError::UpdateFailed(output)) => assert!(output.contains("broken registry")),
        other => panic!("{other:?}"),
    }
    assert!(f.core.engines.admit_with("script", || Ok(())).is_ok());
    let audit = f.core.store.list_audit(None, 10).unwrap();
    assert_eq!(audit[0].action, "engine.update.result");
    assert_eq!(audit[0].detail["ok"], json!(false));
}

#[cfg(unix)]
#[tokio::test]
async fn hung_update_is_stopped_at_its_timeout_with_its_children() {
    let dir = TempDir::new();
    let marker = dir.path().join("survived");
    let script = format!("(sleep 1; touch '{}') & wait", marker.display());
    let f = fixture_with(finished(), &script, Duration::from_millis(100)).await;
    let timed_out = f.core.engines.update("script", "t").await;
    match timed_out {
        Err(CoreError::UpdateFailed(output)) => assert!(output.contains("in time")),
        other => panic!("{other:?}"),
    }
    tokio::time::sleep(Duration::from_millis(1300)).await;
    assert!(!marker.exists(), "the update's child outlived its timeout");
}

#[cfg(unix)]
#[tokio::test]
async fn admission_is_refused_while_an_update_runs() {
    let f = fixture_with(finished(), "sleep 1", Duration::from_secs(30)).await;
    let engines = f.core.engines.clone();
    let updating = tokio::spawn(async move { engines.update("script", "t").await });
    for _ in 0..500 {
        let statuses = f.core.engines.statuses(false).await.unwrap();
        if statuses.iter().any(|status| status.updating) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(matches!(
        f.core.runs.admit(&f.thread, "go", "t"),
        Err(CoreError::EngineBusy(_))
    ));
    assert_eq!(f.core.store.latest_seq(&f.thread).unwrap(), 0);
    assert!(updating.await.unwrap().is_ok());
    assert!(f.core.runs.admit(&f.thread, "go", "t").is_ok());
}
