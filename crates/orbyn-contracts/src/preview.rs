//! Preview browser tabs, profiles, picking and recording.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
pub struct LocalServer {
    pub port: u16,
    pub pid: u32,
    pub process: String,
    /// False where the OS will not say which folder a process runs in, as for a
    /// system service. Only a server in the checkout can be stopped.
    pub in_checkout: bool,
}
}

wire! {
pub struct PreviewTab {
    pub id: String,
    pub thread_id: String,
    pub url: String,
    /// `human` while a person holds the page, `none` otherwise.
    pub control: String,
    pub recording: bool,
    /// A recording stopped on a quota or write failure; starting again clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recording_error: Option<String>,
    pub viewers: usize,
    pub width: u32,
    pub height: u32,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub title: String,
    /// Page-declared icon; bytes must go through the bounded image proxy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub favicon_url: Option<String>,
    /// `1` is actual size.
    pub zoom: f64,
    /// `system`, `light` or `dark`.
    pub appearance: String,
    /// True emulates a phone: touch, and pages that lay out for one.
    pub mobile: bool,
    /// True keeps the page out of recent pages and history.
    pub private: bool,
    /// The named profile, or `None` for one that ends with the tab.
    pub profile: Option<String>,
    /// `person` or `agent`.
    pub opened_by: String,
    /// True once the page has crashed too often to bring back.
    pub crashed: bool,
    /// `one` or `many` while the page waits for files a person chooses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub file_chooser: Option<String>,
    /// What the page downloaded, oldest first, kept while the tab lives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<PreviewDownload>>", optional)]
    pub downloads: Vec<PreviewDownload>,
}
}

wire! {
/// A file the page downloaded into the tab's own folder.
pub struct PreviewDownload {
    pub id: String,
    /// The name the page suggested, made safe to save under.
    pub name: String,
    #[ts(type = "number")]
    pub bytes: u64,
    /// `downloading`, `done` or `stopped`.
    pub state: String,
}
}

wire! {
/// Text a person copied out of the page they hold.
pub struct PreviewCopy {
    pub text: String,
}
}

wire! {
/// A file a person chose for the page, waiting in the tab's own folder.
pub struct PreviewUpload {
    pub id: String,
}
}

wire! {
/// Answers the page's file chooser with uploaded files; none cancels it.
pub struct AnswerFileChooser {
    #[serde(default)]
    pub files: Vec<String>,
}
}

wire! {
/// A content-free cue from a successful agent input; coordinates are frame pixels.
pub struct PreviewPointer {
    pub sequence: u32,
    /// `move`, `click`, or `clear`.
    pub phase: String,
    pub x: f64,
    pub y: f64,
    pub width: u32,
    pub height: u32,
}
}

wire! {
pub struct OpenPreview {
    pub url: String,
    #[serde(default)]
    pub private: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub height: Option<u32>,
}
}

wire! {
pub struct NavigatePreview {
    pub url: String,
}
}

wire! {
pub struct OpenPreviewFile {
    /// Inside the thread's checkout.
    pub path: String,
}
}

wire! {
/// How a page is shown. A field left out stays as it is.
pub struct PreviewView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub zoom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub appearance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mobile: Option<bool>,
}
}

wire! {
pub struct ReloadPreview {
    /// True skips the cache.
    #[serde(default)]
    pub hard: bool,
}
}

wire! {
pub struct ClearPreview {
    #[serde(default)]
    pub cookies: bool,
    #[serde(default)]
    pub cache: bool,
}
}

wire! {
pub struct PickPreview {
    pub x: f64,
    pub y: f64,
}
}

wire! {
/// An element a person pointed at. All of it comes from the page.
pub struct PickedElement {
    pub tag: String,
    pub selector: String,
    pub text: String,
    pub html: String,
}
}

wire! {
pub struct BrowserProfile {
    pub id: String,
    pub name: String,
    /// True while a tab has it open.
    pub in_use: bool,
}
}

wire! {
pub struct CreateBrowserProfile {
    pub name: String,
}
}

wire! {
pub struct StepPreview {
    /// True goes back; false goes forward.
    pub back: bool,
}
}

wire! {
pub struct ResizePreview {
    pub width: u32,
    pub height: u32,
}
}

wire! {
pub struct PreviewControl {
    /// True takes the page; false hands it back.
    pub human: bool,
}
}

wire! {
pub struct PreviewInput {
    /// `click`, `move`, `scroll`, `key` or `text`.
    pub kind: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub delta_y: f64,
    /// A key name for `key`, or the text for `text`.
    #[serde(default)]
    pub text: String,
}
}

wire! {
/// Recording overlays are opt-in; FPS is a requested maximum, not a timing guarantee.
#[serde(default)]
pub struct PreviewRecordingOptions {
    pub fps: u32,
    pub show_key_presses: bool,
    pub show_mouse_presses: bool,
}
}

impl Default for PreviewRecordingOptions {
    fn default() -> Self {
        Self {
            fps: 5,
            show_key_presses: false,
            show_mouse_presses: false,
        }
    }
}

wire! {
pub struct PreviewRecording {
    pub path: String,
    pub frames: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}
}
