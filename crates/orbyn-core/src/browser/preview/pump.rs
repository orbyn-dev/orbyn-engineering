//! The frame loop: polls a tab while anyone watches or records.

use super::crash::{Crashes, bring_back};
use super::favicon::{FAVICON, favicon_url};
use super::lock;
use super::{FRAME_EVERY, FRAME_QUALITY, HISTORY_TICKS, PROFILE_GRACE, RECORDING_SAFE_FOCUS, Tab};
use crate::browser::preview_bridge;
use crate::browser::preview_recording::MAX_JPEG;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

pub(super) fn decode_base64(text: &str) -> Vec<u8> {
    let value = |byte: u8| match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut bits, mut count) = (0u32, 0u8);
    for six in text.bytes().filter_map(value) {
        bits = (bits << 6) | u32::from(six);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    out
}

async fn refresh_metadata(tab: &Tab) {
    let mut session = tab.session.lock().await;
    let history = session.history().await;
    let title = session.title().await;
    let icon = session.read(FAVICON).await;
    if let Ok(history) = history {
        tab.note_history(&history);
    }
    if let Ok(title) = title {
        lock(&tab.info).title = title;
    }
    if let Ok(icon) = icon {
        let mut info = lock(&tab.info);
        info.favicon_url = favicon_url(&icon, &info.url);
    }
}

/// Polls frames while anyone wants them. A page that did not change sends nothing,
/// and a tab that stops answering just skips that frame.
pub(super) async fn pump(tab: Arc<Tab>) {
    let mut last = Vec::new();
    let mut ticks = tokio::time::interval(Duration::from_millis(100));
    let mut since_history = Instant::now();
    let mut last_view = Instant::now() - FRAME_EVERY;
    let mut crashes = Crashes::default();
    'frames: loop {
        tokio::select! {
            () = tab.stop.cancelled() => {
                let recording = { lock(&tab.recording).take() };
                if let Some(recording) = recording
                    && let Err(error) = recording.finish(Some("The tab closed while recording.")).await
                {
                    tracing::warn!(%error, "could not finalize a closed tab's recording");
                }
                let (profile, keep) = (tab.profile.clone(), tab.keep_profile);
                let files = tab.files.clone();
                drop(tab);
                tokio::time::sleep(PROFILE_GRACE).await;
                if !keep { let _ = std::fs::remove_dir_all(profile); }
                let _ = std::fs::remove_dir_all(files);
                return;
            }
            _ = ticks.tick() => {}
        }
        let recording = { lock(&tab.recording).clone() };
        let record_due = match &recording {
            Some(recording) => recording.due().await,
            None => false,
        };
        let viewers = tab.frames.receiver_count();
        // Consume before capture: a subscription during capture stays pending
        // for the next tick, even when the viewer count does not change.
        if tab.initial_frame_pending.swap(false, Ordering::AcqRel) {
            last.clear();
        }
        let view_due = viewers > 0 && last_view.elapsed() >= FRAME_EVERY;
        if !view_due && !record_due {
            continue;
        }
        if since_history.elapsed() >= FRAME_EVERY * HISTORY_TICKS {
            since_history = Instant::now();
            tokio::select! {
                biased;
                () = tab.stop.cancelled() => continue,
                () = refresh_metadata(&tab) => {}
            }
        }
        // Capture owns its reply slot, so metadata and input can use the session
        // while the browser renders. Close cancels that slot before releasing Tab.
        let prepare = async {
            let mut session = tab.session.lock().await;
            let request = if record_due && let Some(recording) = &recording {
                let safe = !recording.keys_enabled()
                    || session
                        .isolated_read(RECORDING_SAFE_FOCUS)
                        .await
                        .is_ok_and(|value| value == json!(true));
                Some(recording.begin_capture(session.document_generation(), safe))
            } else {
                None
            };
            (session.pending_frame(FRAME_QUALITY), request)
        };
        let (capture, recording_request) = tokio::select! {
            biased;
            () = tab.stop.cancelled() => continue 'frames,
            prepared = prepare => prepared,
        };
        let capture = async move { (capture.await, recording_request) };
        tokio::pin!(capture);
        let (frame, recording_request) = loop {
            tokio::select! {
                biased;
                () = tab.stop.cancelled() => continue 'frames,
                frame = &mut capture => break frame,
                // Reuse the pump's existing ticks: metadata keeps its cadence
                // even if this capture will never produce a reply.
                _ = ticks.tick() => {
                    if since_history.elapsed() >= FRAME_EVERY * HISTORY_TICKS {
                        since_history = Instant::now();
                        tokio::select! {
                            biased;
                            () = tab.stop.cancelled() => continue 'frames,
                            frame = &mut capture => break frame,
                            () = refresh_metadata(&tab) => {}
                        }
                    }
                }
            }
        };
        let complete = async {
            let mut session = tab.session.lock().await;
            let captured = if let (Ok(frame), Some(request)) = (&frame, recording_request) {
                let safe = if recording
                    .as_ref()
                    .is_some_and(|recording| recording.keys_enabled())
                {
                    session
                        .isolated_read(RECORDING_SAFE_FOCUS)
                        .await
                        .is_ok_and(|value| value == json!(true))
                } else {
                    true
                };
                // Reject the encoded size before allocating the decoded byte vector.
                let jpeg = (frame.len() <= (MAX_JPEG / 3 + 1) * 4).then(|| decode_base64(frame));
                let current = lock(&tab.recording).clone();
                request.finish(jpeg, session.document_generation(), safe, current.as_ref())
            } else {
                None
            };
            (frame, captured)
        };
        let (frame, captured) = tokio::select! {
            biased;
            () = tab.stop.cancelled() => continue 'frames,
            completed = complete => completed,
        };
        let Ok(frame) = frame else {
            if !tab.session.lock().await.is_alive().await {
                bring_back(&tab, &mut crashes).await;
            }
            continue;
        };
        preview_bridge::settle(&tab.bridge, &mut *tab.session.lock().await, &tab.files).await;
        if let Some(captured) = captured {
            // Normal frames hand over without waiting on encoding.
            captured.submit().await;
        }
        if view_due {
            last_view = Instant::now();
            let digest = Sha256::digest(frame.as_bytes()).to_vec();
            if digest != last {
                last = digest;
                let _ = tab.frames.send(Arc::from(frame));
            }
        }
    }
}
