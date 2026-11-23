//! The wire state the page reads, computed from the settings, the shortcut registration and platform facts.

use serde::Serialize;

use super::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedsSetup {
    ScreenRecording,
    Accessibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Permissions {
    pub screen_recording: bool,
    pub accessibility: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapShotState {
    pub supported: bool,
    pub platform: String,
    pub enabled: bool,
    pub shortcut_registered: bool,
    pub shortcut_message: Option<String>,
    pub permissions: Permissions,
    pub needs_setup: Option<NeedsSetup>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformFacts {
    pub supported: bool,
    pub platform: &'static str,
    pub message: Option<String>,
    pub permissions: Permissions,
}

pub fn compute(
    settings: &Settings,
    shortcut_registered: bool,
    shortcut_message: Option<String>,
    facts: &PlatformFacts,
) -> SnapShotState {
    let needs_setup = if !settings.snapshot_enabled {
        None
    } else if !facts.permissions.screen_recording {
        Some(NeedsSetup::ScreenRecording)
    } else if settings.snapshot_include_accessibility && !facts.permissions.accessibility {
        Some(NeedsSetup::Accessibility)
    } else {
        None
    };
    SnapShotState {
        supported: facts.supported,
        platform: facts.platform.to_owned(),
        enabled: settings.snapshot_enabled,
        shortcut_registered,
        shortcut_message,
        permissions: facts.permissions,
        needs_setup,
        message: facts.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> PlatformFacts {
        PlatformFacts {
            supported: true,
            platform: "macos",
            message: None,
            permissions: Permissions {
                screen_recording: false,
                accessibility: false,
            },
        }
    }

    #[test]
    fn setup_names_what_blocks_the_next_capture() {
        let mut settings = Settings::default();
        let mut facts = facts();
        assert_eq!(compute(&settings, false, None, &facts).needs_setup, None);

        settings.snapshot_enabled = true;
        assert_eq!(
            compute(&settings, true, None, &facts).needs_setup,
            Some(NeedsSetup::ScreenRecording)
        );

        facts.permissions.screen_recording = true;
        assert_eq!(compute(&settings, true, None, &facts).needs_setup, None);

        settings.snapshot_include_accessibility = true;
        assert_eq!(
            compute(&settings, true, None, &facts).needs_setup,
            Some(NeedsSetup::Accessibility)
        );

        facts.permissions.accessibility = true;
        assert_eq!(compute(&settings, true, None, &facts).needs_setup, None);
    }

    #[test]
    fn accessibility_blocks_only_when_the_text_setting_asks_for_it() {
        let mut settings = Settings {
            snapshot_enabled: true,
            ..Settings::default()
        };
        let mut facts = facts();
        facts.permissions.screen_recording = true;
        assert_eq!(compute(&settings, true, None, &facts).needs_setup, None);

        settings.snapshot_include_accessibility = true;
        assert_eq!(
            compute(&settings, true, None, &facts).needs_setup,
            Some(NeedsSetup::Accessibility)
        );

        settings.snapshot_include_accessibility = false;
        assert_eq!(compute(&settings, true, None, &facts).needs_setup, None);
    }

    #[test]
    fn the_state_echoes_the_setting_and_facts() {
        let settings = Settings {
            snapshot_enabled: true,
            ..Settings::default()
        };
        let facts = PlatformFacts {
            supported: false,
            platform: "macos",
            message: Some("Snap Shot needs macOS 14 or newer.".to_owned()),
            permissions: Permissions {
                screen_recording: true,
                accessibility: true,
            },
        };
        let state = compute(
            &settings,
            true,
            Some("Another app uses Cmd+Shift+9.".to_owned()),
            &facts,
        );
        assert!(state.enabled);
        assert!(!state.supported);
        assert_eq!(state.platform, "macos");
        assert!(state.shortcut_registered);
        assert_eq!(
            state.shortcut_message.as_deref(),
            Some("Another app uses Cmd+Shift+9.")
        );
        assert_eq!(
            state.message.as_deref(),
            Some("Snap Shot needs macOS 14 or newer.")
        );
        assert_eq!(
            serde_json::to_value(&state).unwrap(),
            serde_json::json!({
                "supported": false,
                "platform": "macos",
                "enabled": true,
                "shortcut_registered": true,
                "shortcut_message": "Another app uses Cmd+Shift+9.",
                "permissions": { "screen_recording": true, "accessibility": true },
                "needs_setup": null,
                "message": "Snap Shot needs macOS 14 or newer."
            })
        );
    }
}
