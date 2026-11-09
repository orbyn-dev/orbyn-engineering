//! The page a person watches beside a thread, and the servers a thread started. A
//! tab is a headless Chrome of its own, on a profile of its own, separate from the
//! governed browser agents use. Frames are polled and go out only while someone
//! watches or records. This is local: a client watches over the same access as the
//! rest of the API, and nothing here reaches another machine.

mod tabs;

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
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use orbyn_contracts::{PreviewPointer, PreviewTab};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use crate::browser::preview_bridge::Bridge;
use crate::browser::preview_recording::Recording;
use crate::error::{CoreError, Result};
use crate::governance::computer::cdp::{BrowserSession, HistoryState, LaunchOptions};
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
}

#[cfg(test)]
mod tests;
