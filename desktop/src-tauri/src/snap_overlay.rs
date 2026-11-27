//! Windows' Snap Layouts flyout for the integrated bar's Maximize button.
//!
//! Windows offers the flyout to the window that answers `WM_NCHITTEST` with `HTMAXBUTTON` under the
//! pointer ("Support snap layouts for desktop apps on Windows 11"). The frame cannot answer for its
//! own page: WebView2 puts a child window over the whole client area, that child answers the hit
//! test first, and a subclass of the frame never sees it (issue #1104's counters, and the reason a
//! subclass of the frame cannot fix it).
//!
//! So the shell puts a small window of its own over the page's drawn button, the way Tauri's own
//! undecorated resizing puts one over the window's edges: it never paints, it reports `HTMAXBUTTON`
//! for every point of itself, and it sits above the webview. It owns the pointer while it is there,
//! so it reports hover and press back to the page through the root element's `data-window-control`
//! attribute (the styles in `web/src/styles/windowFrame.css` read it) and runs the caption button's
//! own action. Nothing outside the button's rectangle changes.
//!
//! Because it answers the hit test for every point of itself, the window is only ever shown for a
//! rectangle that (a) the page reported, (b) fits the window's client area, and (c) was reported
//! after the window last changed size or scale: any window size or DPI change hides it, and the
//! next report from the page puts it back. So a wrong or stale rectangle never leaves a window
//! covering part of the page: while there is no such report, the page keeps the pointer and its own
//! button is the one that answers. The shell replaces the page's document on its own at times
//! (restarting, failed): it clears the overlay as it navigates, because a page's own cleanup does
//! not run then and the next document may draw no button where the last one drew.

// Only the Windows shell creates the window; the geometry and page state are plain Rust, tested on
// every OS, so on other systems they are exercised by the tests alone.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::window_frame::Rect;

/// Where the overlay window goes: the page's rectangle in whole device pixels from the window's
/// client top-left, never empty.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The page's device-pixel rectangle rounded to whole pixels. The button's box comes from
/// `getBoundingClientRect`, so at some zoom levels it starts or ends on a fraction of a pixel.
///
/// A control that is not laid out measures zero, which is how a hidden or absent button reaches
/// here: it has no window. Nothing may be left covering a rectangle that no longer exists.
pub fn bounds(rect: Option<Rect>) -> Option<Bounds> {
    let rect = rect?;
    if !rect.is_valid() {
        return None;
    }
    let width = whole(rect.width);
    let height = whole(rect.height);
    if width < 1 || height < 1 {
        return None;
    }
    Some(Bounds {
        x: whole(rect.x),
        y: whole(rect.y),
        width,
        height,
    })
}

/// The one device pixel a rectangle may round over the client area: a fractional start or end can
/// land past the edge at a zoom level, and that is not a wrong rectangle.
const SLACK: i32 = 1;

/// Whether the page's rectangle can be the Maximize button: inside the window's client area, where
/// the page draws. A rectangle that is not there is a wrong one (an older layout, another zoom,
/// another window) and nothing may be put over it.
pub fn fits(place: Bounds, client: (i32, i32)) -> bool {
    let (width, height) = client;
    place.width > 0
        && place.height > 0
        && place.x >= -SLACK
        && place.y >= -SLACK
        && place.x.saturating_add(place.width) <= width.saturating_add(SLACK)
        && place.y.saturating_add(place.height) <= height.saturating_add(SLACK)
}

fn whole(value: f64) -> i32 {
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

/// What the bar's Maximize button does next.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Zoom {
    Maximize,
    Restore,
}

pub fn zoom(maximized: bool) -> Zoom {
    if maximized {
        Zoom::Restore
    } else {
        Zoom::Maximize
    }
}

/// Where the pointer is over the overlay, as the page's styles name it.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum Pointer {
    /// Not over the button.
    #[default]
    Idle,
    /// Over the button, not pressed.
    Hover,
    /// Pressed on the button.
    Press,
}

/// What the overlay knows about the pointer: the state the page draws, and whether the press that
/// is down began on the button.
///
/// The two part ways when a press drags off the button and comes back still held: the page draws
/// the button unpressed in between, while the release still ends the press that began on the
/// button, as it does on the system's own caption button.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct PointerState {
    /// The state the page draws.
    pub state: Pointer,
    /// Whether the press now down began on the button.
    pub owns_press: bool,
}

/// The script that tells the page which state its controls are in. The page cannot see the pointer
/// itself: the overlay window has it, so the shell reports the state on the root element, where
/// `windowFrame.css` styles the Maximize button as it styles its own `:hover` and `:active`.
pub fn pointer_script(pointer: Pointer) -> &'static str {
    match pointer {
        Pointer::Idle => r#"document.documentElement.removeAttribute("data-window-control")"#,
        Pointer::Hover => {
            r#"document.documentElement.setAttribute("data-window-control", "hover")"#
        }
        Pointer::Press => {
            r#"document.documentElement.setAttribute("data-window-control", "press")"#
        }
    }
}

/// What the pointer did over the overlay.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PointerEvent {
    /// The pointer came onto the button.
    Enter,
    /// The button was pressed.
    Press,
    /// The press ended.
    Release,
    /// The pointer left the button.
    Leave,
}

/// What the overlay reports for one pointer event.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Reaction {
    /// The state the page draws.
    pub state: Pointer,
    /// Whether the press now down began on the button, after this event.
    pub owns_press: bool,
    /// Whether the bar's Maximize button acts. As on the system's own caption button, a release
    /// only presses it when the press started on it: a drag that happens to end over the button is
    /// not a click.
    pub press: bool,
}

impl Reaction {
    /// What the overlay knows about the pointer after this event.
    pub fn next(self) -> PointerState {
        PointerState {
            state: self.state,
            owns_press: self.owns_press,
        }
    }
}

/// What one pointer event does to what the overlay knows.
///
/// `button_down` says whether the pointer's left button is down at this event. A press that drags
/// off the button ends where the overlay receives no message at all, so the next message it does
/// receive is where that end becomes known: an arrival with the button up is no longer that press.
pub fn react(pointer: PointerState, button_down: bool, event: PointerEvent) -> Reaction {
    // The press that began on the button is still down, so the pointer is still pressing it, even
    // where it left the button and came back while held.
    let pressing = pointer.owns_press && button_down;
    let state = match event {
        PointerEvent::Enter if pressing => Pointer::Press,
        PointerEvent::Enter => Pointer::Hover,
        PointerEvent::Press => Pointer::Press,
        PointerEvent::Release => Pointer::Hover,
        PointerEvent::Leave => Pointer::Idle,
    };
    let owns_press = match event {
        // The press began on the button.
        PointerEvent::Press => true,
        // Off the button and back again while still held is the same press; seen with the button
        // up, it is a press that ended where the overlay could not see, and it holds nothing.
        PointerEvent::Enter | PointerEvent::Leave => pressing,
        // The release ends the press wherever it landed.
        PointerEvent::Release => false,
    };
    Reaction {
        state,
        owns_press,
        press: pointer.owns_press && event == PointerEvent::Release,
    }
}

#[cfg(windows)]
pub use self::win::{install, set_bounds};

/// Takes the overlay off the window until the page reports its button again.
///
/// The shell calls this before it navigates the window to another document. A page's own cleanup
/// does not run when the shell replaces its document, and the next document may draw no Maximize
/// button where the last one drew: an overlay left there would answer the hit test over a place
/// with no button, where a hover would offer the Snap Layouts flyout and a click would maximize.
pub fn clear(window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    win::set_bounds(window, None);
    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(windows)]
mod win {
    use super::{
        Bounds, Pointer, PointerEvent, PointerState, Zoom, bounds, fits, pointer_script, react,
        zoom,
    };
    use crate::window_frame::Rect;
    use std::sync::Mutex;
    use std::sync::mpsc::{Sender, channel};
    use tauri::WebviewWindow;
    use windows_sys::Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::{
                GetKeyState, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent, VK_LBUTTON,
            },
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                CREATESTRUCTW, CreateWindowExW, DefWindowProcW, FindWindowExW, GWLP_USERDATA,
                GetClientRect, GetParent, GetWindowLongPtrW, HTMAXBUTTON, HWND_TOP, IsZoomed,
                RegisterClassExW, SC_MAXIMIZE, SC_RESTORE, SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
                SendMessageW, SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CREATE,
                WM_DPICHANGED, WM_MOUSEMOVE, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
                WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_SIZE,
                WM_SYSCOMMAND, WNDCLASSEXW, WS_CHILD, WS_CLIPSIBLINGS,
            },
        },
    };
    use windows_sys::core::{PCWSTR, w};

    /// The overlay's window class. It is registered once per process and every main window's
    /// overlay uses it.
    const CLASS_NAME: PCWSTR = w!("ORBYN_SNAP_HOVER");
    const WINDOW_NAME: PCWSTR = w!("Orbyn Engineering Maximize");
    /// `WM_MOUSELEAVE`: windows-sys 0.61 generates no constant for it, and the overlay listens for
    /// the client and the non-client leave alike.
    const WM_MOUSELEAVE: u32 = 0x02A3;

    /// The frame's subclass, which hides the overlay when the window changes size or scale.
    const FRAME_SUBCLASS_ID: usize = 1;

    /// The frame's subclass: a size or scale change means the page's button has moved before the
    /// page can say where it is, so the overlay stops covering anything until the next report.
    ///
    /// # Safety
    ///
    /// Called by Windows for this module's frame, on the thread that made it.
    unsafe extern "system" fn frame_proc(
        frame: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        match message {
            WM_SIZE | WM_DPICHANGED => {
                if let Some(overlay) = overlay_of(frame) {
                    hide(overlay);
                }
            }
            WM_NCDESTROY => {
                // SAFETY: removes this subclass from the frame that is going away, before its
                // children are destroyed below.
                unsafe { RemoveWindowSubclass(frame, Some(frame_proc), FRAME_SUBCLASS_ID) };
            }
            _ => {}
        }
        // SAFETY: passes the message to the next handler in the frame's chain.
        unsafe { DefSubclassProc(frame, message, wparam, lparam) }
    }

    /// What the overlay window keeps for its life: the channel it reports the pointer on, and what
    /// it knows about the pointer.
    ///
    /// The scripts run on a thread of their own so that the state reaches the page in the order the
    /// pointer made it, and so that no script runs inside a window procedure.
    struct Context {
        states: Sender<Pointer>,
        pointer: Mutex<PointerState>,
    }

    /// Covers the page's Maximize button with a window that answers `HTMAXBUTTON`.
    ///
    /// Best effort: if the class or the window cannot be made (an older or unusual host), the bar
    /// keeps working as it did before, without the flyout. Called once per window; a second call
    /// for a window that already has its overlay does nothing, so no second hit-test window can
    /// outlive a report.
    pub fn install(window: &WebviewWindow) {
        let Ok(handle) = window.hwnd() else {
            return;
        };
        let parent: HWND = handle.0;
        // SAFETY: `parent` is a live window handle and the search only walks its children.
        if !unsafe { FindWindowExW(parent, std::ptr::null_mut(), CLASS_NAME, WINDOW_NAME) }
            .is_null()
        {
            return;
        }
        register_class();

        let (states, scripts) = channel();
        let page = window.clone();
        std::thread::spawn(move || {
            for pointer in scripts {
                let _ = page.eval(pointer_script(pointer));
            }
        });

        let context = Box::into_raw(Box::new(Context {
            states,
            pointer: Mutex::new(PointerState::default()),
        }));
        // Hidden until the page says where its button is: an overlay with no rectangle must not
        // take a pixel of the window.
        // SAFETY: `parent` is this window's live top-level handle, the class is registered above,
        // and `context` is the box the window procedure frees on WM_NCDESTROY.
        let overlay = unsafe {
            CreateWindowExW(
                0,
                CLASS_NAME,
                WINDOW_NAME,
                WS_CHILD | WS_CLIPSIBLINGS,
                0,
                0,
                0,
                0,
                parent,
                std::ptr::null_mut(),
                instance(),
                context as *const std::ffi::c_void,
            )
        };
        if overlay.is_null() {
            // SAFETY: no window was made, so nothing but this call owns the box. Dropping it closes
            // the script channel above and its thread ends.
            unsafe { drop(Box::from_raw(context)) };
            return;
        }
        // A size or scale change moves the page's button before the page can say where it is, so
        // the overlay goes away with it and waits for the next report.
        // SAFETY: `parent` is this window's own frame and the subclass is removed on its
        // WM_NCDESTROY below.
        unsafe { SetWindowSubclass(parent, Some(frame_proc), FRAME_SUBCLASS_ID, 0) };
    }

    /// Moves the overlay onto the page's Maximize button, or hides it when the page reports no
    /// button, a rectangle that is not a rectangle, or one that is not inside the client area.
    pub fn set_bounds(window: &WebviewWindow, rect: Option<Rect>) {
        let Ok(handle) = window.hwnd() else {
            return;
        };
        let parent: HWND = handle.0;
        match bounds(rect) {
            Some(place) if fits(place, client_size(parent)) => {
                if let Some(overlay) = overlay_of(parent) {
                    place_over(overlay, place);
                }
            }
            _ => {
                if let Some(overlay) = overlay_of(parent) {
                    hide(overlay);
                }
            }
        }
    }

    /// This window's overlay, if it was made.
    fn overlay_of(parent: HWND) -> Option<HWND> {
        // SAFETY: `parent` is a live window handle and the search only walks its children.
        let overlay =
            unsafe { FindWindowExW(parent, std::ptr::null_mut(), CLASS_NAME, WINDOW_NAME) };
        (!overlay.is_null()).then_some(overlay)
    }

    /// The client area the page draws in, in device pixels.
    fn client_size(parent: HWND) -> (i32, i32) {
        let mut rect = RECT::default();
        // SAFETY: `parent` is a live window and `rect` is ours to write.
        if unsafe { GetClientRect(parent, &mut rect) } == 0 {
            return (0, 0);
        }
        (rect.right - rect.left, rect.bottom - rect.top)
    }

    /// Takes the overlay off the window: nothing is covered and the page's own button is what the
    /// pointer meets, as before this module existed.
    fn hide(overlay: HWND) {
        // SAFETY: `overlay` is this window's own child and it is the one being hidden.
        unsafe { ShowWindow(overlay, SW_HIDE) };
        apply(overlay, PointerEvent::Leave);
        // A hidden overlay never receives the release that would end a press it watched, so it
        // holds none of it: whatever ends over it after it comes back is not that press's click.
        update(overlay, |pointer| pointer.owns_press = false);
    }

    fn place_over(overlay: HWND, place: Bounds) {
        // SAFETY: `overlay` is this window's own child; the z-order puts it above the webview and
        // above Tauri's own edge window, and the sizes are the page's own numbers.
        unsafe {
            SetWindowPos(
                overlay,
                HWND_TOP,
                place.x,
                place.y,
                place.width,
                place.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    /// The overlay answers the hit test, reports the pointer to the page, and runs the button.
    ///
    /// The hover messages are handled in both their non-client and client forms: answering
    /// `HTMAXBUTTON` is what should bring the non-client ones, but which form a child window's
    /// pointer produces is the host's business, and the page must not lose its hover state if it
    /// is the other one.
    ///
    /// # Safety
    ///
    /// Called by Windows for a window this module made, on the thread that made it.
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        _wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_CREATE => {
                // SAFETY: `lparam` is the CREATESTRUCTW of this window, whose lpCreateParams is the
                // box `install` made; the value is kept for the window's life below.
                let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
                // SAFETY: `hwnd` is the window being created for this process.
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
                }
                return 0;
            }
            // The window covers the button and nothing else, so every point of it is the button.
            WM_NCHITTEST => return HTMAXBUTTON as LRESULT,
            WM_NCMOUSEMOVE | WM_MOUSEMOVE => {
                apply(hwnd, PointerEvent::Enter);
                // Ask for the leave message; the overlay owns the pointer, so no other message
                // would ever say it left.
                // SAFETY: `hwnd` is the window handling this message and `track` is ours.
                unsafe {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: if message == WM_NCMOUSEMOVE {
                            TME_LEAVE | TME_NONCLIENT
                        } else {
                            TME_LEAVE
                        },
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                }
                return 0;
            }
            WM_NCMOUSELEAVE | WM_MOUSELEAVE => {
                apply(hwnd, PointerEvent::Leave);
            }
            WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
                apply(hwnd, PointerEvent::Press);
                return 0;
            }
            WM_NCLBUTTONUP => {
                if apply(hwnd, PointerEvent::Release) {
                    zoom_window(hwnd);
                }
                return 0;
            }
            WM_NCDESTROY => {
                // SAFETY: the value is the box `install` made, and this is the one place it is
                // freed.
                let context = unsafe { window_context(hwnd) };
                if !context.is_null() {
                    // SAFETY: the pointer came from `Box::into_raw` and this window is going away.
                    unsafe { drop(Box::from_raw(context)) };
                }
            }
            _ => {}
        }
        // SAFETY: passes the message to the default handler for this window.
        unsafe { DefWindowProcW(hwnd, message, _wparam, lparam) }
    }

    /// Maximizes or restores the frame the overlay belongs to, as the system's own caption button
    /// does, so the state the page shows follows from the window itself.
    fn zoom_window(overlay: HWND) {
        // SAFETY: `overlay` is this window's own child, so its parent is live for this call.
        let frame = unsafe { GetParent(overlay) };
        if frame.is_null() {
            return;
        }
        // SAFETY: `frame` is this window's parent and `IsZoomed` only reads its state.
        let maximized = unsafe { IsZoomed(frame) } != 0;
        let command = match zoom(maximized) {
            Zoom::Maximize => SC_MAXIMIZE,
            Zoom::Restore => SC_RESTORE,
        };
        // SAFETY: the command is the one the system's caption button sends to its own frame.
        unsafe { SendMessageW(frame, WM_SYSCOMMAND, command as WPARAM, 0) };
    }

    /// Reports one pointer event to the page, and says whether the bar's button acts for it.
    fn apply(overlay: HWND, event: PointerEvent) -> bool {
        // SAFETY: `overlay` is the window this call is about; its box lives until WM_NCDESTROY.
        let context = unsafe { window_context(overlay) };
        if context.is_null() {
            return false;
        }
        // SAFETY: `context` is the box `install` made, valid until WM_NCDESTROY.
        let context = unsafe { &*context };
        let Ok(mut pointer) = context.pointer.lock() else {
            return false;
        };
        let before = *pointer;
        let reaction = react(before, button_down(), event);
        *pointer = reaction.next();
        if reaction.state != before.state {
            // A closed channel only means the window is going away.
            let _ = context.states.send(reaction.state);
        }
        reaction.press
    }

    /// Changes what this window knows about the pointer, if `overlay` is this module's window.
    fn update(overlay: HWND, change: impl FnOnce(&mut PointerState)) {
        // SAFETY: `overlay` is the window this call is about; its box lives until WM_NCDESTROY.
        let context = unsafe { window_context(overlay) };
        if context.is_null() {
            return;
        }
        // SAFETY: `context` is the box `install` made, valid until WM_NCDESTROY.
        let context = unsafe { &*context };
        if let Ok(mut pointer) = context.pointer.lock() {
            change(&mut pointer);
        }
    }

    /// Whether the pointer's left button is down for the message being handled.
    fn button_down() -> bool {
        // SAFETY: `GetKeyState` only reads this thread's own key state.
        (unsafe { GetKeyState(i32::from(VK_LBUTTON)) }) < 0
    }

    /// The box `install` made. Null before WM_CREATE stored it.
    ///
    /// # Safety
    ///
    /// `overlay` must be a window of this module.
    unsafe fn window_context(overlay: HWND) -> *mut Context {
        // SAFETY: reading the window's own user data, which this module sets.
        let data = unsafe { GetWindowLongPtrW(overlay, GWLP_USERDATA) };
        data as *mut Context
    }

    fn register_class() {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: instance(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        // SAFETY: `class` is fully initialised for this process and stays alive for the call.
        unsafe { RegisterClassExW(&class) };
    }

    fn instance() -> HINSTANCE {
        // SAFETY: `GetModuleHandleW(null)` asks for this process's own module.
        unsafe { GetModuleHandleW(std::ptr::null()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect {
            x: 1000.0,
            y: 0.0,
            width: 46.0,
            height: 32.0,
        }
    }

    #[test]
    fn the_overlay_is_the_pages_rectangle_in_whole_pixels() {
        assert_eq!(
            bounds(Some(rect())),
            Some(Bounds {
                x: 1000,
                y: 0,
                width: 46,
                height: 32,
            })
        );
        // At 125% zoom the page's box can land on half a pixel: the window rounds it.
        assert_eq!(
            bounds(Some(Rect {
                x: 1142.5,
                y: 0.5,
                width: 45.6,
                height: 31.4,
            })),
            Some(Bounds {
                x: 1143,
                y: 1,
                width: 46,
                height: 31,
            })
        );
        // A control that is not laid out measures zero: there is no button, so there is no window.
        // Putting a window there would cover a corner of the page for nothing.
        assert_eq!(
            bounds(Some(Rect {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            })),
            None
        );
        assert_eq!(
            bounds(Some(Rect {
                x: 40.0,
                y: 0.0,
                width: 0.0,
                height: 32.0,
            })),
            None
        );
        // Nothing drawn means no window at all.
        assert_eq!(bounds(None), None);
        // The page cannot place the window with a value that is not a rectangle.
        assert_eq!(
            bounds(Some(Rect {
                width: f64::NAN,
                ..rect()
            })),
            None
        );
        assert_eq!(
            bounds(Some(Rect {
                height: -1.0,
                ..rect()
            })),
            None
        );
    }

    #[test]
    fn only_a_rectangle_inside_the_client_area_can_be_the_button() {
        // The top-right corner of a 1280x820 client: the button's own place.
        let button = Bounds {
            x: 1234,
            y: 0,
            width: 46,
            height: 32,
        };
        assert!(fits(button, (1280, 820)));
        // A rectangle that ends exactly on the edges is the button and nothing else.
        assert!(fits(
            Bounds {
                x: 0,
                y: 788,
                width: 1280,
                height: 32,
            },
            (1280, 820)
        ));
        // One pixel over the edge is the zoom level's rounding, not a wrong rectangle.
        assert!(fits(
            Bounds {
                x: 1234,
                y: 0,
                width: 47,
                height: 32,
            },
            (1280, 820)
        ));
        assert!(fits(
            Bounds {
                x: -1,
                y: 0,
                width: 46,
                height: 32,
            },
            (1280, 820)
        ));
        // A rectangle from another layout, another zoom or another window is not the button: it
        // would cover part of the page, and a click there would maximize the window instead.
        assert!(!fits(
            Bounds {
                x: 1234,
                y: 0,
                width: 460,
                height: 32,
            },
            (1280, 820)
        ));
        assert!(!fits(
            Bounds {
                x: 1234,
                y: 800,
                width: 46,
                height: 32,
            },
            (1280, 820)
        ));
        assert!(!fits(
            Bounds {
                x: -8,
                y: 0,
                width: 46,
                height: 32,
            },
            (1280, 820)
        ));
        assert!(!fits(
            Bounds {
                x: 1300,
                y: 0,
                width: 46,
                height: 32,
            },
            (1280, 820)
        ));
        // A window with no client area yet has no button in it.
        assert!(!fits(button, (0, 0)));
        assert!(!fits(
            Bounds {
                width: 0,
                height: 0,
                ..button
            },
            (1280, 820)
        ));
    }

    #[test]
    fn the_button_restores_a_maximized_window_and_maximizes_one_that_is_not() {
        assert_eq!(zoom(false), Zoom::Maximize);
        assert_eq!(zoom(true), Zoom::Restore);
    }

    #[test]
    fn the_pointer_state_follows_the_messages_and_only_a_press_on_the_button_acts() {
        use Pointer::{Hover, Idle, Press};
        use PointerEvent::{Enter, Leave, Release};
        let idle = PointerState::default();
        // Coming onto the button hovers it; leaving clears it, from any state. Nothing the pointer
        // does without a press that began on the button ever acts.
        for from in [idle, known(Hover, false), known(Press, true)] {
            let entered = react(from, false, Enter);
            assert_eq!(entered.state, Hover, "{from:?}");
            assert!(!entered.press, "{from:?}");
            let left = react(from, false, Leave);
            assert_eq!(left.state, Idle, "{from:?}");
            assert!(!left.press, "{from:?}");
            assert!(!left.owns_press, "{from:?}");
        }
        // Pressing shows the pressed state, and the press does nothing on its own.
        let pressed = react(known(Hover, false), true, PointerEvent::Press);
        assert_eq!(pressed.state, Press);
        assert!(pressed.owns_press);
        assert!(!pressed.press);
        // The release acts when the press began on the button, and the button then hovers again:
        // the pointer is still over it.
        let click = react(known(Press, true), false, Release);
        assert_eq!(click.state, Hover);
        assert!(!click.owns_press);
        assert!(click.press);
        // A press that started anywhere else ends over the button without a click.
        for from in [idle, known(Hover, false)] {
            let drag = react(from, false, Release);
            assert_eq!(drag.state, Hover, "{from:?}");
            assert!(!drag.press, "{from:?}");
        }
        // Moving over the button with its own press still down keeps it pressed.
        let moved = react(known(Press, true), true, Enter);
        assert_eq!(moved.state, Press);
        assert!(moved.owns_press);
        assert!(!moved.press);
    }

    #[test]
    fn a_press_that_leaves_the_button_and_comes_back_still_acts() {
        use Pointer::{Hover, Idle, Press};
        use PointerEvent::{Enter, Leave, Release};
        // A press drags off the button: the page stops drawing it pressed, and the release that
        // ends it is no longer over the button.
        let off = react(known(Press, true), true, Leave);
        assert_eq!(off.state, Idle);
        assert!(!off.press);
        assert!(off.owns_press);
        // Back over the button while still held: the same press, and the page draws it pressed.
        let back = react(off.next(), true, Enter);
        assert_eq!(back.state, Press);
        assert!(back.owns_press);
        // The release over the button is that press's click, as on the system's caption button.
        let click = react(back.next(), false, Release);
        assert_eq!(click.state, Hover);
        assert!(click.press);
    }

    #[test]
    fn a_press_released_outside_the_overlay_is_forgotten() {
        use Pointer::{Hover, Idle, Press};
        use PointerEvent::{Enter, Leave, Release};
        // The overlay receives no release for a press that dragged away and ended elsewhere, so
        // the next arrival is where its end shows: the button is up, and nothing is held any more.
        let off = react(known(Press, true), true, Leave);
        let ended = react(off.next(), false, Enter);
        assert_eq!(ended.state, Hover);
        assert!(!ended.owns_press);
        // A release over the button after that is not the click of a press it ever watched.
        assert!(!react(ended.next(), false, Release).press);
        // A leave seen with the button up holds nothing either.
        assert!(!react(known(Idle, true), false, Leave).owns_press);
    }

    /// What the overlay knows about the pointer before one of the tests above.
    fn known(state: Pointer, owns_press: bool) -> PointerState {
        PointerState { state, owns_press }
    }

    #[test]
    fn the_page_state_is_the_root_attributes_the_styles_read() {
        assert_eq!(
            pointer_script(Pointer::Hover),
            r#"document.documentElement.setAttribute("data-window-control", "hover")"#
        );
        assert_eq!(
            pointer_script(Pointer::Press),
            r#"document.documentElement.setAttribute("data-window-control", "press")"#
        );
        assert_eq!(
            pointer_script(Pointer::Idle),
            r#"document.documentElement.removeAttribute("data-window-control")"#
        );
        // The styles in the web shell read this exact attribute and these exact values.
        let css = include_str!("../../../web/src/styles/windowFrame.css");
        for state in ["hover", "press"] {
            assert!(
                css.contains(&format!(
                    "[data-window-control=\"{state}\"] .window-control-maximize"
                )),
                "{state}"
            );
        }
    }
}
