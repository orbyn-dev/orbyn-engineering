//! The page a person watches beside a thread, and the servers a thread started. A
//! tab is a headless Chrome of its own, on a profile of its own, separate from the
//! governed browser agents use. Frames are polled and go out only while someone
//! watches or records. This is local: a client watches over the same access as the
//! rest of the API, and nothing here reaches another machine.

mod expressions;

mod snapshots;

mod open;

mod profiles;

mod servers;

mod crash;

mod pump;

mod favicon;

pub mod bridge;
pub(crate) mod recording;
pub(crate) mod trace;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use orbyn_contracts::{
    PreviewInput, PreviewPointer, PreviewRecording, PreviewRecordingOptions, PreviewTab,
    PreviewView,
};
use serde_json::json;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::app::Core;
use crate::browser::preview_bridge::Bridge;
use crate::browser::preview_recording::Recording;
use crate::error::{CoreError, Result};
use crate::governance::computer::cdp::{
    BrowserSession, HistoryState, LaunchOptions, normalize_url,
};
use crate::util::new_id;
pub(crate) use expressions::{DEFAULT_SCROLL, MAX_SCROLL, MAX_TYPE_CHARS, MAX_WAIT_TEXT_CHARS};
pub(crate) use snapshots::MAX_SELECTOR_CHARS;

const MAX_TABS: usize = 4;
const FRAME_EVERY: Duration = Duration::from_millis(200);
const FRAME_QUALITY: u32 = 60;
// Key overlays never inspect or retain a field value. Body and editing surfaces
// cannot establish safe focus (including a closed shadow root hosted by body).
// It runs in an isolated world, so the page cannot replace what it calls.
const RECORDING_SAFE_FOCUS: &str = r"(() => {
  const el = document.activeElement;
  return Boolean(el && document.hasFocus() && document.designMode === 'off' &&
    !el.closest('[contenteditable], [autocomplete=one-time-code]') &&
    ['BUTTON', 'A'].includes(el.tagName));
})()";

const DEFAULT_SIZE: (u32, u32) = (1280, 800);
const PROFILE_GRACE: Duration = Duration::from_millis(500);
/// Bounds a page's size, which sets how much memory each frame takes.
const SIZES: std::ops::RangeInclusive<u32> = 100..=4096;
/// How many frame ticks pass between looks at history, about one a second. A link
/// clicked in the page changes it with nobody telling this side.
const HISTORY_TICKS: u32 = 5;
const ZOOMS: std::ops::RangeInclusive<f64> = 0.25..=3.0;
pub(crate) struct Tab {
    pub(crate) info: Mutex<PreviewTab>,
    pub(crate) session: tokio::sync::Mutex<BrowserSession>,
    frames: broadcast::Sender<Arc<str>>,
    initial_frame_pending: AtomicBool,
    pointers: broadcast::Sender<PreviewPointer>,
    trace: Mutex<crate::browser::preview_trace::Trace>,
    recording: Mutex<Option<Arc<Recording>>>,
    stop: CancellationToken,
    profile: PathBuf,
    /// A named profile stays when the tab ends.
    keep_profile: bool,
    browser: PathBuf,
    options: LaunchOptions,
    /// Clipboard, file chooser and downloads; see `preview_bridge`.
    pub(crate) bridge: Mutex<Bridge>,
    /// The tab's own folder for chosen and downloaded files, gone with the tab.
    pub(crate) files: PathBuf,
}

#[derive(Default)]
pub struct PreviewManager {
    tabs: Mutex<HashMap<String, Arc<Tab>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Tab {
    fn pointer(&self, phase: &str, x: f64, y: f64) {
        if self.stop.is_cancelled() {
            return;
        }
        let info = lock(&self.info);
        lock(&self.trace).emit(
            &self.pointers,
            phase,
            (x * info.zoom, y * info.zoom),
            (info.width, info.height),
        );
        if phase == "click"
            && let Some(recording) = lock(&self.recording).as_ref()
        {
            recording.mouse(x * info.zoom, y * info.zoom);
        }
    }

    fn clear_pointer(&self) {
        self.pointer("clear", 0.0, 0.0);
        if let Some(recording) = lock(&self.recording).as_ref() {
            recording.clear_input();
        }
    }

    fn view(&self) -> PreviewTab {
        let mut info = lock(&self.info).clone();
        info.viewers = self.frames.receiver_count();
        let recording = lock(&self.recording);
        info.recording = recording
            .as_ref()
            .is_some_and(|recording| recording.active());
        info.recording_error = recording.as_ref().and_then(|recording| recording.error());
        let bridge = lock(&self.bridge);
        info.file_chooser = bridge.chooser();
        info.downloads = bridge.downloads();
        info
    }

    fn note_history(&self, history: &HistoryState) {
        let mut info = lock(&self.info);
        if !history.url.is_empty() {
            if info.url != history.url {
                lock(&self.trace).emit(
                    &self.pointers,
                    "clear",
                    (0.0, 0.0),
                    (info.width, info.height),
                );
                info.favicon_url = None;
            }
            history.url.clone_into(&mut info.url);
        }
        info.can_go_back = history.can_go_back;
        info.can_go_forward = history.can_go_forward;
    }
}

fn check_size(width: u32, height: u32) -> Result<()> {
    if SIZES.contains(&width) && SIZES.contains(&height) {
        return Ok(());
    }
    Err(CoreError::Invalid(format!(
        "A preview is {} to {} pixels each way.",
        SIZES.start(),
        SIZES.end()
    )))
}

/// How a tab starts, apart from where it goes.
#[derive(Clone, Debug, Default)]
pub struct OpenHow {
    pub private: bool,
    pub profile: Option<String>,
}

/// A page action an agent asked for, once its arguments are checked.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewAct {
    pub action: PreviewAction,
    pub selector: Option<String>,
    pub text: Option<String>,
    pub dy: f64,
}

/// The three ways an agent may drive a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewAction {
    Click,
    Type,
    Scroll,
}

impl PreviewManager {
    /// A tab a person holds, for crossings that must be theirs.
    pub(crate) fn held_tab(&self, thread_id: &str, id: &str) -> Result<Arc<Tab>> {
        let tab = self.tab(thread_id, id)?;
        if lock(&tab.info).control != "human" {
            return Err(CoreError::Invalid(
                "Take control of this page first.".into(),
            ));
        }
        Ok(tab)
    }

    pub(crate) fn tab(&self, thread_id: &str, id: &str) -> Result<Arc<Tab>> {
        lock(&self.tabs)
            .get(id)
            .filter(|tab| lock(&tab.info).thread_id == thread_id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound("preview".into()))
    }

    pub fn list(&self, thread_id: &str) -> Vec<PreviewTab> {
        lock(&self.tabs)
            .values()
            .map(|tab| tab.view())
            .filter(|tab| tab.thread_id == thread_id)
            .collect()
    }

    /// Everything a new viewer needs: the stream of frames.
    pub fn subscribe(&self, thread_id: &str, id: &str) -> Result<broadcast::Receiver<Arc<str>>> {
        let tab = self.tab(thread_id, id)?;
        let receiver = tab.frames.subscribe();
        // Register first so the requested frame cannot precede this receiver.
        tab.initial_frame_pending.store(true, Ordering::Release);
        Ok(receiver)
    }

    /// A separate event feed keeps legacy JPEG viewers compatible.
    pub fn subscribe_pointers(
        &self,
        thread_id: &str,
        id: &str,
    ) -> Result<broadcast::Receiver<PreviewPointer>> {
        Ok(self.tab(thread_id, id)?.pointers.subscribe())
    }

    /// Takes the page or hands it back.
    pub fn set_control(&self, thread_id: &str, id: &str, human: bool) -> Result<PreviewTab> {
        let tab = self.tab(thread_id, id)?;
        let word = if human { "human" } else { "none" };
        word.clone_into(&mut lock(&tab.info).control);
        Ok(tab.view())
    }

    pub async fn navigate(&self, thread_id: &str, id: &str, url: &str) -> Result<PreviewTab> {
        let url = normalize_url(url)?;
        let tab = self.tab(thread_id, id)?;
        let mut session = tab.session.lock().await;
        tab.clear_pointer();
        session.navigate(&url).await?;
        let history = session.history().await?;
        drop(session);
        {
            let mut info = lock(&tab.info);
            info.url = url;
            info.favicon_url = None;
        }
        tab.note_history(&history);
        Ok(tab.view())
    }

    /// One page back or forward. At either end this changes nothing.
    pub async fn step(&self, thread_id: &str, id: &str, back: bool) -> Result<PreviewTab> {
        let tab = self.tab(thread_id, id)?;
        let mut session = tab.session.lock().await;
        tab.clear_pointer();
        let history = session.step_history(back).await?;
        tab.note_history(&history);
        Ok(tab.view())
    }

    pub async fn resize(
        &self,
        thread_id: &str,
        id: &str,
        width: u32,
        height: u32,
    ) -> Result<PreviewTab> {
        check_size(width, height)?;
        let tab = self.tab(thread_id, id)?;
        let (zoom, mobile) = {
            let info = lock(&tab.info);
            (info.zoom, info.mobile)
        };
        let mut session = tab.session.lock().await;
        session.set_view(width, height, zoom, mobile).await?;
        tab.clear_pointer();
        {
            let mut info = lock(&tab.info);
            info.width = width;
            info.height = height;
        }
        drop(session);
        Ok(tab.view())
    }

    /// Changes how a page is shown. A value the service won't take changes nothing.
    pub async fn set_view(
        &self,
        thread_id: &str,
        id: &str,
        view: &PreviewView,
    ) -> Result<PreviewTab> {
        if view.zoom.is_some_and(|zoom| !ZOOMS.contains(&zoom)) {
            return Err(CoreError::Invalid(format!(
                "Zoom is {} to {}.",
                ZOOMS.start(),
                ZOOMS.end()
            )));
        }
        if let Some(appearance) = &view.appearance
            && !["system", "light", "dark"].contains(&appearance.as_str())
        {
            return Err(CoreError::Invalid("Unknown appearance.".into()));
        }
        let tab = self.tab(thread_id, id)?;
        let current = lock(&tab.info).clone();
        let zoom = view.zoom.unwrap_or(current.zoom);
        let mobile = view.mobile.unwrap_or(current.mobile);
        let appearance = view.appearance.clone().unwrap_or(current.appearance);
        let mut session = tab.session.lock().await;
        session
            .set_view(current.width, current.height, zoom, mobile)
            .await?;
        session.set_appearance(&appearance).await?;
        tab.clear_pointer();
        {
            let mut info = lock(&tab.info);
            info.zoom = zoom;
            info.mobile = mobile;
            info.appearance = appearance;
        }
        drop(session);
        Ok(tab.view())
    }

    /// Answers a dialog the page opened. A page blocked on one does nothing else.
    pub async fn answer_dialog(
        &self,
        thread_id: &str,
        id: &str,
        accept: bool,
        text: Option<&str>,
    ) -> Result<bool> {
        let tab = self.tab(thread_id, id)?;
        let mut session = tab.session.lock().await;
        session.answer_dialog(accept, text).await
    }

    pub async fn reload(&self, thread_id: &str, id: &str, hard: bool) -> Result<PreviewTab> {
        let tab = self.tab(thread_id, id)?;
        let mut session = tab.session.lock().await;
        tab.clear_pointer();
        session.reload(hard).await?;
        Ok(tab.view())
    }

    pub async fn clear(&self, thread_id: &str, id: &str, cookies: bool, cache: bool) -> Result<()> {
        let tab = self.tab(thread_id, id)?;
        let mut session = tab.session.lock().await;
        session.clear(cookies, cache).await
    }

    /// A person's input, which needs them to hold the page, so a watcher cannot
    /// type into it by accident.
    pub async fn input(&self, thread_id: &str, id: &str, input: &PreviewInput) -> Result<()> {
        let tab = self.tab(thread_id, id)?;
        if lock(&tab.info).control != "human" {
            return Err(CoreError::Invalid(
                "Take control of this page first.".into(),
            ));
        }
        let mut session = tab.session.lock().await;
        let document = session.document_generation();
        let recording = { lock(&tab.recording).clone() };
        let safe_key = if input.kind == "key"
            && recording
                .as_ref()
                .is_some_and(|recording| recording.keys_enabled())
        {
            session
                .isolated_read(RECORDING_SAFE_FOCUS)
                .await
                .is_ok_and(|value| value == json!(true))
        } else {
            false
        };
        let mouse = |kind: &str, buttons: u32, count: u32| {
            json!({
                "type": kind, "x": input.x, "y": input.y, "button": "left",
                "buttons": buttons, "clickCount": count,
            })
        };
        match input.kind.as_str() {
            "move" => {
                session
                    .input("Input.dispatchMouseEvent", mouse("mouseMoved", 0, 0))
                    .await
            }
            "click" => {
                session
                    .input("Input.dispatchMouseEvent", mouse("mousePressed", 1, 1))
                    .await?;
                session
                    .input("Input.dispatchMouseEvent", mouse("mouseReleased", 0, 1))
                    .await?;
                if document == session.document_generation()
                    && let Some(recording) = &recording
                {
                    let zoom = lock(&tab.info).zoom;
                    recording.mouse(input.x * zoom, input.y * zoom);
                }
                Ok(())
            }
            "scroll" => {
                session
                    .input(
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mouseWheel", "x": input.x, "y": input.y,
                            "deltaX": 0, "deltaY": input.delta_y,
                        }),
                    )
                    .await
            }
            "text" => {
                if let Some(recording) = &recording {
                    recording.key("", false);
                }
                session
                    .input("Input.insertText", json!({ "text": input.text }))
                    .await
            }
            "key" => {
                for kind in ["keyDown", "keyUp"] {
                    session
                        .input(
                            "Input.dispatchKeyEvent",
                            json!({ "type": kind, "key": input.text, "code": input.text }),
                        )
                        .await?;
                }
                if let Some(recording) = &recording {
                    recording.key(
                        &input.text,
                        safe_key && document == session.document_generation(),
                    );
                }
                Ok(())
            }
            _ => Err(CoreError::Invalid("Unknown input.".into())),
        }
    }

    pub fn start_recording(
        &self,
        thread_id: &str,
        id: &str,
        folder: PathBuf,
        options: PreviewRecordingOptions,
    ) -> Result<()> {
        let tab = self.tab(thread_id, id)?;
        let mut recording = lock(&tab.recording);
        if let Some(previous) = recording.as_ref() {
            if previous.active() {
                return Err(CoreError::Invalid("This page is already recording.".into()));
            }
            if !previous.published() {
                return Err(CoreError::Invalid(
                    "Finish the previous recording to save its summary first.".into(),
                ));
            }
        }
        *recording = Some(Recording::start(folder, options)?);
        Ok(())
    }

    pub async fn stop_recording(&self, thread_id: &str, id: &str) -> Result<PreviewRecording> {
        let tab = self.tab(thread_id, id)?;
        let finished = { lock(&tab.recording).clone() }
            .ok_or_else(|| CoreError::Invalid("This page is not recording.".into()))?;
        let result = finished.finish(None).await;
        let mut current = lock(&tab.recording);
        if result.is_ok()
            && current
                .as_ref()
                .is_some_and(|recording| Arc::ptr_eq(recording, &finished))
        {
            current.take();
        }
        result
    }

    /// Ends a tab, its browser and its profile.
    pub fn close(&self, thread_id: &str, id: &str) -> Result<()> {
        self.tab(thread_id, id)?;
        if let Some(tab) = lock(&self.tabs).remove(id) {
            forget(&tab);
        }
        Ok(())
    }

    /// Ends every tab of a thread.
    pub fn close_thread(&self, thread_id: &str) {
        lock(&self.tabs).retain(|_, tab| {
            let keep = lock(&tab.info).thread_id != thread_id;
            if !keep {
                forget(tab);
            }
            keep
        });
    }

    pub fn close_all(&self) {
        for (_, tab) in lock(&self.tabs).drain() {
            forget(&tab);
        }
    }
}

/// Stops the pump, which holds the last handle and clears up after the browser.
fn forget(tab: &Tab) {
    if let Some(recording) = lock(&tab.recording).as_ref() {
        recording.cancel();
    }
    tab.stop.cancel();
}

impl Core {
    pub fn start_preview_recording(
        &self,
        thread_id: &str,
        id: &str,
        options: PreviewRecordingOptions,
    ) -> Result<()> {
        let root = self.config.data_dir.join("browser-artifacts");
        crate::browser::preview_recording::ensure_room(
            &root,
            crate::browser::preview_recording::TOTAL_BUDGET,
        )?;
        let folder = root.join(format!("{id}-{}", new_id()));
        self.previews
            .start_recording(thread_id, id, folder, options)
    }
}

#[cfg(test)]
mod tests;
