//! Shared state for every handler.

use std::path::PathBuf;
use std::sync::Arc;

use orbyn_core::Core;

use crate::http::access::Access;

/// The actor recorded for anything arriving over the local API.
pub const LOCAL_USER: &str = "user";

#[derive(Clone)]
pub struct AppState {
    pub images: Arc<orbyn_core::browser::remote_images::ImageProxy>,
    pub core: Arc<Core>,
    pub access: Arc<Access>,
    pub remote: bool,
    pub ui_dir: Option<PathBuf>,
}
