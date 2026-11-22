//! Windows and Linux build the shell state but cannot capture yet. Every capture is a capture error, and the page
//! hides Snap Shot because `supported` is false.

#![cfg(not(target_os = "macos"))]

use super::flow::{Captured, Capturer, Failure};
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

pub fn capturer(_own_bundle_id: &str) -> Box<dyn Capturer> {
    Box::new(StubCapturer)
}

struct StubCapturer;

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
        let capturer = capturer("com.example.own");
        assert!(matches!(
            capturer.capture_frontmost(&|_, _| false),
            Err(Failure::CaptureError)
        ));
    }
}
