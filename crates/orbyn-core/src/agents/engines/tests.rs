#[test]
fn update_all_takes_only_an_owned_newer_engine_that_is_not_an_instance() {
    let base = serde_json::json!({
        "id": "codex", "name": "Codex", "tier": "native",
        "capabilities": {"streaming": true, "approvals": true, "resume": true, "mcp": true,
            "sandbox": true, "managed": true, "skill_delivery": [], "goal_control": false,
            "decline_feedback": false, "permission_modes": []},
        "installed": true, "path": "/bin/codex", "version": "0.166.0", "auth": "unknown",
        "installer": "npm", "latest": "0.167.0", "support": "unknown",
        "update_command": "npm install -g @openai/codex@latest", "manual_hint": null,
        "sign_in_command": null, "updating": false, "problem": null
    });
    let status = |change: serde_json::Value| -> EngineStatus {
        let mut value = base.clone();
        value
            .as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        serde_json::from_value(value).unwrap()
    };
    assert!(wants_update(&status(serde_json::json!({}))));
    assert!(!wants_update(&status(
        serde_json::json!({"latest": "0.166.0"})
    )));
    assert!(!wants_update(&status(serde_json::json!({"latest": null}))));
    assert!(!wants_update(&status(
        serde_json::json!({"update_command": null})
    )));
    assert!(!wants_update(&status(
        serde_json::json!({"instance_of": "codex"})
    )));
}

use super::*;
use crate::store::{Admission, NewProject, NewThread};
use crate::testing::TempDir;

fn manager(dir: &TempDir) -> EngineManager {
    EngineManager::new(
        Arc::new(Store::open(&dir.path().join("store.sqlite3"), "test").unwrap()),
        Arc::new(TerminalManager::default()),
        reqwest::Client::new(),
        EngineConfig::default(),
    )
}

#[tokio::test]
async fn native_operations_exclude_command_changes_and_same_instance_admission() {
    let dir = TempDir::new();
    let engines = manager(&dir);
    let operation = engines.begin_native_operation("work").unwrap();
    assert!(matches!(
        engines.begin_native_operation("work"),
        Err(CoreError::EngineBusy(_))
    ));
    let mut changed = false;
    assert!(matches!(
        engines.change_provider_configuration(|| {
            changed = true;
            Ok(())
        }),
        Err(CoreError::EngineBusy(_))
    ));
    assert!(!changed);
    assert!(matches!(
        engines.admit_with("work", || Ok(())),
        Err(CoreError::EngineBusy(_))
    ));
    assert!(engines.admit_with("other", || Ok(())).is_ok());
    assert!(matches!(
        engines.begin_update("claude"),
        Err(CoreError::EngineBusy(_))
    ));
    drop(operation);
    engines
        .change_provider_configuration(|| {
            changed = true;
            Ok(())
        })
        .unwrap();
    assert!(changed);
    assert!(engines.admit_with("work", || Ok(())).is_ok());
    let update = engines.begin_update("claude").unwrap();
    assert!(matches!(
        engines.begin_native_operation("work"),
        Err(CoreError::EngineBusy(_))
    ));
    drop(update);
    assert!(engines.begin_native_operation("work").is_ok());
}

#[tokio::test]
async fn an_instance_run_blocks_updates_of_the_shared_engine() {
    let dir = TempDir::new();
    let engines = manager(&dir);
    let project = engines
        .store
        .add_project(NewProject {
            path: "/project",
            name: "project",
            is_git: false,
            managed: false,
            scratch: false,
        })
        .unwrap();
    engines.store.trust_project(&project.id, "test").unwrap();
    let thread = engines
        .store
        .create_thread(NewThread {
            project_id: project.id,
            title: "test".into(),
            engine: "claude-beta".into(),
            model: None,
            cwd: "/project".into(),
            worktree: None,
            skill_profile: None,
            parent_thread_id: None,
            relation: None,
            scratch: false,
        })
        .unwrap();
    let run = engines
        .admit_with("claude-beta", || {
            engines.store.admit_run(Admission {
                thread_id: &thread.id,
                prompt: "test",
                actor: "test",
            })
        })
        .unwrap();
    assert!(matches!(
        engines.begin_update("claude"),
        Err(CoreError::EngineBusy(_))
    ));
    engines
        .store
        .finish_run(&run.id, orbyn_contracts::RunState::Completed, None)
        .unwrap();
    assert!(engines.begin_update("claude").is_ok());
}

#[tokio::test]
async fn an_engine_update_blocks_admission_through_an_instance() {
    let dir = TempDir::new();
    let engines = manager(&dir);
    let marker = engines.begin_update("claude").unwrap();
    assert!(matches!(
        engines.admit_with("claude-beta", || Ok(())),
        Err(CoreError::EngineBusy(_))
    ));
    drop(marker);
    assert!(engines.admit_with("claude-beta", || Ok(())).is_ok());
}

struct KeyedDriver {
    key: Option<String>,
    failing: bool,
    empty: bool,
}

#[async_trait::async_trait]
impl EngineDriver for KeyedDriver {
    fn descriptor(&self) -> crate::agents::driver::EngineDescriptor {
        crate::agents::driver::EngineDescriptor {
            id: "keyed".into(),
            name: "Keyed".into(),
            tier: orbyn_contracts::EngineTier::Terminal,
            capabilities: orbyn_contracts::EngineCapabilities {
                streaming: false,
                approvals: false,
                resume: false,
                mcp: false,
                sandbox: false,
                managed: true,
                skill_delivery: Vec::new(),
                goal_control: false,
                decline_feedback: false,
                permission_modes: orbyn_contracts::PermissionMode::ALL.to_vec(),
            },
        }
    }

    async fn probe(&self) -> crate::agents::driver::EngineProbe {
        crate::agents::driver::EngineProbe::default()
    }

    async fn start_run(
        &self,
        _spec: crate::agents::driver::RunSpec,
        _events: tokio::sync::mpsc::Sender<crate::agents::driver::EngineEvent>,
    ) -> Result<crate::agents::driver::RunHandle> {
        Err(CoreError::Unsupported("no run".into()))
    }

    async fn models(&self) -> Result<Option<ModelCatalog>> {
        if self.failing {
            return Err(CoreError::EngineUnavailable("the engine is gone".into()));
        }
        if self.empty {
            return Ok(Some(ModelCatalog::engine(Vec::new())));
        }
        Ok(Some(ModelCatalog::engine(vec![
            orbyn_contracts::ModelInfo {
                id: "keyed-model".into(),
                label: "Keyed Model".into(),
                description: None,
                is_default: true,
                custom: false,
                options: Vec::new(),
                input_modalities: Vec::new(),
                upgrade: None,
            },
        ])))
    }

    async fn catalog_key(&self) -> Option<String> {
        self.key.clone()
    }
}

fn keyed_manager(dir: &TempDir, key: Option<&str>, failing: bool, empty: bool) -> EngineManager {
    let config = EngineConfig {
        cache_dir: Some(dir.path().join("model-cache")),
        ..EngineConfig::default()
    };
    let engines = EngineManager::new(
        Arc::new(Store::open(&dir.path().join("store.sqlite3"), "test").unwrap()),
        Arc::new(TerminalManager::default()),
        reqwest::Client::new(),
        config,
    );
    engines.register(Arc::new(KeyedDriver {
        key: key.map(str::to_owned),
        failing,
        empty,
    }));
    engines
}

/// A restart has no memory, so the disk copy answers only for its own key.
#[tokio::test]
async fn a_stored_catalog_is_served_only_under_its_own_key() {
    let dir = TempDir::new();
    let good = keyed_manager(&dir, Some("one"), false, false)
        .models("keyed", true)
        .await
        .unwrap();
    assert!(good.error.is_none());

    let changed = keyed_manager(&dir, Some("two"), true, false)
        .models("keyed", true)
        .await
        .unwrap();
    assert!(changed.models.is_empty(), "{changed:?}");
    assert!(changed.error.is_some());

    let remembered = keyed_manager(&dir, Some("one"), true, false)
        .models("keyed", true)
        .await
        .unwrap();
    assert_eq!(remembered.models, good.models);
    assert_eq!(remembered.checked_at, good.checked_at);
    assert!(remembered.error.unwrap().contains("engine is gone"));
}

/// A failing first read leaves nothing behind for the next start to trust.
#[tokio::test]
async fn a_failure_is_not_saved() {
    let dir = TempDir::new();
    let failed = keyed_manager(&dir, None, true, false)
        .models("keyed", true)
        .await
        .unwrap();
    assert!(failed.models.is_empty());
    assert!(!dir.path().join("model-cache/catalog.json").exists());
}

/// A good answer with no models means the engine has none, so a remembered
/// list must not stand in for it and look freshly reported.
#[tokio::test]
async fn an_empty_answer_is_not_replaced_by_the_stored_catalog() {
    let dir = TempDir::new();
    let good = keyed_manager(&dir, Some("one"), false, false)
        .models("keyed", true)
        .await
        .unwrap();
    assert!(!good.models.is_empty());

    let empty = keyed_manager(&dir, Some("one"), false, true)
        .models("keyed", true)
        .await
        .unwrap();
    assert!(empty.models.is_empty(), "{empty:?}");
    assert!(empty.error.is_none(), "{empty:?}");

    let stored = std::fs::read_to_string(dir.path().join("model-cache/catalog.json")).unwrap();
    assert!(stored.contains("keyed-model"));
}

/// Only the signed manifest raises a warning, and only its own installer may
/// install the version it recommends.
#[test]
fn compatibility_comes_from_the_signed_manifest_alone() {
    let manifest = TestedManifest::embedded();
    let plan = UpdatePlan {
        installer: InstallerKind::Npm,
        program: "npm".into(),
        args: ["install", "-g", "@openai/codex@latest"]
            .iter()
            .map(|arg| (*arg).to_string())
            .collect(),
        lock_key: "npm".into(),
    };
    let below = compatibility(&manifest, "codex", "0.160.0", Some(&plan)).unwrap();
    assert_eq!(
        below.status,
        orbyn_contracts::CompatibilityStatus::Unsupported
    );
    assert_eq!(below.recommended_version.as_deref(), Some("0.166.0"));
    assert_eq!(
        below.recommended_command.as_deref(),
        Some("npm install -g @openai/codex@0.166.0")
    );
    assert_eq!(
        below.message,
        "This engine version is outside the supported range for this Orbyn release. Use 0.166.0."
    );
    // Without an installer that can pin the version, the card keeps its manual route.
    let manual = compatibility(&manifest, "codex", "0.160.0", None).unwrap();
    assert_eq!(manual.recommended_command, None);
    assert_eq!(manual.message, below.message);
    // Supported, untested and unknown versions raise nothing at all.
    assert!(compatibility(&manifest, "codex", "0.166.0", Some(&plan)).is_none());
    assert!(compatibility(&manifest, "claude", "2.1.318", Some(&plan)).is_none());
    assert!(compatibility(&manifest, "other", "1.0.0", Some(&plan)).is_none());
}

/// An installer that prints, waits to be told to go on, then writes the new
/// version. Arguments: the version file, the go-on file.
const INSTALLER: &str = r#"
const fs = require('fs');
const [version, go] = process.argv.slice(1);
process.stdout.write('fetching\n');
process.stderr.write('\x1b[32mhalf\x1b[0m\rlinked\n');
const wait = setInterval(() => {
  if (!fs.existsSync(go)) return;
  clearInterval(wait);
  fs.writeFileSync(version, '2.0.0');
  process.stdout.write('added 1 package');
}, 20);
"#;

struct PlannedDriver {
    version: PathBuf,
    go: PathBuf,
}

#[async_trait::async_trait]
impl EngineDriver for PlannedDriver {
    fn descriptor(&self) -> crate::agents::driver::EngineDescriptor {
        let mut descriptor = KeyedDriver {
            key: None,
            failing: false,
            empty: false,
        }
        .descriptor();
        descriptor.id = "planned".into();
        descriptor
    }

    async fn probe(&self) -> crate::agents::driver::EngineProbe {
        crate::agents::driver::EngineProbe {
            path: Some(self.version.clone()),
            version: std::fs::read_to_string(&self.version).ok(),
            ..Default::default()
        }
    }

    async fn update_plan(&self, _probe: &crate::agents::driver::EngineProbe) -> Option<UpdatePlan> {
        Some(UpdatePlan {
            installer: InstallerKind::Npm,
            program: "node".into(),
            args: vec![
                "-e".into(),
                INSTALLER.into(),
                self.version.to_string_lossy().into_owned(),
                self.go.to_string_lossy().into_owned(),
            ],
            lock_key: "planned".into(),
        })
    }

    async fn start_run(
        &self,
        _spec: crate::agents::driver::RunSpec,
        _events: tokio::sync::mpsc::Sender<crate::agents::driver::EngineEvent>,
    ) -> Result<crate::agents::driver::RunHandle> {
        Err(CoreError::Unsupported("no run".into()))
    }
}

async fn next(
    events: &mut tokio::sync::broadcast::Receiver<EngineUpdateProgress>,
) -> EngineUpdateProgress {
    tokio::time::timeout(Duration::from_secs(20), events.recv())
        .await
        .expect("an update step in time")
        .unwrap()
}

#[tokio::test]
async fn an_update_reports_each_installer_line_while_it_runs() {
    let dir = TempDir::new();
    let engines = Arc::new(manager(&dir));
    let version = dir.path().join("version");
    let go = dir.path().join("go");
    std::fs::write(&version, "1.0.0").unwrap();
    engines.register(Arc::new(PlannedDriver {
        version: version.clone(),
        go: go.clone(),
    }));
    let mut events = engines.update_events();
    let update = tokio::spawn({
        let engines = engines.clone();
        async move { engines.update("planned", "test").await }
    });

    let EngineUpdateProgress::Started { engine, command } = next(&mut events).await else {
        panic!("the update did not start first");
    };
    assert_eq!(engine, "planned");
    assert!(command.starts_with("node -e"));
    let mut lines = Vec::new();
    while lines.len() < 3 {
        match next(&mut events).await {
            EngineUpdateProgress::Output { engine, line } => {
                assert_eq!(engine, "planned");
                lines.push(line);
            }
            other => panic!("expected a line, got {other:?}"),
        }
    }
    // Stdout and stderr arrive in either order; each keeps its own.
    lines.sort();
    assert_eq!(lines, ["fetching", "half", "linked"]);
    // The installer is still waiting, so the lines came while it ran.
    assert!(!update.is_finished());

    std::fs::write(&go, "").unwrap();
    assert_eq!(
        next(&mut events).await,
        EngineUpdateProgress::Output {
            engine: "planned".into(),
            line: "added 1 package".into(),
        }
    );
    assert_eq!(
        next(&mut events).await,
        EngineUpdateProgress::Finished {
            engine: "planned".into(),
            version: Some("2.0.0".into()),
            error: None,
        }
    );
    assert_eq!(
        update.await.unwrap().unwrap().version.as_deref(),
        Some("2.0.0")
    );
}

#[tokio::test]
async fn an_update_that_changes_nothing_finishes_with_its_error() {
    let dir = TempDir::new();
    let engines = manager(&dir);
    let version = dir.path().join("version");
    std::fs::write(&version, "2.0.0").unwrap();
    engines.register(Arc::new(PlannedDriver {
        version,
        go: dir.path().join("go"),
    }));
    std::fs::write(dir.path().join("go"), "").unwrap();
    let mut events = engines.update_events();
    let error = engines.update("planned", "test").await.unwrap_err();
    let mut last = None;
    while let Ok(event) = events.try_recv() {
        last = Some(event);
    }
    assert_eq!(
        last,
        Some(EngineUpdateProgress::Finished {
            engine: "planned".into(),
            version: None,
            error: Some(error.to_string()),
        })
    );
}

#[tokio::test]
async fn a_dropped_update_still_finishes_for_its_listeners() {
    let dir = TempDir::new();
    let engines = Arc::new(manager(&dir));
    let version = dir.path().join("version");
    std::fs::write(&version, "1.0.0").unwrap();
    engines.register(Arc::new(PlannedDriver {
        version,
        go: dir.path().join("go"),
    }));
    let mut events = engines.update_events();
    let update = tokio::spawn({
        let engines = engines.clone();
        async move { engines.update("planned", "test").await }
    });
    assert!(matches!(
        next(&mut events).await,
        EngineUpdateProgress::Started { .. }
    ));
    update.abort();
    loop {
        if let EngineUpdateProgress::Finished { version, error, .. } = next(&mut events).await {
            assert_eq!(version, None);
            assert!(error.unwrap().contains("stopped"));
            break;
        }
    }
}

#[test]
fn a_progress_line_drops_colour_codes_and_control_characters() {
    assert_eq!(
        strip_escapes("\u{1b}[1;31merror\u{1b}[0m\u{7} done"),
        "error done"
    );
    assert_eq!(
        strip_escapes("\u{1b}]0;npm install\u{7}\u{1b}(Badded\u{1b}]8;;x\u{1b}\\ 1"),
        "added 1"
    );
}
