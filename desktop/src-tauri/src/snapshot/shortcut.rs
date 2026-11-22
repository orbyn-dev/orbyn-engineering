//! One global shortcut, registered from Rust only. The page never gets a shortcut permission, so it can neither
//! register nor unregister one.

use std::str::FromStr;

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use super::settings::{Chord, Settings};

#[derive(Default)]
pub struct Registration {
    pub registered: bool,
    pub message: Option<String>,
    shortcut: Option<Shortcut>,
}

pub fn apply(app: &AppHandle, settings: &Settings, registration: &mut Registration) {
    if let Some(previous) = registration.shortcut.take() {
        let _ = app.global_shortcut().unregister(previous);
    }
    registration.registered = false;
    registration.message = None;
    if !settings.snapshot_enabled {
        return;
    }
    let chord = settings.snapshot_shortcut.clone();
    let Ok(shortcut) = parse_chord(&chord) else {
        registration.message = Some(format!("Another app uses {}.", chord_text(&chord)));
        return;
    };
    let result = app
        .global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            if app.state::<super::SnapShot>().enabled() {
                // The page starts the capture, so a shortcut only asks it to do what the button does.
                let _ = app.emit_to("main", "snapshot://requested", json!({}));
            }
        });
    match result {
        Ok(()) => {
            registration.registered = true;
            registration.shortcut = Some(shortcut);
        }
        Err(_) => {
            registration.message = Some(format!("Another app uses {}.", chord_text(&chord)));
        }
    }
}

fn key_token(key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    let token = if key.chars().count() == 1 {
        if key == "+" {
            return None;
        }
        if key == " " {
            "Space".to_owned()
        } else {
            key.to_owned()
        }
    } else {
        key.to_owned()
    };
    Shortcut::from_str(&format!("super+{token}"))
        .ok()
        .map(|_| token)
}

pub fn parse_chord(chord: &Chord) -> Result<Shortcut, ()> {
    let key = key_token(&chord.key).ok_or(())?;
    let mut parts: Vec<&str> = Vec::new();
    let mut control = false;
    if chord.mod_key {
        parts.push(if cfg!(target_os = "macos") {
            "super"
        } else {
            "control"
        });
        control = !cfg!(target_os = "macos");
    }
    if chord.ctrl && !control {
        parts.push("control");
    }
    if chord.alt {
        parts.push("alt");
    }
    if chord.shift {
        parts.push("shift");
    }
    parts.push(key.as_str());
    Shortcut::from_str(&parts.join("+")).map_err(|_| ())
}

pub fn chord_text(chord: &Chord) -> String {
    let mut parts = Vec::new();
    let mut control = false;
    if chord.mod_key {
        parts.push(if cfg!(target_os = "macos") {
            "Cmd".to_owned()
        } else {
            "Ctrl".to_owned()
        });
        control = !cfg!(target_os = "macos");
    }
    if chord.ctrl && !control {
        parts.push("Ctrl".to_owned());
    }
    if chord.alt {
        parts.push(if cfg!(target_os = "macos") {
            "Option".to_owned()
        } else {
            "Alt".to_owned()
        });
    }
    if chord.shift {
        parts.push("Shift".to_owned());
    }
    parts.push(display_key(&chord.key));
    parts.join("+")
}

fn display_key(key: &str) -> String {
    let mut characters = key.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    let rest = characters.as_str();
    if rest.is_empty() && first == ' ' {
        return "Space".to_owned();
    }
    format!("{}{rest}", first.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    fn chord(key: &str, mod_key: bool, shift: bool, alt: bool, ctrl: bool) -> Chord {
        Chord {
            key: key.to_owned(),
            mod_key,
            shift,
            alt,
            ctrl,
            ..Chord::default()
        }
    }

    #[test]
    fn the_default_chord_reads_as_a_person_writes_it() {
        let text = chord_text(&Chord::default());
        if cfg!(target_os = "macos") {
            assert_eq!(text, "Cmd+Shift+9");
        } else {
            assert_eq!(text, "Ctrl+Shift+9");
        }
    }

    #[test]
    fn chord_text_orders_and_deduplicates_the_modifiers() {
        let text = chord_text(&chord("k", true, true, true, true));
        if cfg!(target_os = "macos") {
            assert_eq!(text, "Cmd+Ctrl+Option+Shift+K");
        } else {
            assert_eq!(text, "Ctrl+Alt+Shift+K");
        }
        assert_eq!(
            chord_text(&chord(" ", false, false, false, true)),
            "Ctrl+Space"
        );
    }

    #[test]
    fn a_chord_becomes_the_plugin_shortcut() {
        let shortcut = parse_chord(&Chord::default()).unwrap();
        assert_eq!(shortcut.key, Code::Digit9);
        if cfg!(target_os = "macos") {
            assert_eq!(shortcut.mods, Modifiers::SUPER | Modifiers::SHIFT);
        } else {
            assert_eq!(shortcut.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        }
        let shortcut = parse_chord(&chord("F1", false, false, true, true)).unwrap();
        assert_eq!(shortcut.key, Code::F1);
        assert_eq!(shortcut.mods, Modifiers::CONTROL | Modifiers::ALT);
    }

    #[test]
    fn a_key_the_plugin_cannot_read_is_refused_without_registering() {
        for key in ["", "10", "nope", "+", "shift"] {
            assert!(
                parse_chord(&chord(key, true, false, false, false)).is_err(),
                "{key}"
            );
        }
    }
}
