//! macOS window capture through ScreenCaptureKit. One window of another app, captured once into an in-memory PNG.

#![cfg(target_os = "macos")]

use std::ffi::{c_char, c_int, c_void};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use objc2::AnyThread;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSDeviceRGBColorSpace,
    NSWorkspace,
};
use objc2_foundation::NSDictionary;
use screencapturekit::prelude::*;
use screencapturekit::screenshot_manager::{CGImage, CGImageExt, SCScreenshotManager};
use screencapturekit::shareable_content::SCShareableContentInfo;

use super::flow::{Captured, Capturer, Failure};
use super::setup::SetupPlatform;
use super::state::{Permissions, PlatformFacts};

const NEEDS_MACOS_14: &str = "Snap Shot needs macOS 14 or newer.";
const SCREEN_RECORDING_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    // SAFETY: takes no arguments, touches no memory, and only asks macOS to show the one
    // Screen Recording prompt for this process.
    fn CGRequestScreenCaptureAccess() -> bool;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

unsafe extern "C" {
    fn sysctlbyname(
        name: *const c_char,
        oldp: *mut c_void,
        oldlenp: *mut usize,
        newp: *mut c_void,
        newlen: usize,
    ) -> c_int;
}

pub fn supported() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        let mut size = 32_usize;
        let mut buffer = [0_u8; 32];
        // SAFETY: sysctlbyname writes at most `size` bytes into `buffer` and updates `size`.
        let result = unsafe {
            sysctlbyname(
                c"kern.osproductversion".as_ptr(),
                buffer.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if result != 0 {
            return false;
        }
        let length = size.min(buffer.len());
        let version = std::str::from_utf8(&buffer[..length]).unwrap_or("");
        version
            .trim_end_matches('\0')
            .split('.')
            .next()
            .and_then(|major| major.parse::<u32>().ok())
            .unwrap_or(0)
            >= 14
    })
}

fn screen_recording_granted() -> bool {
    // SAFETY: the function takes nothing and only reads the Screen Recording grant.
    unsafe { CGPreflightScreenCaptureAccess() }
}

fn accessibility_granted() -> bool {
    // SAFETY: the function takes nothing and only reads the Accessibility grant.
    unsafe { AXIsProcessTrusted() }
}

pub fn setup_platform() -> impl SetupPlatform {
    MacSetup
}

struct MacSetup;

impl SetupPlatform for MacSetup {
    fn screen_recording_granted(&self) -> bool {
        screen_recording_granted()
    }

    fn request_screen_recording(&self) {
        // SAFETY: the function takes nothing and touches no memory; it only asks macOS to
        // show the Screen Recording prompt for this process.
        unsafe {
            CGRequestScreenCaptureAccess();
        }
    }

    fn open_screen_recording_settings(&self) {
        // Best effort: the state answer still reports the grant. The argument is fixed here,
        // never built from page input, and there is no shell.
        let _ = Command::new("/usr/bin/open")
            .arg(SCREEN_RECORDING_SETTINGS)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub fn facts() -> PlatformFacts {
    let supported = supported();
    PlatformFacts {
        supported,
        platform: "macos",
        message: (!supported).then(|| NEEDS_MACOS_14.to_owned()),
        permissions: Permissions {
            screen_recording: screen_recording_granted(),
            accessibility: accessibility_granted(),
        },
    }
}

pub fn capturer(own_bundle_id: &str) -> Box<dyn Capturer> {
    Box::new(MacCapturer {
        own_bundle_id: own_bundle_id.to_owned(),
    })
}

struct MacCapturer {
    own_bundle_id: String,
}

impl Capturer for MacCapturer {
    fn capture_frontmost(
        &self,
        is_excluded: &dyn Fn(&str, &str) -> bool,
    ) -> Result<Captured, Failure> {
        if !supported() {
            return Err(Failure::CaptureError);
        }
        if !screen_recording_granted() {
            return Err(Failure::Permission);
        }
        let content = SCShareableContent::get().map_err(|_| Failure::CaptureError)?;
        let snapshot = content.snapshot().ok_or(Failure::CaptureError)?;
        let own_pid = i32::try_from(std::process::id()).unwrap_or(i32::MAX);
        let front = NSWorkspace::sharedWorkspace().frontmostApplication();
        let front_bundle = front
            .as_ref()
            .and_then(|application| application.bundleIdentifier())
            .map(|bundle| bundle.to_string());
        let front_is_self = front_bundle
            .as_deref()
            .is_some_and(|bundle| bundle.eq_ignore_ascii_case(&self.own_bundle_id));

        let mut active = None;
        let mut first = None;
        for window in &snapshot.windows {
            if !window.is_on_screen
                || window.window_layer != 0
                || window.frame.size.width < 1.0
                || window.frame.size.height < 1.0
            {
                continue;
            }
            let Some(index) = window.owning_app_index else {
                continue;
            };
            let Some(application) = snapshot.applications.get(index) else {
                continue;
            };
            if application.process_id == own_pid
                || application
                    .bundle_identifier
                    .eq_ignore_ascii_case(&self.own_bundle_id)
            {
                continue;
            }
            if !front_is_self {
                let Some(front_bundle) = front_bundle.as_deref() else {
                    continue;
                };
                if !application
                    .bundle_identifier
                    .eq_ignore_ascii_case(front_bundle)
                {
                    continue;
                }
            }
            let candidate = (
                window.window_id,
                window.title.clone().unwrap_or_default(),
                application.application_name.clone(),
            );
            if window.is_active {
                active = Some(candidate);
                break;
            }
            if first.is_none() {
                first = Some(candidate);
            }
        }
        let Some((window_id, snapshot_title, snapshot_app_name)) = active.or(first) else {
            return Err(Failure::NoWindow);
        };
        let live = content
            .windows()
            .into_iter()
            .find(|window| window.window_id() == window_id)
            .ok_or(Failure::NoWindow)?;
        let application = live.owning_application().ok_or(Failure::NoWindow)?;
        let live_name = application.application_name();
        let app_name = if live_name.is_empty() {
            snapshot_app_name
        } else {
            live_name
        };
        let app_id = application.bundle_identifier();
        let window_title = live.title().unwrap_or(snapshot_title);
        if is_excluded(&app_id, &app_name) {
            return Err(Failure::Excluded { app_name });
        }

        let filter = SCContentFilter::create()
            .with_window(&live)
            .build()
            .map_err(|_| Failure::CaptureError)?;
        let (width, height) = SCShareableContentInfo::for_filter(&filter)
            .map(|info| info.pixel_size())
            .unwrap_or((0, 0));
        let (width, height) = if width > 0 && height > 0 {
            (width, height)
        } else {
            let frame = live.frame();
            if frame.size.width < 1.0 || frame.size.height < 1.0 {
                return Err(Failure::NoWindow);
            }
            (
                frame.size.width.min(f64::from(u32::MAX)) as u32,
                frame.size.height.min(f64::from(u32::MAX)) as u32,
            )
        };
        let configuration = SCStreamConfiguration::new()
            .with_width(width)
            .with_height(height);
        let image = SCScreenshotManager::capture_image(&filter, &configuration)
            .map_err(|_| Failure::CaptureError)?;
        let png = png_bytes(&image)?;
        Ok(Captured {
            png,
            app_name,
            window_title,
            app_id,
        })
    }
}

fn png_bytes(image: &CGImage) -> Result<Vec<u8>, Failure> {
    let mut rgba = image.rgba_data().map_err(|_| Failure::CaptureError)?;
    let width = isize::try_from(image.width()).map_err(|_| Failure::CaptureError)?;
    let height = isize::try_from(image.height()).map_err(|_| Failure::CaptureError)?;
    let bytes_per_row = width.checked_mul(4).ok_or(Failure::CaptureError)?;
    let height_usize = usize::try_from(height).map_err(|_| Failure::CaptureError)?;
    let expected = usize::try_from(bytes_per_row)
        .ok()
        .and_then(|row| row.checked_mul(height_usize))
        .ok_or(Failure::CaptureError)?;
    if rgba.len() < expected {
        return Err(Failure::CaptureError);
    }
    let plane = rgba.as_mut_ptr();
    let mut planes = [plane];
    // SAFETY: `planes` points at `rgba`, which stays alive until the PNG is encoded; the
    // dimensions and row size match the buffer.
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            planes.as_mut_ptr(),
            width,
            height,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            bytes_per_row,
            32,
        )
    }
    .ok_or(Failure::CaptureError)?;
    let properties = NSDictionary::<NSBitmapImageRepPropertyKey, AnyObject>::new();
    // SAFETY: an empty properties dictionary is valid for every image file type.
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
    }
    .ok_or(Failure::CaptureError)?;
    Ok(data.to_vec())
}
