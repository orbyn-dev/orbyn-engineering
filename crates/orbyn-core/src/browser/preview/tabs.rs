//! Tab control: who holds it, where it goes, how it looks, input, recordings and closing.

use super::{PreviewManager, RECORDING_SAFE_FOCUS, ZOOMS, check_size};
use super::{Tab, lock};
use crate::browser::preview_recording::Recording;
use crate::error::{CoreError, Result};
use crate::governance::computer::cdp::normalize_url;
use orbyn_contracts::{
    PreviewInput, PreviewPointer, PreviewRecording, PreviewRecordingOptions, PreviewTab,
    PreviewView,
};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::broadcast;

/// Stops the pump, which holds the last handle and clears up after the browser.
pub(super) fn forget(tab: &Tab) {
    if let Some(recording) = lock(&tab.recording).as_ref() {
        recording.cancel();
    }
    tab.stop.cancel();
}

impl PreviewManager {
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
