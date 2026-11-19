//! One view of what can be updated: the skills library, the app and the engines.
//! Checks only read, and nothing here applies on its own. A failed check keeps
//! what was known, since the last good answer beats no answer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use orbyn_contracts::{
    AppMode, AppUpdatePhase, AppUpdateRequest, AppUpdateStatus, ApplyAllResult, EngineStatus,
    EngineUpdateStatus, LibraryUpdateStatus, UpdateStatus, UpdateStep,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::agents::engines::EngineManager;
use crate::agents::engines::manifest::is_newer;
use crate::error::{CoreError, Result};
use crate::project::workspace::Workspace;
use crate::store::{AuditRecord, Store};
use crate::util::now;

/// How long nobody must have done anything before an automatic install.
const IDLE_BEFORE_INSTALL: Duration = Duration::from_secs(2 * 60);
const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(8);
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// How long `GET /api/updates` is answered without a refresh. A status reads Git and
/// the engines, which the poll behind it asks for every few seconds.
const STATUS_FRESH: Duration = Duration::from_secs(20);
/// Past this, an earlier status is no longer shown without waiting for a new one. A window
/// reads again on `data_changed`, but it may have missed one, so this stays short.
const STATUS_MAX_AGE: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
enum AppIntent {
    Download,
    Install,
    Complete,
}

#[derive(Default)]
struct Known {
    app_latest: Option<String>,
    app_notes: Option<String>,
    release_id: Option<String>,
    phase: AppUpdatePhase,
    operation: u32,
    operation_release: Option<String>,
    operation_version: Option<String>,
    operation_notes: Option<String>,
    operation_actor: String,
    install_after_download: bool,
    update_error: Option<String>,
    failed_phase: Option<AppUpdatePhase>,
    failed_version: Option<String>,
    unconfigured: bool,
    downloaded: Option<u64>,
    total: Option<u64>,
    /// The latest version each engine last reported, kept across failed lookups.
    engine_latest: HashMap<String, String>,
    checked_at: Option<String>,
    error: Option<String>,
}

/// Follows every source of a change to the update status.
pub struct UpdateWatch {
    updates: watch::Receiver<u64>,
    engines: watch::Receiver<u64>,
    library: watch::Receiver<u64>,
    runs: watch::Receiver<u64>,
}

impl UpdateWatch {
    /// Waits for the next change; false once the service is shutting down.
    pub async fn changed(&mut self) -> bool {
        tokio::select! {
            changed = self.updates.changed() => changed.is_ok(),
            changed = self.engines.changed() => changed.is_ok(),
            changed = self.library.changed() => changed.is_ok(),
            changed = self.runs.changed() => changed.is_ok(),
        }
    }
}

pub struct UpdateCenter {
    store: Arc<Store>,
    workspace: Arc<Workspace>,
    engines: Arc<EngineManager>,
    http: reqwest::Client,
    manifest_url: Option<String>,
    known: Mutex<Known>,
    /// Lines for the desktop shell. Present exactly when one is attached.
    shell: Mutex<Option<mpsc::UnboundedSender<Value>>>,
    changes: watch::Sender<u64>,
    /// When someone last did something through a client.
    last_activity: Mutex<Instant>,
    /// The last status, for the polled read; dropped whenever `changed` fires.
    status_reads: crate::util::swr::Swr<UpdateStatus>,
}

#[derive(Deserialize)]
struct ReleaseManifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
}

impl UpdateCenter {
    pub fn new(
        store: Arc<Store>,
        workspace: Arc<Workspace>,
        engines: Arc<EngineManager>,
        http: reqwest::Client,
        manifest_url: Option<String>,
    ) -> Self {
        Self {
            store,
            workspace,
            engines,
            http,
            manifest_url,
            known: Mutex::new(Known::default()),
            shell: Mutex::new(None),
            changes: watch::channel(0).0,
            last_activity: Mutex::new(Instant::now()),
            status_reads: crate::util::swr::Swr::new(STATUS_FRESH, STATUS_MAX_AGE, 4),
        }
    }

    pub fn set_notify(&self, notify: &crate::util::swr::Notify) {
        self.status_reads.set_notify(notify.clone());
    }

    /// Marks a person as present, which puts off an automatic install.
    pub fn note_activity(&self) {
        *lock(&self.last_activity) = Instant::now();
    }

    fn idle_for(&self) -> Duration {
        lock(&self.last_activity).elapsed()
    }

    /// Installs an app update only with `automatic` chosen, nothing running and nobody
    /// at the app for two minutes. Engines and the library always wait to be asked.
    pub async fn auto_install(&self, mode: &str) {
        self.auto_install_after(mode, IDLE_BEFORE_INSTALL).await;
    }

    async fn auto_install_after(&self, mode: &str, idle: Duration) -> bool {
        if mode != "automatic" || self.idle_for() < idle {
            return false;
        }
        let app = self.status().await.app;
        if !app.can_apply {
            return false;
        }
        match self.apply_reported_app("system:auto-update", app) {
            Ok(()) => true,
            Err(error) => {
                tracing::debug!(%error, "an automatic update did not start");
                false
            }
        }
    }

    /// Connects the desktop shell. The receiver yields lines for its stdout.
    pub fn attach_shell(&self) -> mpsc::UnboundedReceiver<Value> {
        let (sender, receiver) = mpsc::unbounded_channel();
        *lock(&self.shell) = Some(sender);
        receiver
    }

    /// The way to the shell's stdout, for what else needs to reach it.
    pub fn shell_lines(&self) -> Option<mpsc::UnboundedSender<Value>> {
        lock(&self.shell).clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    /// Wakes on anything that changes the status: this centre's own changes, an engine
    /// update or a changed engine set, and a change to the library.
    pub fn watch(&self) -> UpdateWatch {
        UpdateWatch {
            updates: self.changes.subscribe(),
            engines: self.engines.subscribe_changes(),
            library: self.workspace.subscribe_library(),
            runs: self.store.subscribe_run_changes(),
        }
    }

    fn changed(&self) {
        self.status_reads.invalidate_all();
        self.status_reads.announce("updates");
        self.changes.send_modify(|version| *version += 1);
    }

    fn mode(&self) -> AppMode {
        if lock(&self.shell).is_some() {
            AppMode::Desktop
        } else {
            AppMode::Server
        }
    }

    fn to_shell(&self, line: Value) -> bool {
        lock(&self.shell)
            .as_ref()
            .is_some_and(|sender| sender.send(line).is_ok())
    }

    /// Reads one line from the shell. Anything unrecognized is ignored, so the
    /// two sides can be upgraded apart from each other.
    pub fn on_shell_line(&self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            tracing::debug!("ignoring a malformed shell line");
            return;
        };
        let text = |key: &str| message.get(key).and_then(Value::as_str).map(str::to_owned);
        {
            let mut known = lock(&self.known);
            match message.get("type").and_then(Value::as_str) {
                Some("app_update_available") => {
                    let Some(release_id) = text("release_id") else {
                        return;
                    };
                    let changed = known
                        .release_id
                        .as_ref()
                        .is_some_and(|old| old != &release_id);
                    known.release_id = Some(release_id);
                    known.app_latest = text("version");
                    known.app_notes = text("notes");
                    if changed
                        && (known.phase != AppUpdatePhase::Installing
                            || message.get("settled").and_then(Value::as_bool) == Some(true))
                    {
                        self.revoke_app_consent(&mut known);
                    }
                    if matches!(
                        known.phase,
                        AppUpdatePhase::Idle | AppUpdatePhase::Available
                    ) {
                        known.phase = AppUpdatePhase::Available;
                    }
                    known.unconfigured = false;
                }
                Some("app_update_none") => {
                    if let Some(operation) = message.get("operation").and_then(Value::as_u64) {
                        if operation != u64::from(known.operation)
                            || text("release_id") != known.operation_release
                            || !(matches!(
                                known.phase,
                                AppUpdatePhase::Downloading
                                    | AppUpdatePhase::Preparing
                                    | AppUpdatePhase::Ready
                            ) || (known.phase == AppUpdatePhase::Installing
                                && message.get("settled").and_then(Value::as_bool) == Some(true)))
                        {
                            return;
                        }
                        // Fresh preflight withdrew the release before any transfer. Reuse the no-update state.
                        known.phase = AppUpdatePhase::Idle;
                        known.operation_version = None;
                        known.operation_notes = None;
                        known.downloaded = None;
                        known.total = None;
                        known.update_error = None;
                        known.failed_phase = None;
                        known.failed_version = None;
                        known.install_after_download = false;
                        self.workspace.release_install();
                    }
                    known.app_latest = None;
                    known.app_notes = None;
                    known.release_id = None;
                    if known.phase == AppUpdatePhase::Available {
                        known.phase = AppUpdatePhase::Idle;
                    }
                    known.unconfigured = false;
                }
                Some("app_update_unconfigured") => {
                    known.app_latest = None;
                    known.unconfigured = true;
                }
                Some("app_update_check_failed") => {
                    known.error = text("message");
                }
                Some(
                    kind @ ("app_update_progress"
                    | "app_update_preparing"
                    | "app_update_ready"
                    | "app_update_failed"),
                ) => {
                    // A replaced transfer or a different release cannot change this operation.
                    if message.get("operation").and_then(Value::as_u64)
                        != Some(u64::from(known.operation))
                        || text("release_id") != known.operation_release
                        || !matches!(
                            known.phase,
                            AppUpdatePhase::Downloading
                                | AppUpdatePhase::Preparing
                                | AppUpdatePhase::Installing
                        )
                    {
                        return;
                    }
                    match kind {
                        "app_update_progress" if known.phase == AppUpdatePhase::Downloading => {
                            known.downloaded = message.get("downloaded").and_then(Value::as_u64);
                            known.total = message.get("total").and_then(Value::as_u64);
                        }
                        "app_update_preparing" if known.phase == AppUpdatePhase::Downloading => {
                            known.phase = AppUpdatePhase::Preparing
                        }
                        "app_update_ready"
                            if matches!(
                                known.phase,
                                AppUpdatePhase::Downloading | AppUpdatePhase::Preparing
                            ) =>
                        {
                            if known.install_after_download {
                                known.phase = AppUpdatePhase::Installing;
                                let install = self.audit(&known.operation_actor, "app.update.install", known.operation_release.as_deref().unwrap_or_default())
                                    .and_then(|()| {
                                        if self.to_shell(json!({"type":"install_app_update", "release_id":known.operation_release, "operation":known.operation})) { Ok(()) }
                                        else { Err(CoreError::UpdateFailed("The desktop shell is gone.".into())) }
                                    });
                                if let Err(error) = install {
                                    known.phase = AppUpdatePhase::Ready;
                                    known.update_error = Some(error.public_message());
                                    known.failed_phase = Some(AppUpdatePhase::Installing);
                                    self.workspace.release_install();
                                }
                            } else {
                                known.phase = AppUpdatePhase::Ready;
                                self.workspace.release_install();
                            }
                        }
                        "app_update_failed" => {
                            if known.release_id != known.operation_release {
                                self.revoke_app_consent(&mut known);
                            } else {
                                let installing = known.phase == AppUpdatePhase::Installing;
                                known.failed_phase = Some(if installing {
                                    AppUpdatePhase::Installing
                                } else {
                                    AppUpdatePhase::Downloading
                                });
                                if message.get("changed").and_then(Value::as_bool) == Some(true) {
                                    known.failed_phase = None;
                                }
                                known.failed_version = known.operation_version.clone();
                                known.update_error = text("message");
                                known.phase = if installing
                                    && message.get("retained").and_then(Value::as_bool)
                                        == Some(true)
                                {
                                    AppUpdatePhase::Ready
                                } else if known.release_id.is_some() {
                                    AppUpdatePhase::Available
                                } else {
                                    AppUpdatePhase::Idle
                                };
                                known.downloaded = None;
                                known.total = None;
                                known.install_after_download = false;
                                self.workspace.release_install();
                            }
                        }
                        _ => return,
                    }
                }
                _ => return,
            }
        }
        self.changed();
    }

    fn revoke_app_consent(&self, known: &mut Known) {
        known.phase = if known.release_id.is_some() {
            AppUpdatePhase::Available
        } else {
            AppUpdatePhase::Idle
        };
        known.downloaded = None;
        known.total = None;
        known.install_after_download = false;
        known.operation_release = None;
        known.operation_version = None;
        known.operation_notes = None;
        known.failed_phase = None;
        known.failed_version = None;
        known.update_error =
            Some("The available update changed. Review the new version before downloading.".into());
        self.workspace.release_install();
    }

    /// Looks at the world without changing it. Each source fails on its own.
    pub async fn check(&self) -> UpdateStatus {
        let mut errors = Vec::new();
        if self.workspace.root().is_ok()
            && let Err(error) = self.workspace.fetch().await
        {
            errors.push(format!("Library: {}", error.public_message()));
        }
        match self.engines.statuses(true).await {
            Ok(statuses) => {
                let mut known = lock(&self.known);
                for status in statuses {
                    if let Some(latest) = status.latest {
                        known.engine_latest.insert(status.id, latest);
                    }
                }
            }
            Err(error) => errors.push(format!("Engines: {}", error.public_message())),
        }
        if self.mode() == AppMode::Desktop {
            self.to_shell(json!({ "type": "check_app_update" }));
        } else if let Err(message) = self.check_manifest().await {
            errors.push(format!("App: {message}"));
        }
        {
            let mut known = lock(&self.known);
            if errors.is_empty() {
                known.checked_at = Some(now());
                known.error = None;
            } else {
                known.error = Some(errors.join(" "));
            }
        }
        self.changed();
        self.status().await
    }

    async fn check_manifest(&self) -> std::result::Result<(), String> {
        let Some(url) = &self.manifest_url else {
            lock(&self.known).unconfigured = true;
            return Ok(());
        };
        let response = self
            .http
            .get(url)
            .timeout(MANIFEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| "The release manifest could not be reached.".to_owned())?;
        if !response.status().is_success() {
            return Err("The release manifest could not be read.".to_owned());
        }
        let manifest: ReleaseManifest = response
            .json()
            .await
            .map_err(|_| "The release manifest is malformed.".to_owned())?;
        let mut known = lock(&self.known);
        known.unconfigured = false;
        known.app_notes = manifest.notes;
        known.app_latest = Some(manifest.version);
        Ok(())
    }

    /// What the status is read from: engine updates and changes to the engine set, changes
    /// to the library, and whether a run is active. A change to any of them is a new key, so
    /// progress shows at once and never waits for a fresh time to pass.
    fn status_key(&self, runs_active: bool) -> String {
        crate::util::swr::key(&[
            "status",
            &self.engines.change_epoch().to_string(),
            &self.workspace.library_epoch().to_string(),
            if runs_active { "runs" } else { "idle" },
        ])
    }

    /// The last status at once, read again in the background after 20 seconds and
    /// announced as `updates` when it changed; for the polled read, which must not wait
    /// on Git and the engines. `refresh` reads now and keeps the answer.
    pub async fn cached_status(self: &Arc<Self>, refresh: bool) -> UpdateStatus {
        let center = self.clone();
        // Asked once: the key and the status it reads agree on whether a run is active.
        let runs_active = self.runs_active();
        let kept = self
            .status_reads
            .get(
                &self.status_key(runs_active),
                "updates",
                refresh,
                move || async move { Ok(center.status_given(runs_active).await) },
            )
            .await;
        match kept {
            Ok(status) => status,
            Err(_) => self.status().await,
        }
    }

    /// The current picture: local Git state and run activity are read fresh, and
    /// the network's answers come from the last check.
    pub async fn status(&self) -> UpdateStatus {
        self.status_given(self.runs_active()).await
    }

    fn runs_active(&self) -> bool {
        self.store.active_runs().is_ok_and(|runs| !runs.is_empty())
    }

    async fn status_given(&self, runs_active: bool) -> UpdateStatus {
        let library = self.library_status(runs_active).await;
        let engines = self.engine_statuses().await;
        let known = lock(&self.known);
        UpdateStatus {
            library,
            app: self.app_status(&known, runs_active),
            engines,
            checked_at: known.checked_at.clone(),
            error: known.error.clone(),
        }
    }

    async fn library_status(&self, runs_active: bool) -> LibraryUpdateStatus {
        let identity = self.workspace.identity().await.ok();
        let (ahead, behind, incoming) =
            self.workspace
                .local_position()
                .await
                .unwrap_or((0, 0, Vec::new()));
        let dirty = identity.as_ref().is_some_and(|(_, dirty)| *dirty);
        let blocked_reason = if identity.is_none() {
            Some("Set up the workspace first.".to_owned())
        } else if behind == 0 {
            Some("The library is up to date.".to_owned())
        } else if dirty {
            Some("The library has local edits.".to_owned())
        } else if ahead > 0 {
            Some("The library has its own commits, so it cannot fast-forward.".to_owned())
        } else if runs_active {
            Some("Finish or stop active runs first.".to_owned())
        } else {
            None
        };
        LibraryUpdateStatus {
            current: identity.map(|(revision, _)| revision),
            latest: self.workspace.upstream_revision().await,
            behind,
            ahead,
            dirty,
            incoming,
            can_apply: blocked_reason.is_none(),
            blocked_reason,
        }
    }

    async fn engine_statuses(&self) -> Vec<EngineUpdateStatus> {
        let statuses = self.engines.statuses(false).await.unwrap_or_default();
        let mut engines = Vec::with_capacity(statuses.len());
        for status in statuses {
            let latest = status
                .latest
                .clone()
                .or_else(|| lock(&self.known).engine_latest.get(&status.id).cloned());
            let busy = self
                .store
                .active_runs_for_engine(&status.id)
                .map_or(true, |count| count > 0);
            let blocked_reason = engine_block(&status, latest.as_deref(), busy);
            engines.push(EngineUpdateStatus {
                id: status.id,
                current: status.version,
                latest,
                support: status.support,
                can_apply: blocked_reason.is_none(),
                blocked_reason,
            });
        }
        engines
    }

    fn app_status(&self, known: &Known, runs_active: bool) -> AppUpdateStatus {
        let mode = self.mode();
        let newer = known
            .app_latest
            .as_deref()
            .is_some_and(|latest| is_newer(latest, APP_VERSION));
        let blocked_reason = match mode {
            AppMode::Server => Some(if known.unconfigured {
                "No release manifest is configured. Set --update-manifest to check for releases."
                    .to_owned()
            } else {
                "A server cannot replace itself. Install the new release and restart it.".to_owned()
            }),
            AppMode::Desktop if known.unconfigured => {
                Some("This build has no update channel.".to_owned())
            }
            AppMode::Desktop if !newer && known.phase != AppUpdatePhase::Ready => {
                Some("Orbyn is up to date.".to_owned())
            }
            AppMode::Desktop
                if matches!(
                    known.phase,
                    AppUpdatePhase::Downloading
                        | AppUpdatePhase::Preparing
                        | AppUpdatePhase::Installing
                ) =>
            {
                Some("An app update is already running.".to_owned())
            }
            AppMode::Desktop if runs_active => Some("Finish or stop active runs first.".to_owned()),
            AppMode::Desktop => None,
        };
        AppUpdateStatus {
            current: APP_VERSION.to_owned(),
            latest: if matches!(
                known.phase,
                AppUpdatePhase::Downloading
                    | AppUpdatePhase::Preparing
                    | AppUpdatePhase::Ready
                    | AppUpdatePhase::Installing
            ) {
                known.operation_version.clone()
            } else {
                known.app_latest.clone()
            },
            notes: if matches!(
                known.phase,
                AppUpdatePhase::Downloading
                    | AppUpdatePhase::Preparing
                    | AppUpdatePhase::Ready
                    | AppUpdatePhase::Installing
            ) {
                known.operation_notes.clone()
            } else {
                known.app_notes.clone()
            },
            mode,
            can_apply: blocked_reason.is_none(),
            blocked_reason,
            unconfigured: known.unconfigured,
            phase: known.phase,
            release_id: if matches!(
                known.phase,
                AppUpdatePhase::Downloading
                    | AppUpdatePhase::Preparing
                    | AppUpdatePhase::Ready
                    | AppUpdatePhase::Installing
            ) {
                known.operation_release.clone()
            } else {
                known.release_id.clone()
            },
            operation: known.operation,
            update_error: known.update_error.clone(),
            failed_phase: known.failed_phase,
            failed_version: known.failed_version.clone(),
            downloaded: known.downloaded,
            total: known.total,
        }
    }

    /// Explicit consent to fetch one release. This never installs it.
    pub async fn download_app(&self, actor: &str, request: AppUpdateRequest) -> Result<()> {
        self.start_app(actor, request, AppIntent::Download)
    }

    /// Installs only the verified payload the person saw as ready.
    pub async fn install_app(&self, actor: &str, request: AppUpdateRequest) -> Result<()> {
        self.start_app(actor, request, AppIntent::Install)
    }

    /// Update everything and automatic mode explicitly select both phases.
    pub async fn apply_app(&self, actor: &str) -> Result<()> {
        let app = self.status().await.app;
        self.apply_reported_app(actor, app)
    }

    fn apply_reported_app(&self, actor: &str, app: AppUpdateStatus) -> Result<()> {
        let request = AppUpdateRequest {
            release_id: app.release_id.unwrap_or_default(),
            operation: app.operation,
        };
        self.start_app(actor, request, AppIntent::Complete)
    }

    fn start_app(&self, actor: &str, request: AppUpdateRequest, intent: AppIntent) -> Result<()> {
        if self.mode() == AppMode::Server {
            return Err(CoreError::UpdateUnavailable(
                "A server cannot replace itself.".into(),
            ));
        }
        let mut known = lock(&self.known);
        let install = matches!(intent, AppIntent::Install)
            || (matches!(intent, AppIntent::Complete) && known.phase == AppUpdatePhase::Ready);
        let busy = matches!(
            known.phase,
            AppUpdatePhase::Downloading | AppUpdatePhase::Preparing | AppUpdatePhase::Installing
        );
        if busy {
            let same_action = if install {
                known.phase == AppUpdatePhase::Installing
            } else {
                matches!(
                    known.phase,
                    AppUpdatePhase::Downloading | AppUpdatePhase::Preparing
                )
            };
            if same_action
                && known.operation_release.as_deref() == Some(&request.release_id)
                && (request.operation == known.operation
                    || (!install && request.operation.checked_add(1) == Some(known.operation)))
            {
                return Ok(());
            }
            return Err(CoreError::UpdateUnavailable(
                "An app update is already running.".into(),
            ));
        }
        let expected = if install {
            known.operation_release.as_ref()
        } else {
            known.release_id.as_ref()
        };
        if expected != Some(&request.release_id) || request.operation != known.operation {
            return Err(CoreError::UpdateUnavailable(
                "The available update changed. Review the new version before downloading.".into(),
            ));
        }
        if (install && known.phase != AppUpdatePhase::Ready)
            || (!install && known.phase != AppUpdatePhase::Available)
            || (!install
                && !known
                    .app_latest
                    .as_deref()
                    .is_some_and(|v| is_newer(v, APP_VERSION)))
        {
            return Err(CoreError::UpdateUnavailable("No update is ready.".into()));
        }
        // Known serializes duplicate requests; admission atomically excludes active/new runs.
        self.workspace.hold_for_install()?;
        let action = if install {
            "app.update.install"
        } else {
            "app.update.download"
        };
        if let Err(error) = self.audit(actor, action, &request.release_id) {
            self.workspace.release_install();
            return Err(error);
        }
        let operation = if install {
            known.operation
        } else {
            known.operation.checked_add(1).ok_or_else(|| {
                self.workspace.release_install();
                CoreError::UpdateUnavailable("Restart Orbyn before updating.".into())
            })?
        };
        let line = json!({"type":if install {"install_app_update"} else {"download_app_update"}, "release_id":request.release_id, "operation":operation});
        if !self.to_shell(line) {
            self.workspace.release_install();
            return Err(CoreError::UpdateFailed("The desktop shell is gone.".into()));
        }
        if !install {
            known.operation = operation;
            known.operation_release = Some(request.release_id);
            known.operation_version = known.app_latest.clone();
            known.operation_notes = known.app_notes.clone();
            known.operation_actor = actor.to_owned();
        }
        known.phase = if install {
            AppUpdatePhase::Installing
        } else {
            AppUpdatePhase::Downloading
        };
        known.install_after_download = matches!(intent, AppIntent::Complete);
        known.update_error = None;
        known.failed_phase = None;
        known.failed_version = None;
        known.downloaded = None;
        known.total = None;
        drop(known);
        self.changed();
        Ok(())
    }

    /// Applies what can be applied, in order, stopping at the first failure.
    pub async fn apply_all(&self, actor: &str) -> Result<ApplyAllResult> {
        if !self.store.active_runs()?.is_empty() {
            return Err(CoreError::RunsActive);
        }
        let status = self.status().await;
        self.audit(actor, "updates.apply_all", "all")?;
        let mut steps = Vec::new();
        for engine in status.engines.iter().filter(|engine| engine.can_apply) {
            let outcome = self.engines.update(&engine.id, actor).await.map(|_| ());
            let went_on = record(&mut steps, format!("engine:{}", engine.id), outcome);
            // Each step moves what the update screen shows.
            self.changed();
            if !went_on {
                return Ok(self.finish(steps));
            }
        }
        if status.library.can_apply {
            let outcome = self.workspace.update(true).await.map(|_| ());
            let went_on = record(&mut steps, "library".to_owned(), outcome);
            self.changed();
            if !went_on {
                return Ok(self.finish(steps));
            }
        }
        if status.app.can_apply {
            let outcome = self.apply_reported_app(actor, status.app);
            record(&mut steps, "app".to_owned(), outcome);
        }
        Ok(self.finish(steps))
    }

    fn finish(&self, steps: Vec<UpdateStep>) -> ApplyAllResult {
        self.changed();
        ApplyAllResult { steps }
    }

    fn audit(&self, actor: &str, action: &str, target: &str) -> Result<()> {
        self.store.append_audit(&AuditRecord {
            actor: actor.to_owned(),
            action: action.to_owned(),
            target: target.to_owned(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail: Value::Null,
        })?;
        Ok(())
    }

    /// Checks at startup, then every six hours, unless the setting turns it off.
    pub async fn run_checks(&self, stop: CancellationToken) {
        let mut ticks = tokio::time::interval(CHECK_INTERVAL);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = ticks.tick() => {
                    let enabled = self
                        .store
                        .settings()
                        .map_or(true, |settings| settings.background_update_checks != Some(false));
                    if enabled {
                        self.check().await;
                    }
                }
            }
        }
    }
}

fn engine_block(status: &EngineStatus, latest: Option<&str>, busy: bool) -> Option<String> {
    let newer = matches!(
        (latest, status.version.as_deref()),
        (Some(latest), Some(current)) if is_newer(latest, current)
    );
    if !newer {
        Some("This engine is up to date.".to_owned())
    } else if status.update_command.is_none() {
        Some(
            status
                .manual_hint
                .clone()
                .unwrap_or_else(|| "Update this engine with the installer you used.".to_owned()),
        )
    } else if busy {
        Some("This engine has an active run.".to_owned())
    } else {
        None
    }
}

fn record(steps: &mut Vec<UpdateStep>, target: String, outcome: Result<()>) -> bool {
    let ok = outcome.is_ok();
    steps.push(UpdateStep {
        target,
        ok,
        message: outcome.err().map(|error| error.public_message()),
    });
    ok
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(all(test, unix))]
mod tests;

#[cfg(test)]
mod lifecycle_tests;
