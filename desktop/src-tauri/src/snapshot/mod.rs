//! Snap Shot's desktop shell: settings, the pending in-memory captures, one global shortcut and the six commands.

// The collector is pure and tested on every OS. Only the macOS reader wires it to a platform, so
// non-macOS builds keep the allow instead of reporting every item as unused.
#[cfg_attr(not(test), allow(dead_code))]
mod accessible;
#[cfg(target_os = "macos")]
mod ax;
mod commands;
mod exclusion;
mod flow;
#[cfg(target_os = "macos")]
mod macos;
mod pending;
mod settings;
mod setup;
mod shortcut;
mod state;
#[cfg(not(target_os = "macos"))]
mod stub;
#[cfg(windows)]
mod windows;
// Shared plain code used by the Windows capturer; compiled and tested on every OS.
#[cfg_attr(not(windows), allow(dead_code))]
mod windows_png;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(not(target_os = "macos"))]
use stub as platform;

// `generate_handler!` resolves each command through these generated macros, so they must be
// reachable at this module's path as well as inside `commands`.
#[allow(unused_imports)]
pub use commands::{
    __cmd__snapshot_capture, __cmd__snapshot_configure, __cmd__snapshot_discard,
    __cmd__snapshot_setup, __cmd__snapshot_state, __cmd__snapshot_take,
    __tauri_command_name_snapshot_capture, __tauri_command_name_snapshot_configure,
    __tauri_command_name_snapshot_discard, __tauri_command_name_snapshot_setup,
    __tauri_command_name_snapshot_state, __tauri_command_name_snapshot_take, SetupAction,
    snapshot_capture, snapshot_configure, snapshot_discard, snapshot_setup, snapshot_state,
    snapshot_take,
};
pub use settings::Settings;
pub use state::SnapShotState;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

use flow::{Events, Flow, Gate};
use pending::Pending;
use setup::{ScreenRecordingSetup, SetupPlatform};
use shortcut::Registration;
use state::PlatformFacts;

const SWEEP_INTERVAL: Duration = Duration::from_secs(5);

pub struct SnapShot {
    settings: Mutex<Settings>,
    pub(crate) pending: Arc<Mutex<Pending>>,
    pub(crate) gate: Arc<Gate>,
    registration: Mutex<Registration>,
    screen_recording_setup: Mutex<ScreenRecordingSetup>,
    stop: Arc<AtomicBool>,
    reaper: Option<JoinHandle<()>>,
    reaper_thread: thread::Thread,
}

impl Default for SnapShot {
    fn default() -> Self {
        let pending = Arc::new(Mutex::new(Pending::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let watched = pending.clone();
        let stopped = stop.clone();
        let reaper = thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                thread::park_timeout(SWEEP_INTERVAL);
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                if let Ok(mut pending) = watched.lock() {
                    pending.sweep();
                }
            }
        });
        let reaper_thread = reaper.thread().clone();
        Self {
            settings: Mutex::new(Settings::default()),
            pending,
            gate: Arc::new(Gate::default()),
            registration: Mutex::new(Registration::default()),
            screen_recording_setup: Mutex::new(ScreenRecordingSetup::default()),
            stop,
            reaper: Some(reaper),
            reaper_thread,
        }
    }
}

impl Drop for SnapShot {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.reaper_thread.unpark();
        if let Some(reaper) = self.reaper.take() {
            let _ = reaper.join();
        }
    }
}

impl SnapShot {
    pub fn state(&self) -> SnapShotState {
        self.state_with(platform::facts())
    }

    fn state_with(&self, facts: PlatformFacts) -> SnapShotState {
        let settings = match self.settings.lock() {
            Ok(settings) => settings.clone(),
            Err(_) => Settings::default(),
        };
        let (registered, message) = match self.registration.try_lock() {
            Ok(registration) => (registration.registered, registration.message.clone()),
            Err(_) => (false, None),
        };
        state::compute(&settings, registered, message, &facts)
    }

    pub(crate) fn allow_screen_recording(&self) -> Result<SnapShotState, &'static str> {
        let platform = platform::setup_platform();
        self.allow_screen_recording_with(&platform, platform::facts, Instant::now())
    }

    pub(crate) fn allow_screen_recording_with(
        &self,
        platform: &dyn SetupPlatform,
        facts: impl FnOnce() -> PlatformFacts,
        now: Instant,
    ) -> Result<SnapShotState, &'static str> {
        {
            let mut setup = self
                .screen_recording_setup
                .lock()
                .map_err(|_| "unavailable")?;
            setup.attempt(platform, now);
        }
        Ok(self.state_with(facts()))
    }

    pub fn enabled(&self) -> bool {
        self.settings
            .lock()
            .map(|settings| settings.snapshot_enabled)
            .unwrap_or(false)
    }

    pub fn configure(
        &self,
        app: &AppHandle,
        settings: Settings,
    ) -> Result<SnapShotState, &'static str> {
        settings.validate()?;
        match self.settings.lock() {
            Ok(mut stored) => *stored = settings,
            Err(_) => return Err("unavailable"),
        }
        self.apply_shortcut(app);
        let state = self.state();
        let _ = app.emit_to("main", "snapshot://state_changed", json!({}));
        Ok(state)
    }

    pub fn apply_shortcut(&self, app: &AppHandle) {
        let settings = match self.settings.lock() {
            Ok(settings) => settings.clone(),
            Err(_) => Settings::default(),
        };
        let supported = platform::facts().supported;
        if let Ok(mut registration) = self.registration.lock() {
            shortcut::apply(app, &settings, &mut registration, supported);
        }
    }

    pub fn start_capture(&self, app: &AppHandle) -> Result<String, &'static str> {
        let settings = self.settings.lock().map_err(|_| "unavailable")?.clone();
        if !settings.snapshot_enabled {
            return Err("disabled");
        }
        if !platform::facts().supported {
            return Err("unsupported");
        }
        let own_bundle_id = app.config().identifier.clone();
        let events: Arc<dyn Events> = Arc::new(TauriEvents { app: app.clone() });
        let flow = Flow::new(self.pending.clone(), self.gate.clone(), events);
        let (id, cancelled) = flow.begin().map_err(|_| "unavailable")?;
        if let Some(cancelled) = cancelled {
            let started = id.clone();
            let capturer =
                platform::capturer(&own_bundle_id, settings.snapshot_include_accessibility);
            let _ = std::thread::spawn(move || {
                flow.run(started, cancelled, capturer, settings, own_bundle_id)
            });
        }
        Ok(id)
    }
}

struct TauriEvents {
    app: AppHandle,
}

impl Events for TauriEvents {
    fn emit(&self, name: &str, payload: Value) {
        let _ = self.app.emit_to("main", name, payload);
    }
}
