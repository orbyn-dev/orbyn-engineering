//! The page a person watches beside a thread, and the servers a thread started. A
//! tab is a headless Chrome of its own, on a profile of its own, separate from the
//! governed browser agents use. Frames are polled and go out only while someone
//! watches or records. This is local: a client watches over the same access as the
//! rest of the API, and nothing here reaches another machine.

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
use std::time::{Duration, Instant};

use orbyn_contracts::{
    PreviewInput, PreviewPointer, PreviewRecording, PreviewRecordingOptions, PreviewTab,
    PreviewView,
};
use serde_json::{Value, json};
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
/// What an agent may ask a page to do at once: one typed string, one wheel of a
/// sane size, one expression, and one waited-for string.
pub(crate) const MAX_TYPE_CHARS: usize = 4_000;
pub(crate) const DEFAULT_SCROLL: f64 = 400.0;
/// Far more than a page can scroll, so a wheel event stays a wheel event.
pub(crate) const MAX_SCROLL: f64 = 100_000.0;
const MAX_EXPRESSION_CHARS: usize = 2_000;
const MAX_RESULT_BYTES: usize = 20_000;
/// A wait polls 150 ms apart and gives up inside the 45 seconds a tool has to
/// answer, with the round count holding even if the clock stalls.
const DEFAULT_WAIT_MS: u64 = 5_000;
const MAX_WAIT_MS: u64 = 20_000;
pub(crate) const MAX_WAIT_TEXT_CHARS: usize = 200;
const WAIT_POLL: Duration = Duration::from_millis(150);
const WAIT_ROUNDS: u32 = 140;

/// Focuses one element and puts the caret at its end, so the text an agent types
/// appends to what the field already holds.
const FOCUS_ELEMENT: &str = r"(() => {
  const el = document.querySelector(__SELECTOR__);
  if (!el) return false;
  el.focus();
  if (typeof el.setSelectionRange === 'function' && typeof el.value === 'string') {
    try { el.setSelectionRange(el.value.length, el.value.length); } catch (unusable) {}
  }
  return document.activeElement === el;
})()";

/// The point a wheel event goes to: an element's centre after it is brought into
/// view, or the middle of the viewport when the page is the target.
const SCROLL_POINT: &str = r"(() => {
  const selector = __SELECTOR__;
  let target = null;
  if (selector !== null) {
    target = document.querySelector(selector);
    if (!target) return null;
    target.scrollIntoView({ block: 'center' });
  }
  const box = target
    ? target.getBoundingClientRect()
    : { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight };
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
})()";

/// A wait polls one boolean, so each look is a round trip of about a word.
const WAIT_SELECTOR: &str = "document.querySelector(__SELECTOR__) !== null";
const WAIT_TEXT: &str =
    "(() => (document.body ? document.body.innerText : '').includes(__TEXT__))()";
/// A selector the page cannot parse is refused once, before the wait starts.
const WAIT_SELECTOR_PARSES: &str = r"(() => {
  try { document.querySelector(__SELECTOR__); return true; } catch (unreadable) { return false; }
})()";

/// A wait's length: five seconds unless asked for more, never past the cap.
fn wait_timeout(asked: Option<u64>) -> Duration {
    Duration::from_millis(asked.unwrap_or(DEFAULT_WAIT_MS).clamp(1, MAX_WAIT_MS))
}

/// What an agent gets from an expression: the whole value, or only its size,
/// never a value cut in half.
fn bounded_result(value: Value) -> Value {
    let bytes = serde_json::to_string(&value).map_or(usize::MAX, |text| text.len());
    if bytes > MAX_RESULT_BYTES {
        return json!({ "truncated": true, "bytes": bytes });
    }
    json!({ "value": value, "truncated": false })
}

/// An agent's expression, checked against the cap before the page runs it.
fn checked_expression(expression: &str) -> Result<&str> {
    if expression.chars().count() > MAX_EXPRESSION_CHARS {
        return Err(CoreError::Invalid(format!(
            "An expression is at most {MAX_EXPRESSION_CHARS} characters."
        )));
    }
    Ok(expression)
}

/// An expression that focuses one element, with the selector quoted as data.
fn focus_expression(selector: &str) -> String {
    FOCUS_ELEMENT.replace(
        "__SELECTOR__",
        &Value::String(selector.to_owned()).to_string(),
    )
}

/// An expression for the point a wheel event goes to, for one element or all.
fn scroll_expression(selector: Option<&str>) -> String {
    let target = selector.map_or_else(
        || "null".to_owned(),
        |sel| Value::String(sel.to_owned()).to_string(),
    );
    SCROLL_POINT.replace("__SELECTOR__", &target)
}

/// An expression that is true once a selector matches.
fn wait_selector_expression(selector: &str) -> String {
    WAIT_SELECTOR.replace(
        "__SELECTOR__",
        &Value::String(selector.to_owned()).to_string(),
    )
}

/// An expression that is true once the page's text holds a string.
fn wait_text_expression(text: &str) -> String {
    WAIT_TEXT.replace("__TEXT__", &Value::String(text.to_owned()).to_string())
}

/// An expression that is true when the page can parse a selector.
fn parses_expression(selector: &str) -> String {
    WAIT_SELECTOR_PARSES.replace(
        "__SELECTOR__",
        &Value::String(selector.to_owned()).to_string(),
    )
}

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

    /// Clicks one element, types into one field or turns the wheel once, so an
    /// agent drives the page no further than the one action it named.
    pub async fn act(&self, thread_id: &str, id: &str, step: &PreviewAct) -> Result<Value> {
        if step.action != PreviewAction::Scroll && step.selector.is_none() {
            return Err(CoreError::Invalid(
                "Name the selector of the element to act on.".into(),
            ));
        }
        let tab = self.tab(thread_id, id)?;
        let mut session = tab.session.lock().await;
        let document = session.document_generation();
        match step.action {
            PreviewAction::Click => {
                let selector = step.selector.as_deref().unwrap_or_default();
                let (x, y) = session.click_point(selector).await?;
                if document == session.document_generation() {
                    tab.pointer("click", x, y);
                }
                Ok(json!({ "clicked": selector }))
            }
            PreviewAction::Type => {
                let selector = step.selector.as_deref().unwrap_or_default();
                if session.read(&focus_expression(selector)).await? != json!(true) {
                    return Err(CoreError::Invalid(format!(
                        "Nothing to type into matches {selector}."
                    )));
                }
                let typed = step.text.as_deref().unwrap_or_default();
                if let Some(recording) = lock(&tab.recording).as_ref() {
                    recording.key("", false);
                }
                session
                    .input("Input.insertText", json!({ "text": typed }))
                    .await?;
                Ok(json!({ "typed": typed }))
            }
            PreviewAction::Scroll => {
                let point = session
                    .read(&scroll_expression(step.selector.as_deref()))
                    .await?;
                let place = |key: &str| point.get(key).and_then(Value::as_f64);
                let (Some(x), Some(y)) = (place("x"), place("y")) else {
                    return Err(CoreError::Invalid(format!(
                        "Nothing to scroll matches {}.",
                        step.selector.as_deref().unwrap_or("this page")
                    )));
                };
                session
                    .input(
                        "Input.dispatchMouseEvent",
                        json!({
                            "type": "mouseWheel", "x": x, "y": y,
                            "deltaX": 0, "deltaY": step.dy,
                        }),
                    )
                    .await?;
                if document == session.document_generation() {
                    tab.pointer("move", x, y);
                }
                Ok(json!({ "scrolled": step.dy }))
            }
        }
    }

    /// Runs an agent's expression in the page and answers it whole, or only its
    /// size when the value is too big to pass on.
    pub async fn evaluate(&self, thread_id: &str, id: &str, expression: &str) -> Result<Value> {
        let expression = checked_expression(expression)?;
        let tab = self.tab(thread_id, id)?;
        let value = tab.session.lock().await.read(expression).await?;
        Ok(bounded_result(value))
    }

    /// Waits for a selector or a string to show up. Each poll takes the session for
    /// one boolean, so frames keep coming and other tools still run.
    pub async fn wait_for(
        &self,
        thread_id: &str,
        id: &str,
        selector: Option<&str>,
        wanted: Option<&str>,
        timeout_ms: Option<u64>,
    ) -> Result<Value> {
        let tab = self.tab(thread_id, id)?;
        let timeout = wait_timeout(timeout_ms);
        let started = Instant::now();
        let waited = || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        // Include session contention and CDP reads, not only the delays between polls.
        let outcome = tokio::time::timeout(timeout, async {
            let expression = match (selector, wanted) {
                (Some(selector), None) => {
                    if tab
                        .session
                        .lock()
                        .await
                        .read(&parses_expression(selector))
                        .await?
                        != json!(true)
                    {
                        return Err(CoreError::Invalid(format!(
                            "{selector} is not a selector a page can read."
                        )));
                    }
                    wait_selector_expression(selector)
                }
                (None, Some(wanted)) => wait_text_expression(wanted),
                _ => {
                    return Err(CoreError::Invalid(
                        "Name a selector or text to wait for, not both.".into(),
                    ));
                }
            };
            for _ in 0..WAIT_ROUNDS {
                let found = tab.session.lock().await.read(&expression).await;
                if found.as_ref().is_ok_and(|value| value == &json!(true)) {
                    return Ok(json!({ "found": true, "waited_ms": waited() }));
                }
                // A page that navigated mid-poll answers an error, which is a miss.
                tokio::time::sleep(WAIT_POLL).await;
            }
            Ok(json!({ "found": false, "waited_ms": waited() }))
        })
        .await;
        outcome.unwrap_or_else(|_| Ok(json!({ "found": false, "waited_ms": waited() })))
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
