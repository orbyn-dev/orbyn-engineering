//! Opening a tab: a page or a file, on a profile, with its first frame.

use super::crash::watch_document;
use super::favicon::{FAVICON, favicon_url};
use super::pump::pump;
use super::{DEFAULT_SIZE, MAX_TABS, OpenHow, Tab, ZOOMS, check_size, lock};
use crate::app::Core;
use crate::browser::preview_bridge::{self, Bridge};
use crate::error::{CoreError, Result};
use crate::governance::computer::cdp::{
    BrowserSession, LaunchOptions, find_browser, normalize_url,
};
use crate::store::AuditRecord;
use crate::util::new_id;
use orbyn_contracts::PreviewTab;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// A `file:` address for an absolute path, with each part escaped.
pub(super) fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let escaped: String = text
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    if escaped.starts_with('/') {
        format!("file://{escaped}")
    } else {
        format!("file:///{escaped}")
    }
}

impl Core {
    /// Opens a page. It needs a trusted folder because the usual target is a server
    /// that folder's code is running, and a browser that Orbyn can find.
    pub async fn open_preview(
        &self,
        thread_id: &str,
        url: &str,
        size: (Option<u32>, Option<u32>),
        actor: &str,
        how: OpenHow,
    ) -> Result<PreviewTab> {
        let url = normalize_url(url)?;
        self.open_page(thread_id, url, size, actor, how).await
    }

    /// Opens a file from the thread's checkout. Anywhere else reads as not found.
    pub async fn open_preview_file(
        &self,
        thread_id: &str,
        path: &str,
        actor: &str,
    ) -> Result<PreviewTab> {
        let thread = self.store.thread(thread_id)?;
        let checkout = crate::util::canonicalize(Path::new(&thread.cwd))?;
        let file = crate::util::canonicalize(&checkout.join(path))
            .map_err(|_| CoreError::NotFound("file".into()))?;
        if !file.starts_with(&checkout) || !file.is_file() {
            return Err(CoreError::NotFound("file".into()));
        }
        let url = file_url(&file);
        self.open_page(thread_id, url, (None, None), actor, OpenHow::default())
            .await
    }

    pub(super) async fn open_page(
        &self,
        thread_id: &str,
        url: String,
        size: (Option<u32>, Option<u32>),
        actor: &str,
        how: OpenHow,
    ) -> Result<PreviewTab> {
        let thread = self.store.thread(thread_id)?;
        if !self.store.project(&thread.project_id)?.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        let (width, height) = (
            size.0.unwrap_or(DEFAULT_SIZE.0),
            size.1.unwrap_or(DEFAULT_SIZE.1),
        );
        check_size(width, height)?;
        if lock(&self.previews.tabs).len() >= MAX_TABS {
            return Err(CoreError::Invalid(format!(
                "Close a preview first: {MAX_TABS} can be open at once."
            )));
        }
        let browser = self
            .config
            .browser_path
            .clone()
            .or_else(find_browser)
            .ok_or_else(|| CoreError::Unsupported("Install Chrome to use the preview.".into()))?;
        self.store.append_audit(&AuditRecord {
            actor: actor.to_owned(),
            action: "preview.open".into(),
            target: url.clone(),
            decision: None,
            source: "user".into(),
            thread_id: Some(thread_id.to_owned()),
            run_id: None,
            detail: Value::Null,
        })?;
        let id = new_id();
        let profile = match &how.profile {
            Some(name) => self.claim_profile(name)?,
            None => self.config.data_dir.join("preview-profiles").join(&id),
        };
        let zoom = self
            .preference("preview_zoom", None)?
            .as_f64()
            .filter(|zoom| ZOOMS.contains(zoom))
            .unwrap_or(1.0);
        let appearance = self
            .preference("preview_appearance", None)?
            .as_str()
            .unwrap_or("system")
            .to_owned();
        let options = LaunchOptions {
            headless: true,
            in_process_gpu: self.config.browser_in_process_gpu,
        };
        let files = self.config.data_dir.join("preview-files").join(&id);
        let mut session = BrowserSession::launch(&browser, &profile, options).await?;
        session.set_view(width, height, zoom, false).await?;
        session.set_appearance(&appearance).await?;
        preview_bridge::enable(&mut session, &files).await?;
        session.navigate(&url).await?;
        let title = session.title().await.unwrap_or_default();
        let favicon_url = session
            .read(FAVICON)
            .await
            .ok()
            .and_then(|value| favicon_url(&value, &url));
        let (frames, _) = broadcast::channel(8);
        let (pointers, _) = broadcast::channel(16);
        let tab = Arc::new(Tab {
            info: Mutex::new(PreviewTab {
                id: id.clone(),
                thread_id: thread_id.to_owned(),
                url,
                control: "none".to_owned(),
                recording: false,
                recording_error: None,
                viewers: 0,
                width,
                height,
                can_go_back: false,
                can_go_forward: false,
                title,
                favicon_url,
                zoom,
                appearance,
                mobile: false,
                private: how.private,
                profile: how.profile.clone(),
                opened_by: if actor.starts_with("engine:") {
                    "agent".to_owned()
                } else {
                    "person".to_owned()
                },
                crashed: false,
                file_chooser: None,
                downloads: Vec::new(),
            }),
            session: tokio::sync::Mutex::new(session),
            frames,
            initial_frame_pending: AtomicBool::new(false),
            pointers,
            trace: Mutex::new(crate::browser::preview_trace::Trace::default()),
            recording: Mutex::new(None),
            stop: CancellationToken::new(),
            profile,
            keep_profile: how.profile.is_some(),
            browser,
            options,
            bridge: Mutex::new(Bridge::default()),
            files,
        });
        watch_document(&mut *tab.session.lock().await, &tab);
        lock(&self.previews.tabs).insert(id, tab.clone());
        let view = tab.view();
        tokio::spawn(pump(tab));
        Ok(view)
    }
}
