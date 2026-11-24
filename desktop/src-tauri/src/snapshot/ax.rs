//! macOS accessible text: one AX tree read for the window that was just captured. Read-only and
//! prompt-free: it sets no attribute on another app and performs no action.

#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Once};

use super::accessible::{
    Budget, Fields, Tree, WindowChoice, collect, utf8_buffer_len, window_choice,
};

type CFTypeRef = *const c_void;
type CFIndex = isize;
type CFTypeID = usize;
type CFStringRef = *const c_void;
type CFArrayRef = *const c_void;
/// The C `Boolean` type is an `unsigned char`, not Rust's `bool`.
type Boolean = u8;

#[repr(C)]
struct CFRange {
    location: CFIndex,
    length: CFIndex,
}

const UTF8_ENCODING: u32 = 0x0800_0100;
/// The most UTF-16 units read from one string attribute; the collector bounds the whole text too.
const MAX_FIELD_UTF16: usize = 20_000;
const MAX_WINDOWS: usize = 50;
const MAX_CHILDREN: usize = 500;
const MESSAGING_TIMEOUT_SECONDS: f32 = 0.25;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> i32;
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXIsProcessTrusted() -> Boolean;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: CFTypeRef);
    fn CFRetain(value: CFTypeRef) -> CFTypeRef;
    fn CFGetTypeID(value: CFTypeRef) -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn CFArrayGetTypeID() -> CFTypeID;
    fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, index: CFIndex) -> CFTypeRef;
    fn CFStringGetLength(string: CFStringRef) -> CFIndex;
    fn CFStringGetBytes(
        string: CFStringRef,
        range: CFRange,
        encoding: u32,
        loss_byte: u8,
        is_external: u8,
        buffer: *mut u8,
        max_len: CFIndex,
        used: *mut CFIndex,
    ) -> CFIndex;
    fn CFStringCreateWithBytes(
        alloc: CFTypeRef,
        bytes: *const u8,
        len: CFIndex,
        encoding: u32,
        is_external: u8,
    ) -> CFStringRef;
}

/// One owned CoreFoundation object. The field is private, so every value is non-null by
/// construction, and each value releases exactly the one reference it owns.
struct Cf(CFTypeRef);

impl Cf {
    /// Wraps a new +1 reference from a create or copy call, or answers `None` for null.
    fn owned(value: CFTypeRef) -> Option<Self> {
        if value.is_null() {
            return None;
        }
        Some(Self(value))
    }

    /// Takes a new +1 reference to a borrowed, non-null object.
    fn retained(value: CFTypeRef) -> Option<Self> {
        if value.is_null() {
            return None;
        }
        // SAFETY: `value` is a live object borrowed from a live CFArray, so CFRetain returns the
        // same non-null object with one more retain that this `Cf` owns.
        let retained = unsafe { CFRetain(value) };
        Self::owned(retained)
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        // SAFETY: `self.0` is non-null and holds the single +1 reference this value owns; it is
        // released exactly once here, and no code touches it afterwards.
        unsafe {
            CFRelease(self.0);
        }
    }
}

impl Clone for Cf {
    fn clone(&self) -> Self {
        // SAFETY: `self.0` is a live non-null CoreFoundation object, so CFRetain returns the same
        // non-null object with one more retain that the clone owns.
        Self(unsafe { CFRetain(self.0) })
    }
}

/// Creates a CFString from UTF-8 text; the result is +1 and owned by the returned `Cf`.
fn cf_string(text: &str) -> Option<Cf> {
    let length = CFIndex::try_from(text.len()).ok()?;
    // SAFETY: `text` is a live UTF-8 buffer for the call, `length` is its exact byte count, and
    // the null allocator asks for the default allocator. The result is +1 or null.
    let created = unsafe {
        CFStringCreateWithBytes(std::ptr::null(), text.as_ptr(), length, UTF8_ENCODING, 0)
    };
    Cf::owned(created)
}

/// Bounds every AX message this process sends.
///
/// Apple's `AXUIElement.h` says a timeout set on one object applies only to that object and that
/// the system-wide object is the documented way to set it for the whole process. Setting it on the
/// application element alone therefore leaves the window and child reads (the titles and the tree)
/// on the process default, where a slow app can hold the capture thread well past its budget.
/// Read-only for other apps: this changes our client timeout, not any attribute of theirs.
fn bound_messaging_timeout() {
    static BOUND: Once = Once::new();
    BOUND.call_once(|| {
        // SAFETY: the call takes nothing and returns a new +1 system-wide element or null, which
        // `Cf` owns and releases; the timeout is this process's client setting only.
        let Some(system) = Cf::owned(unsafe { AXUIElementCreateSystemWide() }) else {
            return;
        };
        // SAFETY: `system` is a live AX element. The result only reports whether the client timeout
        // was stored; a failure leaves the default and is deliberately ignored.
        unsafe {
            AXUIElementSetMessagingTimeout(system.0, MESSAGING_TIMEOUT_SECONDS);
        }
    });
}

/// Copies one AX attribute of `element`, which is +1 on success and owned by the returned `Cf`.
fn attribute(element: &Cf, name: &Cf) -> Option<Cf> {
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: `element` and `name` are live CoreFoundation references, and `value` is a live
    // local out-pointer. Only a zero (success) result is read; it holds a +1 reference.
    let error = unsafe { AXUIElementCopyAttributeValue(element.0, name.0, &mut value) };
    if error != 0 {
        // A copy call hands back a +1 value only on success. If a framework fills the out-pointer
        // on a failure anyway, release it so the ownership rule stays total.
        if !value.is_null() {
            // SAFETY: `value` is non-null and was returned by a copy call, so it carries a +1
            // reference this branch owns; nothing else releases it.
            unsafe {
                CFRelease(value);
            }
        }
        return None;
    }
    Cf::owned(value)
}

/// Reads a CFString attribute into a `String`, capped at [`MAX_FIELD_UTF16`] UTF-16 units.
fn string_of(value: &Cf) -> Option<String> {
    // SAFETY: `value` is a live non-null CoreFoundation object; CFGetTypeID only reads it.
    if unsafe { CFGetTypeID(value.0) } != unsafe { CFStringGetTypeID() } {
        return None;
    }
    // SAFETY: the type id check above says this is a CFString; CFStringGetLength only reads it.
    let units = unsafe { CFStringGetLength(value.0) };
    if units < 0 {
        return None;
    }
    let length = usize::try_from(units).ok()?.min(MAX_FIELD_UTF16);
    let capacity = utf8_buffer_len(length);
    let mut buffer = vec![0_u8; capacity];
    let range = CFRange {
        location: 0,
        length: CFIndex::try_from(length).ok()?,
    };
    let max_len = CFIndex::try_from(capacity).ok()?;
    let mut used: CFIndex = 0;
    // SAFETY: `value` is a CFString, `range` covers its first `length` UTF-16 units, and
    // `buffer` has `length * 3 + 4` bytes, the UTF-8 worst case for those units. The call writes
    // at most `max_len` bytes and records how many it wrote in `used`.
    let converted = unsafe {
        CFStringGetBytes(
            value.0,
            range,
            UTF8_ENCODING,
            0,
            0,
            buffer.as_mut_ptr(),
            max_len,
            &mut used,
        )
    };
    // The return is the number of characters converted: 0 when the range is empty or a character
    // cannot be converted, which is no text rather than an empty value.
    if converted <= 0 || used < 0 {
        return None;
    }
    let used = usize::try_from(used).ok()?;
    Some(String::from_utf8_lossy(buffer.get(..used)?).into_owned())
}

fn string_attribute(element: &Cf, name: &Cf) -> String {
    attribute(element, name)
        .and_then(|value| string_of(&value))
        .unwrap_or_default()
}

/// Copies up to `cap` values out of `value` when it is a CFArray. Each copy is retained, so it
/// outlives the array it came from.
fn array_values(value: &Cf, cap: usize) -> Vec<Cf> {
    // SAFETY: `value` is a live non-null CoreFoundation object; CFGetTypeID only reads it.
    if unsafe { CFGetTypeID(value.0) } != unsafe { CFArrayGetTypeID() } {
        return Vec::new();
    }
    // SAFETY: the type id check above says this is a CFArray; CFArrayGetCount only reads it.
    let count = unsafe { CFArrayGetCount(value.0) };
    if count <= 0 {
        return Vec::new();
    }
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    let count = count.min(cap);
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let Ok(index) = CFIndex::try_from(index) else {
            break;
        };
        // SAFETY: `value` is a CFArray and `index` is below its count, so the returned value is
        // borrowed, non-null, and only valid while the array lives; `Cf::retained` takes the +1
        // that outlives it.
        let retained = unsafe { CFArrayGetValueAtIndex(value.0, index) };
        let Some(child) = Cf::retained(retained) else {
            continue;
        };
        values.push(child);
    }
    values
}

/// The AX node source for one window. The six attribute names are CFStrings created once, so
/// every node read reuses them.
struct Ax {
    role: Cf,
    subrole: Cf,
    value: Cf,
    title: Cf,
    description: Cf,
    children: Cf,
}

impl Ax {
    fn new() -> Option<Self> {
        Some(Self {
            role: cf_string("AXRole")?,
            subrole: cf_string("AXSubrole")?,
            value: cf_string("AXValue")?,
            title: cf_string("AXTitle")?,
            description: cf_string("AXDescription")?,
            children: cf_string("AXChildren")?,
        })
    }
}

impl Tree for Ax {
    type Node = Cf;

    fn fields(&self, node: &Cf) -> Fields {
        // Without a role there is no way to tell a secure field from an ordinary one, so the node
        // contributes nothing at all rather than risk reading a password.
        let Some(role) = attribute(node, &self.role).and_then(|role| string_of(&role)) else {
            return Fields::default();
        };
        Fields {
            role,
            subrole: string_attribute(node, &self.subrole),
            value: string_attribute(node, &self.value),
            title: string_attribute(node, &self.title),
            description: string_attribute(node, &self.description),
        }
    }

    fn children(&self, node: &Cf) -> Vec<Cf> {
        attribute(node, &self.children)
            .map(|array| array_values(&array, MAX_CHILDREN))
            .unwrap_or_default()
    }
}

fn named_string(element: &Cf, name: &str) -> String {
    let Some(name) = cf_string(name) else {
        return String::new();
    };
    string_attribute(element, &name)
}

fn windows(app: &Cf) -> Vec<Cf> {
    let Some(name) = cf_string("AXWindows") else {
        return Vec::new();
    };
    let Some(array) = attribute(app, &name) else {
        return Vec::new();
    };
    array_values(&array, MAX_WINDOWS)
}

/// Picks the window named by the capture: the exact title among `AXWindows`, or the app's
/// focused window only when the capture itself was of that focused window. It never reads a
/// different window of the same app, and it checks the budget while reading titles so the
/// selection stays inside the read's wall clock.
fn pick_window(
    app: &Cf,
    window_title: &str,
    captured_is_focused: bool,
    budget: &Budget,
) -> Option<Cf> {
    if !window_title.is_empty() && !budget.expired() && !budget.stopped() {
        let windows = windows(app);
        let mut titles = Vec::with_capacity(windows.len());
        for window in &windows {
            if budget.expired() || budget.stopped() {
                return None;
            }
            titles.push(named_string(window, "AXTitle"));
        }
        match window_choice(&titles, window_title, captured_is_focused) {
            WindowChoice::Exact(index) => return windows.into_iter().nth(index),
            WindowChoice::Focused => return focused_window(app, budget),
            WindowChoice::Skip => return None,
        }
    }
    // An empty title names no window in `AXWindows`; only a capture of the focused window may
    // fall back to it.
    if captured_is_focused {
        focused_window(app, budget)
    } else {
        None
    }
}

/// The app's focused window, then its main window, while the budget lasts.
fn focused_window(app: &Cf, budget: &Budget) -> Option<Cf> {
    for name in ["AXFocusedWindow", "AXMainWindow"] {
        if budget.expired() || budget.stopped() {
            return None;
        }
        let Some(name) = cf_string(name) else {
            continue;
        };
        if let Some(window) = attribute(app, &name) {
            return Some(window);
        }
    }
    None
}

/// Reads the accessible text of one window of `pid`, or answers `None` when this process is not
/// trusted, the process id is invalid, or no window can be read. It never prompts or acts.
///
/// `captured_is_focused` says whether the captured window was the app's focused window (it is the
/// only case where reading the focused window is the same window). `stop` is set when the capture
/// flow gives up, so the read ends instead of outliving the capture.
pub fn read_window_text(
    pid: i32,
    window_title: &str,
    captured_is_focused: bool,
    stop: &Arc<AtomicBool>,
) -> Option<String> {
    if pid <= 0 || stop.load(Ordering::Relaxed) {
        return None;
    }
    // SAFETY: the call takes nothing, touches no memory, and only reads this process's
    // Accessibility grant without prompting.
    if unsafe { AXIsProcessTrusted() } == 0 {
        return None;
    }
    // Bound every AX message before the first read of any element; the memoized call is a no-op
    // after the first read in this process.
    bound_messaging_timeout();
    if stop.load(Ordering::Relaxed) {
        return None;
    }
    // The budget starts before the window is chosen, so title matching counts against it too.
    let budget = Budget {
        stop: Arc::clone(stop),
        ..Budget::new()
    };
    // SAFETY: the call takes a pid and returns a new +1 application element or null, which `Cf`
    // owns and releases.
    let app = Cf::owned(unsafe { AXUIElementCreateApplication(pid) })?;
    if budget.expired() || budget.stopped() {
        return None;
    }
    let window = pick_window(&app, window_title, captured_is_focused, &budget)?;
    collect(&Ax::new()?, &window, &budget)
}
