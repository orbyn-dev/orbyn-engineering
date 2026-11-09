//! A crashed page: how many tries, how long to wait, and starting the browser again.

use super::{Tab, lock};
use crate::browser::preview_bridge::{self};
use crate::error::CoreError;
use crate::governance::computer::cdp::BrowserSession;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How many crashes are put right, and over what time.
pub(super) const CRASH_LIMIT: usize = 3;

pub(super) const CRASH_WINDOW: Duration = Duration::from_secs(30);

pub(super) const CRASH_DELAY: Duration = Duration::from_secs(1);

/// Crashes lately, which decide whether and when to try again.
#[derive(Default)]
pub(super) struct Crashes(Vec<Instant>);

impl Crashes {
    /// How long to wait before starting again, growing each time, or `None` when
    /// this page keeps crashing and should be left.
    pub(super) fn next_delay(&mut self, now: Instant) -> Option<Duration> {
        self.0.retain(|at| now.duration_since(*at) < CRASH_WINDOW);
        if self.0.len() >= CRASH_LIMIT {
            return None;
        }
        self.0.push(now);
        Some(CRASH_DELAY * 2u32.pow(u32::try_from(self.0.len() - 1).unwrap_or(0)))
    }
}

/// Starts the browser again on the same profile and page.
pub(super) async fn bring_back(tab: &Arc<Tab>, crashes: &mut Crashes) {
    tab.clear_pointer();
    let recording = { lock(&tab.recording).clone() };
    if let Some(recording) = recording {
        recording.fail("The browser stopped while recording.").await;
    }
    let Some(delay) = crashes.next_delay(Instant::now()) else {
        lock(&tab.info).crashed = true;
        return;
    };
    tokio::time::sleep(delay).await;
    let view = lock(&tab.info).clone();
    let started = async {
        let mut session = BrowserSession::launch(&tab.browser, &tab.profile, tab.options).await?;
        watch_document(&mut session, tab);
        session
            .set_view(view.width, view.height, view.zoom, view.mobile)
            .await?;
        session.set_appearance(&view.appearance).await?;
        preview_bridge::enable(&mut session, &tab.files).await?;
        session.navigate(&view.url).await?;
        Ok::<_, CoreError>(session)
    };
    match started.await {
        Ok(session) => *tab.session.lock().await = session,
        Err(error) => tracing::warn!(%error, "could not bring a preview back"),
    }
}

pub(super) fn watch_document(session: &mut BrowserSession, tab: &Arc<Tab>) {
    let tab = Arc::downgrade(tab);
    session.on_document_change(move || {
        if let Some(tab) = tab.upgrade() {
            tab.clear_pointer();
        }
    });
}
