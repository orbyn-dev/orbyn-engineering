//! Windows window capture through `PrintWindow`, into an in-memory PNG. The capturer is not
//! constructed unless `supported` is true: `facts().supported` stays false on Windows until
//! capture has run on a Windows machine (docs/snapshots.md), so `start_capture` refuses before
//! any capturer runs. The pixels only ever live in the returned `Vec`.

#![cfg(windows)]

use std::ffi::c_void;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows_sys::Win32::Storage::Xps::PrintWindow;
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, PW_RENDERFULLCONTENT,
};

use super::flow::{Captured, Capturer, Failure};
use super::windows_png::{app_name_from_path, bgra_to_rgba, encode_png, file_name_of};

const MAX_SIDE: i32 = 16384;
const MAX_TITLE_UNITS: usize = 4096;

pub fn capturer(_own_bundle_id: &str, _include_accessibility: bool) -> Box<dyn Capturer> {
    // Accessible text on Windows is Planned: the setting is accepted and ignored.
    Box::new(WindowsCapturer)
}

struct WindowsCapturer;

impl Capturer for WindowsCapturer {
    fn capture_frontmost(
        &self,
        is_excluded: &dyn Fn(&str, &str) -> bool,
    ) -> Result<Captured, Failure> {
        // SAFETY: takes no arguments and returns the current foreground window, or null.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return Err(Failure::NoWindow);
        }
        // SAFETY: `hwnd` is a live window handle and the call only reads its visibility.
        let hidden = unsafe { IsWindowVisible(hwnd) } == 0;
        // SAFETY: `hwnd` is the same live handle and the call only reads its minimized state.
        let minimized = unsafe { IsIconic(hwnd) } != 0;
        // SAFETY: takes no arguments and returns the shell window.
        let shell = unsafe { GetShellWindow() };
        // SAFETY: takes no arguments and returns the desktop window.
        let desktop = unsafe { GetDesktopWindow() };
        if hidden
            || minimized
            || std::ptr::eq(hwnd as *const c_void, shell as *const c_void)
            || std::ptr::eq(hwnd as *const c_void, desktop as *const c_void)
        {
            return Err(Failure::NoWindow);
        }
        let mut pid = 0_u32;
        // SAFETY: `hwnd` is live and `pid` is a writable u32 for the out parameter.
        let thread = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if thread == 0 || pid == std::process::id() {
            return Err(Failure::NoWindow);
        }
        let path = process_image_path(pid);
        let app_id = file_name_of(&path);
        let app_name = app_name_from_path(&path);
        let title = window_title(hwnd);
        if is_excluded(&app_id, &app_name) {
            return Err(Failure::Excluded { app_name });
        }
        let mut rect = RECT::default();
        // SAFETY: `hwnd` is live and `rect` is a writable RECT for the out parameter.
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return Err(Failure::NoWindow);
        }
        let width = rect.right.saturating_sub(rect.left);
        let height = rect.bottom.saturating_sub(rect.top);
        if width <= 0 || height <= 0 {
            return Err(Failure::NoWindow);
        }
        if width > MAX_SIDE || height > MAX_SIDE {
            return Err(Failure::CaptureError);
        }
        let png = capture_png(hwnd, width as u32, height as u32)?;
        Ok(Captured {
            png,
            app_name,
            window_title: title,
            app_id,
            accessible_text: None,
        })
    }
}

fn process_image_path(pid: u32) -> String {
    // SAFETY: PROCESS_QUERY_LIMITED_INFORMATION only asks to read the process image path.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return String::new();
    }
    let _process = ProcessHandle(process);
    let mut buffer = [0_u16; 1024];
    let mut size = buffer.len() as u32;
    // SAFETY: `process` is a live handle, `buffer` is a writable 1024-unit UTF-16 buffer, and
    // `size` describes its length on input and the written length on output.
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut size) };
    if ok == 0 {
        return String::new();
    }
    let length = (size as usize).min(buffer.len());
    let path = String::from_utf16_lossy(&buffer[..length]);
    path.trim_end_matches('\0').to_owned()
}

fn window_title(hwnd: HWND) -> String {
    // SAFETY: `hwnd` is live and the call only reads the title length in UTF-16 units.
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return String::new();
    }
    let capacity = (length as usize + 1).min(MAX_TITLE_UNITS);
    let mut buffer = vec![0_u16; capacity];
    // SAFETY: `buffer` has `capacity` writable UTF-16 units and the call writes at most that
    // many, including the terminator.
    let written = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), capacity as i32) };
    if written <= 0 {
        return String::new();
    }
    let written = (written as usize).min(capacity);
    String::from_utf16_lossy(&buffer[..written])
}

fn capture_png(hwnd: HWND, width: u32, height: u32) -> Result<Vec<u8>, Failure> {
    // SAFETY: a null HWND asks for the screen DC, which the guard releases with ReleaseDC.
    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    if screen.is_null() {
        return Err(Failure::CaptureError);
    }
    let _screen = ScreenDc(screen);

    // SAFETY: `screen` is a live DC and the guard deletes the compatible memory DC.
    let memory = unsafe { CreateCompatibleDC(screen) };
    if memory.is_null() {
        return Err(Failure::CaptureError);
    }
    let _memory = MemoryDc(memory);

    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width as i32;
    info.bmiHeader.biHeight = -(height as i32);
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;

    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: `info` describes a top-down 32-bpp DIB of `width` by `height` pixels, and `bits`
    // receives the pixel pointer owned by the returned bitmap.
    let bitmap = unsafe {
        CreateDIBSection(
            memory,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    if bitmap.is_null() || bits.is_null() {
        return Err(Failure::CaptureError);
    }
    let _bitmap = Dib(bitmap);

    // SAFETY: `memory` is a live DC and `bitmap` a live DIB; the guard restores the object the
    // DC held before this selection.
    let previous = unsafe { SelectObject(memory, bitmap) };
    if previous.is_null()
        || std::ptr::eq(
            previous as *const c_void,
            INVALID_HANDLE_VALUE as *const c_void,
        )
    {
        return Err(Failure::CaptureError);
    }
    let _selected = SelectedObject {
        dc: memory,
        previous,
    };

    // SAFETY: `memory` holds a DIB of exactly `width` by `height` pixels, so PrintWindow draws
    // within the DIB's pixel buffer.
    if unsafe { PrintWindow(hwnd, memory, PW_RENDERFULLCONTENT) } == 0 {
        return Err(Failure::CaptureError);
    }

    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(Failure::CaptureError)?;
    // SAFETY: `bits` points at the DIB's pixel buffer, which holds `width * height * 4` bytes
    // because CreateDIBSection allocated it for those dimensions; `len` is that same count and
    // the bitmap stays alive until the copy is complete.
    let mut rgba = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), len) }.to_vec();
    bgra_to_rgba(&mut rgba);
    encode_png(width, height, &rgba).ok_or(Failure::CaptureError)
}

struct ScreenDc(HDC);

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: the handle came from GetDC(NULL) and is released exactly once.
        unsafe {
            ReleaseDC(std::ptr::null_mut(), self.0);
        }
    }
}

struct MemoryDc(HDC);

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateCompatibleDC and is deleted exactly once.
        unsafe {
            DeleteDC(self.0);
        }
    }
}

struct Dib(HBITMAP);

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: the handle came from CreateDIBSection and is deleted exactly once.
        unsafe {
            DeleteObject(self.0);
        }
    }
}

struct SelectedObject {
    dc: HDC,
    previous: HGDIOBJ,
}

impl Drop for SelectedObject {
    fn drop(&mut self) {
        // SAFETY: both handles are live; the DC guard is declared before this one, so the DC
        // outlives the restore, and `previous` came from SelectObject on that same DC.
        unsafe {
            SelectObject(self.dc, self.previous);
        }
    }
}

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed exactly once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
