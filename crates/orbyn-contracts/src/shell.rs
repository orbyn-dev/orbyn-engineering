//! Shell integration events, actions and status.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire! {
/// What the desktop shell tells the windows. A link or menu item only ever moves
/// the UI; nothing here can start work.
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellEvent {
    /// Go to this route, a `#/...` address.
    Navigate { href: String },
    /// Run this keybinding command, as if its keys were pressed.
    Command { command: String },
    /// `ShellStatus` has changed.
    Changed,
    /// Something a window asked for could not be done.
    Failed { message: String },
    /// Show this preview tab, which the service opened for a web link from another app.
    ShowPreview { thread_id: String, tab_id: String },
}
}

wire_enum! {
/// What a window may ask the desktop shell to do. The list is closed: no path, URL
/// or command comes through here.
pub enum ShellAction {
    ZoomIn,
    ZoomOut,
    ZoomReset,
    InstallCli,
    RemoveCli,
    OpenAccessibilitySettings,
    OpenScreenRecordingSettings,
    /// Quits the app, which stops its service and runs; sent once a window's quit
    /// confirmation is met.
    Quit,
    /// Starts the local service again, as the startup page's Try again does. Runs it
    /// held are interrupted and picked up as after any restart.
    RestartService,
    /// Shows the service's log file in the file manager.
    ShowLog,
}
}

wire! {
pub struct ShellCli {
    /// Where the `orbyn` command goes, or is.
    pub path: String,
    pub installed: bool,
}
}

wire! {
pub struct ShellPermission {
    /// `accessibility` or `screen_recording`.
    pub id: String,
    pub granted: bool,
}
}

wire! {
/// What only the shell can see. Empty until it has said.
#[derive(Default)]
pub struct ShellFacts {
    #[serde(default)]
    pub cli: Option<ShellCli>,
    #[serde(default)]
    pub permissions: Vec<ShellPermission>,
}
}

wire! {
pub struct ShellRequest {
    pub action: ShellAction,
}
}

wire! {
pub struct ShellStatus {
    /// True when a desktop shell started this service.
    pub attached: bool,
    /// How many windows are following shell events.
    pub windows: usize,
    pub facts: ShellFacts,
}
}
