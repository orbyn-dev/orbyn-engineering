//! Windows and Linux build the shell state without support. Windows keeps the real capturer
//! compiled and returns it from `capturer`, but `supported` stays false, so the flow never
//! constructs it; Linux always answers a capture error. The page hides Snap Shot either way.

#![cfg(not(target_os = "macos"))]

use super::flow::Capturer;
#[cfg(not(windows))]
use super::flow::{Captured, Failure};
use super::setup::SetupPlatform;
use super::state::{Permissions, PlatformFacts};

pub fn facts() -> PlatformFacts {
    PlatformFacts {
        supported: false,
        platform: if cfg!(windows) { "windows" } else { "linux" },
        message: None,
        permissions: Permissions {
            screen_recording: false,
            accessibility: false,
        },
    }
}

pub fn setup_platform() -> impl SetupPlatform {
    StubSetup
}

struct StubSetup;

impl SetupPlatform for StubSetup {
    fn screen_recording_granted(&self) -> bool {
        false
    }

    fn request_screen_recording(&self) {}

    fn open_screen_recording_settings(&self) {}
}

#[cfg(windows)]
pub fn capturer(own_bundle_id: &str, include_accessibility: bool) -> Box<dyn Capturer> {
    super::windows::capturer(own_bundle_id, include_accessibility)
}

#[cfg(not(windows))]
pub fn capturer(_own_bundle_id: &str, _include_accessibility: bool) -> Box<dyn Capturer> {
    Box::new(StubCapturer)
}

#[cfg(not(windows))]
struct StubCapturer;

#[cfg(not(windows))]
impl Capturer for StubCapturer {
    fn capture_frontmost(
        &self,
        _is_excluded: &dyn Fn(&str, &str) -> bool,
    ) -> Result<Captured, Failure> {
        Err(Failure::CaptureError)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::super::SnapShot;
    use super::*;

    #[test]
    fn the_stub_is_unsupported_and_never_captures() {
        let facts = facts();
        assert!(!facts.supported);
        assert_eq!(
            facts.platform,
            if cfg!(windows) { "windows" } else { "linux" }
        );
        assert!(facts.message.is_none());
        #[cfg(not(windows))]
        {
            let capturer = capturer("com.example.own", true);
            assert!(matches!(
                capturer.capture_frontmost(&|_, _| false),
                Err(Failure::CaptureError)
            ));
        }
        #[cfg(windows)]
        {
            // The capturer is never reached: `supported` stays false on Windows.
            assert!(!facts.supported);
        }
    }

    #[test]
    fn the_stub_setup_does_nothing_and_answers_the_unsupported_state() {
        let platform = setup_platform();
        assert!(!platform.screen_recording_granted());
        platform.request_screen_recording();
        platform.open_screen_recording_settings();
        assert!(!platform.screen_recording_granted());

        let snapshot = SnapShot::default();
        let state = snapshot
            .allow_screen_recording_with(&platform, facts, Instant::now())
            .expect("the setup lock is free");
        assert!(!state.supported);
        assert!(!state.permissions.screen_recording);
        assert!(!state.permissions.accessibility);
    }
}
