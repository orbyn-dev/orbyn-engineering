//! The Snap Shot client settings from the K49 contract. The page keeps them and resends them on start and on every
//! change; the shell keeps the last set in memory only.

use serde::{Deserialize, Serialize};

pub const MAX_EXCLUDED_APPS: usize = 64;
pub const MAX_EXCLUDED_APP_LEN: usize = 255;

const NEEDS_MODIFIER: &str = "snapshot_shortcut needs at least one modifier";
const BAD_KEY: &str = "snapshot_shortcut needs a single-character or named key";
const TOO_MANY_APPS: &str = "snapshot_excluded_apps has more than 64 entries";
const APP_TOO_LONG: &str = "snapshot_excluded_apps entries must be at most 255 characters";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutKind {
    Chord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chord {
    pub kind: ShortcutKind,
    pub key: String,
    #[serde(default, rename = "mod")]
    pub mod_key: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub ctrl: bool,
}

impl Default for Chord {
    fn default() -> Self {
        Self {
            kind: ShortcutKind::Chord,
            key: "9".to_owned(),
            mod_key: true,
            shift: true,
            alt: false,
            ctrl: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sound {
    #[default]
    SoftPop,
    CameraShutter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub snapshot_enabled: bool,
    pub snapshot_shortcut: Chord,
    pub snapshot_include_accessibility: bool,
    pub snapshot_play_sound: bool,
    pub snapshot_sound: Sound,
    pub snapshot_flash: bool,
    pub snapshot_animations: bool,
    pub snapshot_excluded_apps: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            snapshot_enabled: false,
            snapshot_shortcut: Chord::default(),
            snapshot_include_accessibility: false,
            snapshot_play_sound: true,
            snapshot_sound: Sound::default(),
            snapshot_flash: true,
            snapshot_animations: true,
            snapshot_excluded_apps: Vec::new(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), &'static str> {
        let shortcut = &self.snapshot_shortcut;
        if !(shortcut.mod_key || shortcut.shift || shortcut.alt || shortcut.ctrl) {
            return Err(NEEDS_MODIFIER);
        }
        super::shortcut::parse_chord(shortcut).map_err(|()| BAD_KEY)?;
        if self.snapshot_excluded_apps.len() > MAX_EXCLUDED_APPS {
            return Err(TOO_MANY_APPS);
        }
        if self
            .snapshot_excluded_apps
            .iter()
            .any(|app| app.chars().count() > MAX_EXCLUDED_APP_LEN)
        {
            return Err(APP_TOO_LONG);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_contract() {
        let settings = Settings::default();
        assert!(!settings.snapshot_enabled);
        assert_eq!(settings.snapshot_shortcut, Chord::default());
        assert_eq!(settings.snapshot_shortcut.kind, ShortcutKind::Chord);
        assert_eq!(settings.snapshot_shortcut.key, "9");
        assert!(settings.snapshot_shortcut.mod_key);
        assert!(settings.snapshot_shortcut.shift);
        assert!(!settings.snapshot_shortcut.alt);
        assert!(!settings.snapshot_shortcut.ctrl);
        assert!(!settings.snapshot_include_accessibility);
        assert!(settings.snapshot_play_sound);
        assert_eq!(settings.snapshot_sound, Sound::SoftPop);
        assert!(settings.snapshot_flash);
        assert!(settings.snapshot_animations);
        assert!(settings.snapshot_excluded_apps.is_empty());
    }

    #[test]
    fn missing_fields_take_defaults_and_unknown_fields_fail() {
        let settings: Settings = serde_json::from_value(serde_json::json!({
            "snapshot_enabled": true,
            "snapshot_shortcut": { "kind": "chord", "key": "k", "mod": true }
        }))
        .unwrap();
        assert!(settings.snapshot_enabled);
        assert_eq!(settings.snapshot_shortcut.key, "k");
        assert!(!settings.snapshot_shortcut.shift);
        assert!(settings.snapshot_flash);
        assert!(
            serde_json::from_value::<Settings>(serde_json::json!({ "unknown": true })).is_err()
        );
        assert!(
            serde_json::from_value::<Settings>(serde_json::json!({
                "snapshot_shortcut": { "kind": "chord", "key": "9", "mod": true, "extra": true }
            }))
            .is_err()
        );
    }

    #[test]
    fn validation_bounds_the_shortcut_and_the_person_list() {
        let mut settings = Settings::default();
        assert!(settings.validate().is_ok());

        settings.snapshot_shortcut.mod_key = false;
        settings.snapshot_shortcut.shift = false;
        assert_eq!(settings.validate(), Err(NEEDS_MODIFIER));

        settings.snapshot_shortcut.mod_key = true;
        for key in ["", "10", "nope"] {
            settings.snapshot_shortcut.key = key.to_owned();
            assert_eq!(settings.validate(), Err(BAD_KEY));
        }
        for key in ["a", "9", "/", "F1", "Space", "ArrowUp", "KeyA"] {
            settings.snapshot_shortcut.key = key.to_owned();
            assert!(settings.validate().is_ok(), "{key}");
        }

        settings.snapshot_shortcut.key = "9".to_owned();
        settings.snapshot_excluded_apps = vec!["a".to_owned(); MAX_EXCLUDED_APPS];
        assert!(settings.validate().is_ok());
        settings.snapshot_excluded_apps.push("b".to_owned());
        assert_eq!(settings.validate(), Err(TOO_MANY_APPS));

        settings.snapshot_excluded_apps = vec!["x".repeat(MAX_EXCLUDED_APP_LEN)];
        assert!(settings.validate().is_ok());
        settings.snapshot_excluded_apps = vec!["x".repeat(MAX_EXCLUDED_APP_LEN + 1)];
        assert_eq!(settings.validate(), Err(APP_TOO_LONG));
    }

    #[test]
    fn only_a_chord_is_a_shortcut() {
        assert!(
            serde_json::from_value::<Chord>(serde_json::json!({
                "kind": "both_shift_keys",
                "key": "Shift"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<Chord>(serde_json::json!({
                "kind": "modifier_pair",
                "key": "Shift"
            }))
            .is_err()
        );
    }

    #[test]
    fn the_sound_uses_the_wire_names() {
        assert_eq!(
            serde_json::to_value(Sound::SoftPop).unwrap(),
            serde_json::json!("soft-pop")
        );
        assert_eq!(
            serde_json::to_value(Sound::CameraShutter).unwrap(),
            serde_json::json!("camera-shutter")
        );
    }
}
