//! The web title bar's window actions. On Windows the bar draws Minimize, Maximize and Close and a
//! Menu button, so the shell runs those items, tracks where the Maximize button is and answers the
//! window's hit test there. The rectangle and allowlist logic is plain Rust, tested on every OS.

use serde::Deserialize;
use std::sync::Mutex;
use tauri::{AppHandle, WebviewWindow};

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{HTMAXBUTTON, WM_NCDESTROY, WM_NCHITTEST},
    },
};

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
    fn is_valid(&self) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite())
            && self.width >= 0.0
            && self.height >= 0.0
    }
}

/// What the mouse is over in the client area.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Hit {
    Client,
    MaximizeButton,
}

/// Where the page last drew the Maximize button. `None` means there is no button to hit.
#[derive(Default)]
pub struct MaximizeRect(Mutex<Option<Rect>>);

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

/// Whether a physical point is over the Maximize button. The rectangle is scaled by `scale`
/// (1.0 for the page's device-pixel rectangle); the left and top edges are inside it and the right and bottom edges are not.
// Only the Windows hook reads the hit test; the tests exercise it on every OS.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn hit_test(point_physical: (i32, i32), scale: f64, maximize: Option<&Rect>) -> Hit {
    let Some(rect) = maximize else {
        return Hit::Client;
    };
    if !scale.is_finite() || scale <= 0.0 || !rect.is_valid() {
        return Hit::Client;
    }
    let (x, y) = (f64::from(point_physical.0), f64::from(point_physical.1));
    let left = rect.x * scale;
    let top = rect.y * scale;
    let right = (rect.x + rect.width) * scale;
    let bottom = (rect.y + rect.height) * scale;
    if x >= left && x < right && y >= top && y < bottom {
        Hit::MaximizeButton
    } else {
        Hit::Client
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

/// Records where the page drew the Maximize button, or clears it with `None`.
#[tauri::command]
pub fn window_set_maximize_rect(
    window: WebviewWindow,
    state: tauri::State<'_, MaximizeRect>,
    rect: Option<Rect>,
) -> Result<(), &'static str> {
    crate::presence::require_caller(&window).map_err(|_| FORBIDDEN)?;
    if rect.is_some_and(|rect| !rect.is_valid()) {
        return Err(INVALID);
    }
    *state.0.lock().expect("maximize rect") = rect;
    Ok(())
}

#[cfg(windows)]
const SUBCLASS_ID: usize = 1;

/// What the window procedure keeps after `install_hit_test` returns. The rectangle is managed
/// state for the app's life, and a callback only runs while the window behind it exists.
#[cfg(windows)]
struct Context {
    maximize: *const MaximizeRect,
}

/// Answers Windows' non-client hit test with `HTMAXBUTTON` over the page's Maximize button, so
/// Windows can offer its snap layouts there. Best effort only: WebView2's child window may take
/// the mouse before the parent sees `WM_NCHITTEST`, so the snap flyout is NOT observed; the page's
/// Maximize button still works by click either way.
#[cfg(windows)]
pub fn install_hit_test(window: &tauri::WebviewWindow, state: tauri::State<'_, MaximizeRect>) {
    let Ok(handle) = window.hwnd() else {
        return;
    };
    let hwnd: HWND = handle.0;
    let context = Box::into_raw(Box::new(Context {
        maximize: std::ptr::from_ref(state.inner()),
    }));
    // SAFETY: `hwnd` is this window's live top-level handle; `context` stays valid until the
    // subclass frees it on WM_NCDESTROY; the id is used by this hook alone.
    let installed =
        unsafe { SetWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID, context as usize) };
    if installed == 0 {
        // SAFETY: the subclass was not installed, so nothing but this call owns the box.
        unsafe { drop(Box::from_raw(context)) };
    }
}

/// The top-level window's subclass. `data` is the box `install_hit_test` made.
#[cfg(windows)]
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    if message == WM_NCDESTROY {
        // SAFETY: removes this subclass from the window that is going away.
        unsafe { RemoveWindowSubclass(hwnd, Some(window_proc), SUBCLASS_ID) };
        // SAFETY: `data` is the box `install_hit_test` leaked, and this is the one place it is freed.
        drop(unsafe { Box::from_raw(data as *mut Context) });
        // SAFETY: passes the message to the next handler in the chain.
        return unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
    }
    if message == WM_NCHITTEST {
        // SAFETY: `data` still names the box until WM_NCDESTROY above.
        let context = unsafe { &*(data as *const Context) };
        // GET_X_LPARAM and GET_Y_LPARAM: signed 16-bit halves of the screen position.
        let x = lparam as i16 as i32;
        let y = (lparam >> 16) as i16 as i32;
        let mut point = POINT { x, y };
        // SAFETY: `hwnd` is live for this message and `point` is ours to read and write.
        if unsafe { ScreenToClient(hwnd, &mut point) } != 0 {
            // SAFETY: the managed rectangle outlives this window, and WM_NCDESTROY above is the
            // last message that may read it.
            let maximize = unsafe { &*context.maximize };
            let maximize = maximize.0.lock().ok().and_then(|rect| *rect);
            if hit_test((point.x, point.y), 1.0, maximize.as_ref()) == Hit::MaximizeButton {
                return HTMAXBUTTON as LRESULT;
            }
        }
    }
    // SAFETY: passes the message to the next handler in the chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
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
    fn the_maximize_button_takes_the_left_and_top_edges_but_not_the_right_and_bottom() {
        let rect = rect();
        let hit = |x, y| hit_test((x, y), 1.0, Some(&rect));
        assert_eq!(hit(1023, 24), Hit::MaximizeButton);
        assert_eq!(hit(1000, 24), Hit::MaximizeButton);
        assert_eq!(hit(1023, 8), Hit::MaximizeButton);
        assert_eq!(hit(1046, 24), Hit::Client);
        assert_eq!(hit(1023, 40), Hit::Client);
        assert_eq!(hit(999, 24), Hit::Client);
        assert_eq!(hit(1023, 7), Hit::Client);
        assert_eq!(hit(1047, 24), Hit::Client);
        assert_eq!(hit(1023, 41), Hit::Client);
        assert_eq!(hit_test((1023, 24), 1.0, None), Hit::Client);
        assert_eq!(hit_test((1023, 24), f64::NAN, Some(&rect)), Hit::Client);
    }

    #[test]
    fn the_maximize_button_scales_with_the_monitor() {
        let rect = rect();
        let hit = |x, y, scale| hit_test((x, y), scale, Some(&rect));
        // 125%: 1250..1307.5 by 10..50 physical pixels.
        assert_eq!(hit(1279, 30, 1.25), Hit::MaximizeButton);
        assert_eq!(hit(1250, 10, 1.25), Hit::MaximizeButton);
        assert_eq!(hit(1307, 49, 1.25), Hit::MaximizeButton);
        assert_eq!(hit(1308, 49, 1.25), Hit::Client);
        assert_eq!(hit(1307, 50, 1.25), Hit::Client);
        // 150%: 1500..1569 by 12..60.
        assert_eq!(hit(1534, 36, 1.5), Hit::MaximizeButton);
        assert_eq!(hit(1500, 12, 1.5), Hit::MaximizeButton);
        assert_eq!(hit(1569, 36, 1.5), Hit::Client);
        assert_eq!(hit(1534, 60, 1.5), Hit::Client);
        // 200%: 2000..2092 by 16..80.
        assert_eq!(hit(2046, 48, 2.0), Hit::MaximizeButton);
        assert_eq!(hit(2000, 16, 2.0), Hit::MaximizeButton);
        assert_eq!(hit(2092, 48, 2.0), Hit::Client);
        assert_eq!(hit(2046, 80, 2.0), Hit::Client);
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
