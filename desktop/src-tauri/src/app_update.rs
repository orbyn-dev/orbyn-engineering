//! The shell owns release handles, verified payloads and exclusion across service crashes.
mod ownership;
use ownership::{Ownership, Settlement};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub struct Updates(Mutex<Slot>);
#[derive(Default)]
struct Slot {
    owner: Ownership,
    check: u64,
    candidate: Option<(String, Update)>,
    staged: Option<Staged>,
    download: Option<tauri::async_runtime::JoinHandle<()>>,
    kind: Option<EffectKind>,
    operation: Option<(String, u32)>,
    pending_report: Option<Value>,
}
#[derive(Clone, Copy, PartialEq)]
enum EffectKind {
    Download,
    Install,
}
fn same_release(left: &Update, right: &Update) -> bool {
    left.version == right.version
        && left.target == right.target
        && left.download_url == right.download_url
        && left.signature == right.signature
}

/// The version an artifact's signature was made for, read from its trusted comment.
///
/// Only call this with a signature the plugin has verified over the downloaded bytes: minisign
/// signs the trusted comment together with the artifact, so before that verification the comment
/// is endpoint data and not evidence. `None` when the signature is malformed or was made by a Tauri
/// CLI that did not record the version.
fn signed_version(signature: &str) -> Option<String> {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(signature.trim())
        .ok()?;
    let text = String::from_utf8(decoded).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("trusted comment: "))
        .and_then(|comment| {
            comment
                .split('\t')
                .find_map(|field| field.strip_prefix("version:"))
        })
        .map(|version| version.trim().to_owned())
}

/// Refuses a release whose verified signature does not name the announced version.
///
/// The update endpoint is fetched over TLS but is not itself signed, and the artifact signature
/// only covers the artifact, so the announced version proves nothing on its own: a compromised
/// endpoint could pair an older, still validly signed artifact with a higher version and force a
/// downgrade to a genuine but outdated build. The plugin checks this binding through
/// `requireSignedVersion` in `tauri.conf.json`; checking it again at the boundary that stages
/// verified bytes keeps the shell closed if a build-time config override or a plugin default ever
/// drops that flag. The cryptographic verification stays the plugin's: this only reads the version
/// from a signature the plugin has already validated. Versions compare literally apart from a
/// leading `v`, which is how the endpoint and the signing CLI spell the same release.
fn version_binding(version: &str, signature: &str) -> Result<(), String> {
    if signature.trim().is_empty() {
        return Err("The update is unsigned, so it cannot be installed.".into());
    }
    let Some(signed) = signed_version(signature) else {
        return Err(
            "The update signature does not carry the version it was signed for.".into(),
        );
    };
    let normalise = |value: &str| value.trim().trim_start_matches('v').to_owned();
    if normalise(&signed) == normalise(version) {
        Ok(())
    } else {
        Err(format!(
            "The update was signed for version {signed}, not the announced {version}."
        ))
    }
}

struct Staged {
    generation: u64,
    release_id: String,
    operation: u32,
    update: Update,
    bytes: Arc<Vec<u8>>,
}
fn epoch(app: &AppHandle) -> u64 {
    app.state::<super::ServiceGeneration>()
        .0
        .load(Ordering::SeqCst)
}
fn slot(updates: &Updates) -> std::sync::MutexGuard<'_, Slot> {
    updates
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn send(app: &AppHandle, generation: u64, value: Value) {
    if epoch(app) == generation {
        super::tell_core(app, value);
    }
}
fn failed(
    app: &AppHandle,
    generation: u64,
    id: &str,
    operation: u32,
    message: String,
    retained: bool,
) {
    send(
        app,
        generation,
        json!({"type":"app_update_failed","release_id":id,"operation":operation,"message":message,"retained":retained}),
    );
}

/// Called before stopping/replacing a service. An install owns exclusion until its blocking effect settles.
pub fn prepare_service(app: &AppHandle) -> Option<u64> {
    let state = app.state::<Updates>();
    let mut state = slot(&state);
    if !state.owner.request_restart() {
        return None;
    }
    let generation = app
        .state::<super::ServiceGeneration>()
        .0
        .fetch_add(1, Ordering::SeqCst)
        + 1;
    if !state.owner.attach(generation) {
        return None;
    }
    state.candidate = None;
    state.staged = None;
    state.pending_report = None;
    state.operation = None;
    Some(generation)
}
/// Invalidates this generation before canceling its owned network future. A blocking installer is never aborted.
pub fn service_exited(app: &AppHandle, generation: u64) -> bool {
    if app
        .state::<super::ServiceGeneration>()
        .0
        .compare_exchange(
            generation,
            generation + 1,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_err()
    {
        return false;
    }
    let download = {
        let state = app.state::<Updates>();
        let mut state = slot(&state);
        state.owner.exited(generation);
        state.candidate = None;
        state.staged = None;
        state.download.take()
    };
    if let Some(download) = download {
        download.abort();
    }
    true
}
pub fn shutdown(app: &AppHandle) {
    app.state::<super::ServiceGeneration>()
        .0
        .fetch_add(1, Ordering::SeqCst);
    let download = {
        let state = app.state::<Updates>();
        let mut state = slot(&state);
        state.owner.shutdown();
        state.download.take()
    };
    if let Some(download) = download {
        download.abort();
    }
}
fn settlement(
    app: AppHandle,
    generation: u64,
    effect: u64,
    id: String,
    operation: u32,
    retained: bool,
) -> Settlement<impl FnOnce(bool)> {
    Settlement::new(move |reported| {
        let (restart, report) = {
            let state = app.state::<Updates>();
            let mut state = slot(&state);
            if !state.owner.owns(generation, effect) {
                return;
            }
            state.download = None;
            state.kind = None;
            state.operation = None;
            let restart = state.owner.finish(generation, effect);
            let mut report = state.pending_report.take();
            if let Some(report) = &mut report {
                report["settled"] = json!(true);
            }
            if !state.owner.current(generation) {
                state.staged = None;
                state.candidate = None;
                report = None;
            }
            (restart, report)
        };
        if let Some(report) = report {
            send(&app, generation, report);
        } else if !reported {
            failed(
                &app,
                generation,
                &id,
                operation,
                "The app update stopped before completion.".into(),
                retained,
            );
        }
        if restart {
            let dispatch = app.clone();
            if let Err(error) =
                std::thread::Builder::new().spawn(move || super::launch_core(&dispatch))
            {
                super::show_failure(&app, &format!("Cannot restart the service: {error}."), true);
            }
        }
    })
}

async fn read_release(app: &AppHandle) -> Result<Option<Update>, String> {
    let (Some(pubkey), Some(endpoint)) = (
        option_env!("ORBYN_UPDATE_PUBKEY"),
        option_env!("ORBYN_UPDATE_ENDPOINT").and_then(|v| url::Url::parse(v).ok()),
    ) else {
        return Err("This build has no update channel.".into());
    };
    let updater = app
        .updater_builder()
        .pubkey(pubkey)
        .timeout(Duration::from_secs(30))
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| e.to_string())
}
pub fn check(app: &AppHandle, generation: u64) {
    if epoch(app) != generation {
        return;
    }
    let check = {
        let state = app.state::<Updates>();
        let mut state = slot(&state);
        if !state.owner.current(generation) {
            return;
        }
        state.check = state
            .check
            .checked_add(1)
            .expect("update check counter exhausted");
        state.check
    };
    if option_env!("ORBYN_UPDATE_PUBKEY").is_none()
        || option_env!("ORBYN_UPDATE_ENDPOINT").is_none()
    {
        send(app, generation, json!({"type":"app_update_unconfigured"}));
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = read_release(&app).await;
        let state = app.state::<Updates>();
        let mut state = slot(&state);
        if !state.owner.current(generation) || epoch(&app) != generation || state.check != check {
            return;
        }
        let mut abort = None;
        let message = match result {
            Ok(Some(mut update)) => {
                update.timeout = Some(Duration::from_secs(120));
                let previous = state.candidate.as_ref().map(|(id, _)| id.clone());
                let id = state
                    .candidate
                    .as_ref()
                    .filter(|(_, old)| same_release(old, &update))
                    .map(|(id, _)| id.clone())
                    .unwrap_or_else(|| format!("release-{check}"));
                let changed = previous.as_ref().is_some_and(|old| old != &id)
                    || state
                        .staged
                        .as_ref()
                        .is_some_and(|old| old.release_id != id)
                    || state.operation.as_ref().is_some_and(|(old, _)| old != &id);
                let message = json!({"type":"app_update_available","release_id":id,"version":update.version,"notes":update.body,"changed":changed});
                state.candidate = Some((id, update));
                if changed {
                    state.staged = None;
                }
                if changed && state.owner.busy() {
                    state.pending_report = Some(message);
                    if state.kind == Some(EffectKind::Download) {
                        abort = state.download.take();
                    }
                    None
                } else {
                    Some(message)
                }
            }
            Ok(None) => {
                let previous = state.operation.clone().or_else(|| {
                    state
                        .staged
                        .as_ref()
                        .map(|stage| (stage.release_id.clone(), stage.operation))
                });
                state.candidate = None;
                state.staged = None;
                let message = if let Some((id, operation)) = previous {
                    json!({"type":"app_update_none","release_id":id,"operation":operation})
                } else {
                    json!({"type":"app_update_none"})
                };
                if state.owner.busy() {
                    state.pending_report = Some(message);
                    if state.kind == Some(EffectKind::Download) {
                        abort = state.download.take();
                    }
                    None
                } else {
                    Some(message)
                }
            }
            Err(message) => Some(json!({"type":"app_update_check_failed","message":message})),
        };
        if let Some(message) = message {
            send(&app, generation, message);
        }
        drop(state);
        if let Some(abort) = abort {
            abort.abort();
        }
    });
}
pub fn download(app: &AppHandle, generation: u64, release_id: String, operation: u32) {
    if epoch(app) != generation {
        return;
    }
    let (update, effect, preflight) = {
        let state = app.state::<Updates>();
        let mut state = slot(&state);
        if !state.owner.current(generation) {
            return;
        }
        if state.owner.busy() {
            failed(
                app,
                generation,
                &release_id,
                operation,
                "An app update is already running.".into(),
                false,
            );
            return;
        }
        let Some((id, update)) = &state.candidate else {
            state.staged = None;
            send(
                app,
                generation,
                json!({"type":"app_update_none","release_id":release_id,"operation":operation}),
            );
            return;
        };
        if id != &release_id {
            send(
                app,
                generation,
                json!({"type":"app_update_failed","release_id":release_id,"operation":operation,"message":"The available update changed. Review the new version before downloading.","changed":true,"retained":false}),
            );
            return;
        }
        let update = update.clone();
        let Some(effect) = state.owner.begin(generation) else {
            return;
        };
        state.staged = None;
        state.kind = Some(EffectKind::Download);
        state.operation = Some((release_id.clone(), operation));
        state.check = state
            .check
            .checked_add(1)
            .expect("update check counter exhausted");
        (update, effect, state.check)
    };
    // Created outside the future: cancellation before its first poll still settles the lease.
    let guard = settlement(
        app.clone(),
        generation,
        effect,
        release_id.clone(),
        operation,
        false,
    );
    let task_app = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        let app = task_app;
        let mut guard = guard;
        let fresh = read_release(&app).await;
        let superseded = {
            let state = app.state::<Updates>();
            let state = slot(&state);
            state.check != preflight
                || !state.owner.current(generation)
                || state.pending_report.is_some()
                || state
                    .candidate
                    .as_ref()
                    .is_none_or(|(id, _)| id != &release_id)
        };
        if superseded {
            guard.reported();
            drop(guard);
            failed(
                &app,
                generation,
                &release_id,
                operation,
                "The update check was superseded before download.".into(),
                false,
            );
            return;
        }
        let same = fresh
            .as_ref()
            .ok()
            .and_then(Option::as_ref)
            .is_some_and(|fresh| same_release(fresh, &update));
        if !same {
            let message = {
                let state = app.state::<Updates>();
                let mut state = slot(&state);
                if !state.owner.current(generation)
                    || epoch(&app) != generation
                    || state.check != preflight
                    || state.pending_report.is_some()
                {
                    return;
                }
                match fresh {
                    Ok(Some(mut fresh)) => {
                        fresh.timeout = Some(Duration::from_secs(120));
                        state.check = state
                            .check
                            .checked_add(1)
                            .expect("update check counter exhausted");
                        let id = format!("release-{}", state.check);
                        let available = json!({"type":"app_update_available","release_id":id,"version":fresh.version,"notes":fresh.body,"changed":true});
                        state.candidate = Some((id, fresh));
                        state.pending_report = Some(available);
                        None
                    }
                    Ok(None) => {
                        state.candidate = None;
                        state.staged = None;
                        state.check = state
                            .check
                            .checked_add(1)
                            .expect("update check counter exhausted");
                        state.pending_report = Some(
                            json!({"type":"app_update_none","release_id":release_id,"operation":operation}),
                        );
                        None
                    }
                    Err(message) => Some(
                        json!({"type":"app_update_failed","release_id":release_id,"operation":operation,"message":message,"retained":false}),
                    ),
                }
            };
            guard.reported();
            drop(guard);
            if let Some(message) = message {
                send(&app, generation, message);
            }
            return;
        }
        let revoked = {
            let state = app.state::<Updates>();
            let state = slot(&state);
            !state.owner.current(generation)
                || state.pending_report.is_some()
                || state
                    .candidate
                    .as_ref()
                    .is_none_or(|(id, _)| id != &release_id)
        };
        if revoked {
            guard.reported();
            drop(guard);
            return;
        }
        let progress = app.clone();
        let preparing = app.clone();
        let progress_id = release_id.clone();
        let preparing_id = release_id.clone();
        let mut downloaded = 0u64;
        let result=update.download(move |chunk,total| {
            downloaded=downloaded.saturating_add(chunk as u64);
            send(&progress,generation,json!({"type":"app_update_progress","release_id":progress_id,"operation":operation,"downloaded":downloaded,"total":total}));
        },move || {send(&preparing,generation,json!({"type":"app_update_preparing","release_id":preparing_id,"operation":operation}));}).await;
        let message = {
            let state = app.state::<Updates>();
            let mut state = slot(&state);
            if !state.owner.current(generation)
                || epoch(&app) != generation
                || state.pending_report.is_some()
                || state
                    .candidate
                    .as_ref()
                    .is_none_or(|(id, _)| id != &release_id)
            {
                return;
            }
            match result {
                Ok(bytes) => match version_binding(&update.version, &update.signature) {
                    Ok(()) => {
                        state.staged = Some(Staged {
                            generation,
                            release_id: release_id.clone(),
                            operation,
                            update,
                            bytes: Arc::new(bytes),
                        });
                        json!({"type":"app_update_ready","release_id":release_id,"operation":operation})
                    }
                    // Nothing is staged, so there is no consent to install the rejected release.
                    Err(message) => json!({"type":"app_update_failed","release_id":release_id,"operation":operation,"message":message,"retained":false}),
                },
                Err(error) => {
                    json!({"type":"app_update_failed","release_id":release_id,"operation":operation,"message":error.to_string(),"retained":false})
                }
            }
        };
        guard.reported();
        drop(guard);
        send(&app, generation, message);
    });
    let mut task = Some(task);
    {
        let state = app.state::<Updates>();
        let mut state = slot(&state);
        if state.owner.current(generation)
            && state.owner.owns(generation, effect)
            && state.pending_report.is_none()
        {
            state.download = task.take();
        }
    }
    if let Some(task) = task {
        task.abort();
    }
}
pub fn install(app: &AppHandle, generation: u64, release_id: String, operation: u32) {
    if epoch(app) != generation {
        return;
    }
    let (payload, effect) = {
        let state = app.state::<Updates>();
        let mut state = slot(&state);
        if !state.owner.current(generation) {
            return;
        }
        if state.owner.busy() {
            failed(
                app,
                generation,
                &release_id,
                operation,
                "An app update is already running.".into(),
                false,
            );
            return;
        }
        let Some(stage) = &state.staged else {
            failed(
                app,
                generation,
                &release_id,
                operation,
                "No verified update is ready.".into(),
                false,
            );
            return;
        };
        if stage.generation != generation
            || stage.release_id != release_id
            || stage.operation != operation
        {
            failed(
                app,
                generation,
                &release_id,
                operation,
                "The staged update changed.".into(),
                false,
            );
            return;
        }
        let payload = (stage.update.clone(), stage.bytes.clone());
        let Some(effect) = state.owner.begin(generation) else {
            return;
        };
        state.kind = Some(EffectKind::Install);
        state.operation = Some((release_id.clone(), operation));
        (payload, effect)
    };
    let guard = settlement(
        app.clone(),
        generation,
        effect,
        release_id.clone(),
        operation,
        true,
    );
    let app = app.clone();
    // The closure owns the lease. Dropping a join handle cannot release an installer that is still running.
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = guard;
        match payload.0.install(payload.1.as_ref()) {
            Ok(()) => {
                #[cfg(not(windows))]
                {
                    guard.reported();
                    app.restart();
                }
                #[cfg(windows)]
                {
                    guard.reported();
                    drop(guard);
                    failed(
                        &app,
                        generation,
                        &release_id,
                        operation,
                        "The installer returned without restarting Orbyn.".into(),
                        true,
                    );
                }
            }
            Err(error) => {
                guard.reported();
                drop(guard);
                failed(
                    &app,
                    generation,
                    &release_id,
                    operation,
                    error.to_string(),
                    true,
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{signed_version, version_binding};

    const SIGNED: &str = "timestamp:1700000000\tfile:app_1.2.3_x64.msi.zip\tversion:1.2.3";

    /// A minisign `.sig` file, base64 encoded the way an update manifest carries it. The signature
    /// bodies are placeholders: these tests cover the version binding, not the cryptography, which
    /// stays the updater plugin's (`Update::download`/`verify`).
    fn signature(trusted_comment: &str) -> String {
        use base64::Engine;
        let file = format!(
            "untrusted comment: signature from tauri secret key\nplaceholder signature\n\
             trusted comment: {trusted_comment}\nplaceholder global signature"
        );
        base64::engine::general_purpose::STANDARD.encode(file)
    }

    #[test]
    fn a_release_signed_for_the_announced_version_is_accepted() {
        let signature = signature(SIGNED);
        assert_eq!(signed_version(&signature).as_deref(), Some("1.2.3"));
        assert!(version_binding("1.2.3", &signature).is_ok());
        // The CLI and the endpoint may spell the same version differently.
        assert!(version_binding("v1.2.3", &signature).is_ok());
    }

    #[test]
    fn a_release_without_a_versioned_signature_is_rejected() {
        // Unsigned, blank, or not a signature at all.
        assert!(version_binding("1.2.3", "").is_err());
        assert!(version_binding("1.2.3", "   ").is_err());
        assert!(version_binding("1.2.3", "not base64").is_err());
        // A signature made by a CLI that did not record the version names no version.
        let unversioned = signature("timestamp:1600000000\tfile:app_1.0.0_x64.msi.zip");
        assert_eq!(signed_version(&unversioned), None);
        assert!(version_binding("1.2.3", &unversioned).is_err());
    }

    #[test]
    fn a_tampered_signature_is_rejected() {
        // An endpoint announcing a newer version than the artifact was signed for.
        let err = version_binding("9.9.9", &signature(SIGNED)).unwrap_err();
        assert!(err.contains("1.2.3") && err.contains("9.9.9"), "{err}");
        // A trusted comment edited after signing no longer names the announced version.
        let edited = signature("timestamp:1700000000\tfile:app.zip\tversion:1.2.4");
        assert!(version_binding("1.2.3", &edited).is_err());
        // Only a whole `version:` field counts, never the tail of another field.
        assert_eq!(
            signed_version(&signature("timestamp:1\tfile:app-version:2.zip")),
            None
        );
    }

    #[test]
    fn the_shipped_updater_configuration_requires_a_signed_version() {
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
            .expect("tauri.conf.json is not valid JSON");
        let updater: tauri_plugin_updater::Config =
            serde_json::from_value(config["plugins"]["updater"].clone())
                .expect("the updater configuration does not match the plugin's schema");
        assert!(
            updater.require_signed_version,
            "the app updater must reject a version its signature does not cover"
        );
    }
}
