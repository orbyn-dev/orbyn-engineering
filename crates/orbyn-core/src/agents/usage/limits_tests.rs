//! Subscription reads use fake providers only; their account names are test labels.
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::*;
use crate::agents::driver::{EngineDescriptor, EngineDriver, EngineProbe, RunHandle, RunSpec};
use crate::app::Core;
use crate::testing::TempDir;

type ReadGate = (
    tokio::sync::oneshot::Sender<()>,
    tokio::sync::oneshot::Receiver<()>,
);
type LookupGate = (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>);

struct FakeLimits {
    id: &'static str,
    family: &'static str,
    failed: AtomicBool,
    /// An engine that answers the read without reporting a subscription.
    absent: AtomicBool,
    calls: AtomicUsize,
    report: Mutex<Value>,
    gate: Mutex<Option<ReadGate>>,
    lookup_gate: Mutex<Option<LookupGate>>,
}

impl FakeLimits {
    fn new(id: &'static str, family: &'static str) -> Arc<Self> {
        Arc::new(Self {
            id,
            family,
            failed: AtomicBool::new(false),
            absent: AtomicBool::new(false),
            calls: AtomicUsize::new(0),
            report: Mutex::new(serde_json::json!({"rateLimits": {"primary": {
                "usedPercent": 37.0, "windowDurationMins": 300
            }}})),
            gate: Mutex::new(None),
            lookup_gate: Mutex::new(None),
        })
    }
}

#[async_trait::async_trait]
impl EngineDriver for FakeLimits {
    fn descriptor(&self) -> EngineDescriptor {
        let gate = self.lookup_gate.lock().unwrap().take();
        if let Some((started, release)) = gate {
            started.send(()).unwrap();
            release
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
        }
        EngineDescriptor {
            id: self.id.into(),
            name: self.id.into(),
            tier: orbyn_contracts::EngineTier::Native,
            capabilities: serde_json::from_value(serde_json::json!({
                "streaming": false, "approvals": false, "resume": false, "mcp": false,
                "sandbox": false, "managed": false, "skill_delivery": [], "goal_control": false,
                "decline_feedback": false, "permission_modes": []
            }))
            .unwrap(),
        }
    }
    fn family(&self) -> String {
        self.family.into()
    }
    async fn probe(&self) -> EngineProbe {
        assert_ne!(
            self.family, "claude",
            "the overview must not probe Claude or its instances"
        );
        EngineProbe {
            path: Some("fake-provider".into()),
            ..EngineProbe::default()
        }
    }
    async fn start_run(
        &self,
        _: RunSpec,
        _: tokio::sync::mpsc::Sender<crate::agents::driver::EngineEvent>,
    ) -> Result<RunHandle> {
        Err(CoreError::Unsupported(
            "The fixture never runs a model.".into(),
        ))
    }
    async fn limits(&self) -> Result<Option<Value>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let failed = self.failed.load(Ordering::SeqCst);
        let report = self.report.lock().unwrap().clone();
        let gate = self.gate.lock().unwrap().take();
        if let Some((started, release)) = gate {
            let _ = started.send(());
            release.await.unwrap();
        }
        if failed {
            Err(CoreError::EngineUnavailable(
                "The fake account cannot be read.".into(),
            ))
        } else if self.absent.load(Ordering::SeqCst) {
            Ok(None)
        } else {
            Ok(Some(report))
        }
    }
}

fn core(dir: &TempDir) -> Core {
    let mut config = crate::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
    config.engines.lookup_latest = false;
    config.engines.known_engine_ids = Some(Vec::new());
    config.engines.claude_program = dir.path().join("no-claude").to_string_lossy().into_owned();
    config.engines.codex_program = dir.path().join("no-codex").to_string_lossy().into_owned();
    Core::open(config).unwrap()
}

/// A scheduled-task fixture must ask a provider, rather than inject an unbound cache.
pub(crate) fn register_report(core: &Core, id: &'static str, report: Value) {
    let driver = FakeLimits::new(id, "codex");
    *driver.report.lock().unwrap() = report;
    core.engines.register(driver);
}

#[tokio::test]
async fn account_switch_or_signout_and_failed_read_never_reuses_an_old_window() {
    for force in [false, true] {
        let dir = TempDir::new();
        let core = core(&dir);
        let driver = FakeLimits::new("account-fixture", "codex");
        core.engines.register(driver.clone());
        let old = core.engine_limits(driver.id, false).await.unwrap();
        assert_eq!(old.windows[0].used_percent, 37.0);
        // A different account or signed-out CLI can fail identically. Neither proves
        // the old account is current, even if a cached status still says logged in.
        driver.failed.store(true, Ordering::SeqCst);
        assert!(core.engine_limits(driver.id, force).await.is_err());
        assert!(core.cached_limits(driver.id).is_none());
        assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn missing_identity_and_repeated_success_always_read_the_current_provider() {
    let dir = TempDir::new();
    let core = core(&dir);
    let driver = FakeLimits::new("old-protocol", "codex");
    core.engines.register(driver.clone());
    assert_eq!(
        core.engine_limits(driver.id, false).await.unwrap().windows[0].used_percent,
        37.0
    );
    driver.report.lock().unwrap()["rateLimits"]["primary"]["usedPercent"] = 81.0.into();
    let current = core.engine_limits(driver.id, false).await.unwrap();
    assert_eq!(current.windows[0].used_percent, 81.0);
    assert!(!current.checked_at.is_empty());
    assert!(core.cached_limits(driver.id).is_none());
}

#[tokio::test]
async fn an_overlapping_read_still_answers_and_a_forgotten_one_does_not() {
    let dir = TempDir::new();
    let core = core(&dir);
    let driver = FakeLimits::new("account-fixture", "codex");
    core.engines.register(driver.clone());
    let (started, wait_started) = tokio::sync::oneshot::channel();
    let (release, wait_release) = tokio::sync::oneshot::channel();
    *driver.gate.lock().unwrap() = Some((started, wait_release));
    let first = core.engine_limits(driver.id, false);
    let overlap = async {
        wait_started.await.unwrap();
        assert!(core.engine_limits(driver.id, true).await.is_ok());
        release.send(()).unwrap();
    };
    let (first, ()) = tokio::join!(first, overlap);
    assert!(
        first.is_ok(),
        "a newer overlapping read must not fail this one"
    );

    let (started, wait_started) = tokio::sync::oneshot::channel();
    let (release, wait_release) = tokio::sync::oneshot::channel();
    *driver.gate.lock().unwrap() = Some((started, wait_release));
    let old = core.engine_limits(driver.id, false);
    let switch = async {
        wait_started.await.unwrap();
        core.usage.forget();
        release.send(()).unwrap();
    };
    let (old, ()) = tokio::join!(old, switch);
    assert!(
        old.is_err(),
        "a read in flight across forget must not return the prior account's windows"
    );
    assert!(core.cached_limits(driver.id).is_none());
}

#[tokio::test]
async fn forget_and_configuration_changes_fence_reads_already_in_progress() {
    for configuration_change in [false, true] {
        let dir = TempDir::new();
        let core = core(&dir);
        let driver = FakeLimits::new("account-fixture", "codex");
        core.engines.register(driver.clone());
        let (started, wait_started) = tokio::sync::oneshot::channel();
        let (release, wait_release) = tokio::sync::oneshot::channel();
        *driver.gate.lock().unwrap() = Some((started, wait_release));
        let old = core.engine_limits(driver.id, false);
        let invalidate = async {
            wait_started.await.unwrap();
            if configuration_change {
                core.engines.invalidate().await;
            } else {
                core.usage.forget();
            }
            release.send(()).unwrap();
        };
        let (old, ()) = tokio::join!(old, invalidate);
        assert!(old.is_err());
        assert!(core.engine_limits(driver.id, false).await.is_ok());
    }
}

#[tokio::test]
async fn configuration_change_during_driver_lookup_cannot_pair_old_driver_with_new_epoch() {
    let dir = TempDir::new();
    let core = core(&dir);
    let driver = FakeLimits::new("account-fixture", "codex");
    let (started, wait_started) = std::sync::mpsc::channel();
    let (release, wait_release) = std::sync::mpsc::channel();
    *driver.lookup_gate.lock().unwrap() = Some((started, wait_release));
    core.engines.register(driver.clone());
    let engines = core.engines.clone();
    let runtime = tokio::runtime::Handle::current();
    // The existing descriptor call is the lookup seam; no production hook is needed.
    // Invalidation does not acquire the registry lock the lookup currently holds.
    let changed = std::thread::spawn(move || {
        wait_started
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        runtime.block_on(engines.invalidate());
        release.send(()).unwrap();
    });
    let result = core.engine_limits(driver.id, false).await;
    changed.join().unwrap();
    assert!(
        result.is_err(),
        "an epoch captured after lookup would accept this old driver"
    );
    assert!(core.engine_limits(driver.id, false).await.is_ok());
}

#[tokio::test]
async fn concurrent_engines_do_not_supersede_each_others_reads() {
    let dir = TempDir::new();
    let core = core(&dir);
    let first = FakeLimits::new("first-account", "codex");
    let second = FakeLimits::new("second-account", "codex");
    core.engines.register(first.clone());
    core.engines.register(second.clone());
    let (started, wait_started) = tokio::sync::oneshot::channel();
    let (release, wait_release) = tokio::sync::oneshot::channel();
    *first.gate.lock().unwrap() = Some((started, wait_release));
    let old = core.engine_limits(first.id, false);
    let other = async {
        wait_started.await.unwrap();
        assert!(core.engine_limits(second.id, false).await.is_ok());
        release.send(()).unwrap();
    };
    let (old, ()) = tokio::join!(old, other);
    assert!(old.is_ok());
}

#[tokio::test]
async fn cancelling_a_read_does_not_leave_a_shared_flight_or_held_entry() {
    let dir = TempDir::new();
    let core = core(&dir);
    let driver = FakeLimits::new("account-fixture", "codex");
    core.engines.register(driver.clone());
    let (started, wait_started) = tokio::sync::oneshot::channel();
    let (_release, wait_release) = tokio::sync::oneshot::channel();
    *driver.gate.lock().unwrap() = Some((started, wait_release));
    let mut read = Box::pin(core.engine_limits(driver.id, false));
    tokio::select! {
        _ = wait_started => {}
        _ = &mut read => panic!("the fixture must wait for cancellation"),
    }
    drop(read);
    assert!(core.engine_limits(driver.id, false).await.is_ok());
    assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn overview_skips_claude_family_before_probe_and_reports_other_failures() {
    let dir = TempDir::new();
    let core = core(&dir);
    let claude = FakeLimits::new("claude-work", "claude");
    let codex = FakeLimits::new("codex-work", "codex");
    core.engines.register(claude.clone());
    core.engines.register(codex.clone());
    let first = core.limits_overview(false).await.unwrap();
    assert_eq!(first.engines.len(), 1);
    assert_eq!(first.engines[0].engine, codex.id);
    assert_eq!(claude.calls.load(Ordering::SeqCst), 0);
    codex.failed.store(true, Ordering::SeqCst);
    let failed = core.limits_overview(false).await.unwrap();
    assert!(failed.engines.is_empty());
    assert!(failed.pooled.is_empty());
    assert!(failed.unavailable.iter().any(|one| one.engine == codex.id));
    // An explicit Claude read remains supported and is never silently skipped.
    assert!(core.engine_limits(claude.id, false).await.is_ok());
    assert_eq!(claude.calls.load(Ordering::SeqCst), 1);
}

/// A person's refresh asks the engine again; it is never answered from a kept
/// report, because no report can be proved to belong to the current account.
#[tokio::test]
async fn a_forced_read_asks_the_provider_and_keeps_nothing() {
    let dir = TempDir::new();
    let core = core(&dir);
    let driver = FakeLimits::new("account-fixture", "codex");
    core.engines.register(driver.clone());
    let first = core.engine_limits(driver.id, true).await.unwrap();
    let second = core.engine_limits(driver.id, true).await.unwrap();
    assert_eq!(
        driver.calls.load(Ordering::SeqCst),
        2,
        "a forced read must not be answered from memory"
    );
    assert_eq!(first.windows[0].used_percent, 37.0);
    assert_eq!(second.windows[0].used_percent, 37.0);
    // Nothing is kept for any reader, including one that forces.
    assert!(core.cached_limits(driver.id).is_none());
}

/// Force skips neither fence: an account change or sign-out during the read
/// still refuses it, so a refresh cannot put another account's figures back.
#[tokio::test]
async fn a_forced_read_is_fenced_like_an_automatic_one() {
    for configuration_change in [false, true] {
        let dir = TempDir::new();
        let core = core(&dir);
        let driver = FakeLimits::new("account-fixture", "codex");
        core.engines.register(driver.clone());
        let (started, wait_started) = tokio::sync::oneshot::channel();
        let (release, wait_release) = tokio::sync::oneshot::channel();
        *driver.gate.lock().unwrap() = Some((started, wait_release));
        let forced = core.engine_limits(driver.id, true);
        let invalidate = async {
            wait_started.await.unwrap();
            if configuration_change {
                core.engines.invalidate().await;
            } else {
                core.usage.forget();
            }
            release.send(()).unwrap();
        };
        let (forced, ()) = tokio::join!(forced, invalidate);
        assert!(
            forced.is_err(),
            "a forced read must not survive an account change or sign-out"
        );
        assert!(core.cached_limits(driver.id).is_none());
        let after = core.engine_limits(driver.id, true).await.unwrap();
        assert_eq!(after.windows[0].used_percent, 37.0);
        assert_eq!(driver.calls.load(Ordering::SeqCst), 2);
    }
}

/// Force reads once more; it does not invent a subscription for an engine that
/// reports none, and it does not start rather than reuse the engine's own read.
#[tokio::test]
async fn a_forced_read_refuses_an_engine_that_reports_no_subscription() {
    let dir = TempDir::new();
    let core = core(&dir);
    let driver = FakeLimits::new("quiet-provider", "codex");
    driver.absent.store(true, Ordering::SeqCst);
    core.engines.register(driver.clone());
    assert!(matches!(
        core.engine_limits(driver.id, true).await,
        Err(CoreError::Unsupported(_))
    ));
    assert_eq!(driver.calls.load(Ordering::SeqCst), 1);
}

/// A forced overview still filters the Claude family before any status or usage
/// probe: a person's refresh is not a way to start a provider the overview must
/// never start. It also reports failures rather than masking them.
#[tokio::test]
async fn a_forced_overview_still_never_starts_claude() {
    let dir = TempDir::new();
    let core = core(&dir);
    let claude = FakeLimits::new("claude-work", "claude");
    let codex = FakeLimits::new("codex-work", "codex");
    core.engines.register(claude.clone());
    core.engines.register(codex.clone());
    let overview = core.limits_overview(true).await.unwrap();
    assert_eq!(overview.engines.len(), 1);
    // FakeLimits::probe panics for the Claude family, so this also proves no probe ran.
    assert_eq!(claude.calls.load(Ordering::SeqCst), 0);
    assert_eq!(codex.calls.load(Ordering::SeqCst), 1);
    codex.failed.store(true, Ordering::SeqCst);
    let failed = core.limits_overview(true).await.unwrap();
    assert!(failed.engines.is_empty());
    assert!(failed.pooled.is_empty());
    assert!(failed.unavailable.iter().any(|one| one.engine == codex.id));
    assert_eq!(codex.calls.load(Ordering::SeqCst), 2);
}
