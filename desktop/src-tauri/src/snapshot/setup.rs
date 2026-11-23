//! Setup prompts behind a platform seam. One screen-recording attempt at a time, and never more than once per two
//! seconds, so a second click cannot show a second prompt or a second System Settings window.

use std::time::{Duration, Instant};

const THROTTLE: Duration = Duration::from_secs(2);

pub(crate) trait SetupPlatform {
    fn screen_recording_granted(&self) -> bool;
    fn request_screen_recording(&self);
    fn open_screen_recording_settings(&self);
}

#[derive(Default)]
pub(crate) struct ScreenRecordingSetup {
    last_attempt: Option<Instant>,
}

impl ScreenRecordingSetup {
    /// Reads the grant first: a granted permission changes nothing. Otherwise asks macOS once, rereads the grant and,
    /// only when it is still missing, opens the Screen Recording pane. The throttle starts at the request, so a second
    /// click inside two seconds does nothing.
    pub(crate) fn attempt(&mut self, platform: &dyn SetupPlatform, now: Instant) {
        if platform.screen_recording_granted() {
            return;
        }
        if self
            .last_attempt
            .is_some_and(|last| now.saturating_duration_since(last) < THROTTLE)
        {
            return;
        }
        self.last_attempt = Some(now);
        platform.request_screen_recording();
        if !platform.screen_recording_granted() {
            platform.open_screen_recording_settings();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    use super::super::SnapShot;
    use super::super::state::{Permissions, PlatformFacts, SnapShotState};
    use super::*;

    struct FakePlatform {
        granted: Cell<bool>,
        grants_on_request: bool,
        requests: Cell<usize>,
        opens: Cell<usize>,
    }

    impl FakePlatform {
        fn new(granted: bool, grants_on_request: bool) -> Self {
            Self {
                granted: Cell::new(granted),
                grants_on_request,
                requests: Cell::new(0),
                opens: Cell::new(0),
            }
        }
    }

    impl SetupPlatform for FakePlatform {
        fn screen_recording_granted(&self) -> bool {
            self.granted.get()
        }

        fn request_screen_recording(&self) {
            self.requests.set(self.requests.get() + 1);
            if self.grants_on_request {
                self.granted.set(true);
            }
        }

        fn open_screen_recording_settings(&self) {
            self.opens.set(self.opens.get() + 1);
        }
    }

    fn facts(screen_recording: bool) -> PlatformFacts {
        PlatformFacts {
            supported: true,
            platform: "macos",
            message: None,
            permissions: Permissions {
                screen_recording,
                accessibility: false,
            },
        }
    }

    fn state_after(snapshot: &SnapShot, platform: &FakePlatform, now: Instant) -> SnapShotState {
        snapshot
            .allow_screen_recording_with(platform, || facts(platform.granted.get()), now)
            .expect("the setup lock is free")
    }

    #[test]
    fn a_granted_screen_recording_changes_nothing_and_answers_granted() {
        let snapshot = SnapShot::default();
        let platform = FakePlatform::new(true, false);
        let state = state_after(&snapshot, &platform, Instant::now());
        assert!(state.permissions.screen_recording);
        assert_eq!(platform.requests.get(), 0);
        assert_eq!(platform.opens.get(), 0);
    }

    #[test]
    fn a_missing_grant_requests_and_opens_settings_only_when_still_missing() {
        let snapshot = SnapShot::default();
        let platform = FakePlatform::new(false, false);
        let state = state_after(&snapshot, &platform, Instant::now());
        assert_eq!(platform.requests.get(), 1);
        assert_eq!(platform.opens.get(), 1);
        assert!(!state.permissions.screen_recording);
    }

    #[test]
    fn a_grant_from_the_request_is_answered_after_the_request_without_opening_settings() {
        let snapshot = SnapShot::default();
        let platform = FakePlatform::new(false, true);
        let state = state_after(&snapshot, &platform, Instant::now());
        assert_eq!(platform.requests.get(), 1);
        assert_eq!(platform.opens.get(), 0);
        assert!(state.permissions.screen_recording);
    }

    #[test]
    fn a_second_attempt_inside_two_seconds_does_nothing_and_answers_the_current_state() {
        let snapshot = SnapShot::default();
        let platform = FakePlatform::new(false, false);
        let start = Instant::now();
        let _ = state_after(&snapshot, &platform, start);
        let throttled = state_after(&snapshot, &platform, start + Duration::from_millis(1_999));
        assert!(!throttled.permissions.screen_recording);
        assert_eq!(platform.requests.get(), 1);
        assert_eq!(platform.opens.get(), 1);

        let _ = state_after(&snapshot, &platform, start + Duration::from_secs(2));
        assert_eq!(platform.requests.get(), 2);
        assert_eq!(platform.opens.get(), 2);
    }
}
