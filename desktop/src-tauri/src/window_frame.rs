//! The web title bar's window actions. On Windows the bar draws Minimize, Maximize and Close and a
//! Menu button, so the shell runs those items and tracks where the Maximize button is drawn, for the
//! overlay window that answers Windows' hit test there and offers its snap layouts
//! (`crate::snap_overlay`). The rectangle logic is plain Rust, tested on every OS.

use serde::Deserialize;
use tauri::{AppHandle, WebviewWindow};

const FORBIDDEN: &str = "forbidden";
const UNKNOWN: &str = "unknown";
const INVALID: &str = "invalid";
const FAILED: &str = "failed";

/// A rectangle in device pixels from the window's client top-left: the page's CSS pixels times its devicePixelRatio,
/// so the window's zoom is already in it.
#[derive(Debug, PartialEq, Clone, Copy, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// A rectangle that can be scaled: finite, with no negative side.
    pub(crate) fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

/// What a web Menu button id does.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MenuAction {
    /// An item `menu::run` already handles.
    Menu(&'static str),
    /// A window action the native controls used to own.
    Window(WindowOp),
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WindowOp {
    Minimize,
    Maximize,
    Fullscreen,
}

/// Maps a web Menu button id to an allowed action. Every other id is refused.
pub fn menu_action(id: &str) -> Option<MenuAction> {
    match id {
        "new-thread" => Some(MenuAction::Menu("new-thread")),
        "settings" => Some(MenuAction::Menu("settings")),
        "check-updates" => Some(MenuAction::Menu("check-updates")),
        "close-window" => Some(MenuAction::Menu("close-window")),
        "paste-text" => Some(MenuAction::Menu("paste-text")),
        "toggle-sidebar" => Some(MenuAction::Menu("toggle-sidebar")),
        "palette" => Some(MenuAction::Menu("palette")),
        "reload" => Some(MenuAction::Menu("reload")),
        "force-reload" => Some(MenuAction::Menu("force-reload")),
        "zoom-reset" => Some(MenuAction::Menu("zoom-reset")),
        "zoom-in" => Some(MenuAction::Menu("zoom-in")),
        "zoom-out" => Some(MenuAction::Menu("zoom-out")),
        "minimize" => Some(MenuAction::Window(WindowOp::Minimize)),
        "maximize" => Some(MenuAction::Window(WindowOp::Maximize)),
        "fullscreen" => Some(MenuAction::Window(WindowOp::Fullscreen)),
        _ => None,
    }
}

/// Runs one item from the web Menu button.
#[tauri::command]
pub fn window_menu_run(
    app: AppHandle,
    window: WebviewWindow,
    id: String,
) -> Result<(), &'static str> {
    crate::presence::require_caller(&window).map_err(|_| FORBIDDEN)?;
    match menu_action(&id).ok_or(UNKNOWN)? {
        MenuAction::Menu(id) => {
            crate::menu::run(&app, id);
            Ok(())
        }
        MenuAction::Window(WindowOp::Minimize) => window.minimize().map_err(|_| FAILED),
        MenuAction::Window(WindowOp::Maximize) => {
            if window.is_maximized().map_err(|_| FAILED)? {
                window.unmaximize().map_err(|_| FAILED)
            } else {
                window.maximize().map_err(|_| FAILED)
            }
        }
        MenuAction::Window(WindowOp::Fullscreen) => {
            let fullscreen = window.is_fullscreen().map_err(|_| FAILED)?;
            window.set_fullscreen(!fullscreen).map_err(|_| FAILED)
        }
    }
}

/// Records where the page drew the Maximize button, or clears it with `None`. The shell puts its
/// snap-layout window there, so the flyout is offered where the button is.
#[tauri::command]
pub fn window_set_maximize_rect(
    window: WebviewWindow,
    rect: Option<Rect>,
) -> Result<(), &'static str> {
    crate::presence::require_caller(&window).map_err(|_| FORBIDDEN)?;
    if rect.is_some_and(|rect| !rect.is_valid()) {
        return Err(INVALID);
    }
    #[cfg(windows)]
    crate::snap_overlay::set_bounds(&window, rect);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect {
            x: 1000.0,
            y: 8.0,
            width: 46.0,
            height: 32.0,
        }
    }

    #[test]
    fn the_menu_allowlist_is_exact() {
        for id in [
            "new-thread",
            "settings",
            "check-updates",
            "close-window",
            "paste-text",
            "toggle-sidebar",
            "palette",
            "reload",
            "force-reload",
            "zoom-reset",
            "zoom-in",
            "zoom-out",
        ] {
            assert_eq!(menu_action(id), Some(MenuAction::Menu(id)), "{id}");
        }
        for (id, op) in [
            ("minimize", WindowOp::Minimize),
            ("maximize", WindowOp::Maximize),
            ("fullscreen", WindowOp::Fullscreen),
        ] {
            assert_eq!(menu_action(id), Some(MenuAction::Window(op)), "{id}");
        }
        for id in ["quit", "devtools", "", "../x", "minimize-window", "quit "] {
            assert_eq!(menu_action(id), None, "{id}");
        }
    }

    #[test]
    fn a_maximize_rectangle_is_finite_and_has_no_negative_side() {
        assert!(rect().is_valid());
        for bad in [
            Rect {
                x: f64::NAN,
                ..rect()
            },
            Rect {
                y: f64::INFINITY,
                ..rect()
            },
            Rect {
                width: f64::NAN,
                ..rect()
            },
            Rect {
                height: f64::NEG_INFINITY,
                ..rect()
            },
            Rect {
                width: -1.0,
                ..rect()
            },
            Rect {
                height: -0.5,
                ..rect()
            },
        ] {
            assert!(!bad.is_valid(), "{bad:?}");
        }
        assert!(
            Rect {
                width: 0.0,
                height: 0.0,
                ..rect()
            }
            .is_valid()
        );
    }
}
