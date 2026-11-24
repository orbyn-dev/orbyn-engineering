//! macOS accessible text: one AX tree read for the window that was just captured. Read-only and
//! prompt-free: it sets no attribute on another app and performs no action.

#![cfg(target_os = "macos")]

use std::ffi::c_void;

use super::accessible::{Budget, Fields, Tree, collect, pick_window_index, utf8_buffer_len};

type CFTypeRef = *const c_void;
type CFIndex = isize;
type CFTypeID = usize;
type CFStringRef = *const c_void;
type CFArrayRef = *const c_void;

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
    fn AXIsProcessTrusted() -> bool;
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

/// Copies one AX attribute of `element`, which is +1 on success and owned by the returned `Cf`.
fn attribute(element: &Cf, name: &Cf) -> Option<Cf> {
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: `element` and `name` are live CoreFoundation references, and `value` is a live
    // local out-pointer. Only a zero (success) result is read; it holds a +1 reference.
    let error = unsafe { AXUIElementCopyAttributeValue(element.0, name.0, &mut value) };
    if error != 0 {
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
    unsafe {
        CFStringGetBytes(
            value.0,
            range,
            UTF8_ENCODING,
            0,
            0,
            buffer.as_mut_ptr(),
            max_len,
            &mut used,
        );
    }
    if used < 0 {
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

/// Picks the window named by the capture: the exact title among `AXWindows` when one is asked
/// for, then the focused window, then the main window.
fn pick_window(app: &Cf, window_title: &str) -> Option<Cf> {
    if !window_title.is_empty() {
        let windows = windows(app);
        let titles: Vec<String> = windows
            .iter()
            .map(|window| named_string(window, "AXTitle"))
            .collect();
        if let Some(index) = pick_window_index(&titles, window_title) {
            return windows.into_iter().nth(index);
        }
    }
    for name in ["AXFocusedWindow", "AXMainWindow"] {
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
pub fn read_window_text(pid: i32, window_title: &str) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    // SAFETY: the call takes nothing, touches no memory, and only reads this process's
    // Accessibility grant without prompting.
    if !unsafe { AXIsProcessTrusted() } {
        return None;
    }
    // SAFETY: the call takes a pid and returns a new +1 application element or null, which `Cf`
    // owns and releases.
    let app = Cf::owned(unsafe { AXUIElementCreateApplication(pid) })?;
    // SAFETY: `app` is a live AX application element; the call only bounds the time later AX
    // reads may take, and the result is deliberately ignored.
    unsafe {
        AXUIElementSetMessagingTimeout(app.0, MESSAGING_TIMEOUT_SECONDS);
    }
    let window = pick_window(&app, window_title)?;
    collect(&Ax::new()?, &window, &Budget::new())
}
