//! The fixed argv table. A client value only ever fills a validated enum or the
//! checked 1-to-179 angle; it never becomes a program, subcommand or flag.
//!
//! Every command below is marked `UNVERIFIED`. T3's device files
//! (`packages/contracts/src/device.ts`, `apps/server/src/device/DeviceActions.ts`,
//! `DeviceHubOperations.ts` at `9168ca73d`) give the shape of the actions; no T3
//! code was copied. `adb emu fold`, `adb emu unfold`, the `hinge-angle0` sensor
//! command and the `device_state` commands come from the Android emulator/AOSP
//! documentation. None was run on a real AVD, emulator console or simulator while
//! this service was built. The docs carry the same list.

use std::path::Path;

use orbyn_contracts::{DeviceActionInput, DeviceButton, DevicePlatform, HingePreset};

/// The argv for one action.
pub(super) struct Commands {
    pub(super) argv: Vec<Vec<String>>,
}

/// The only way an action becomes argv. `tool_id` is the stored adb serial or
/// simulator UDID, and `screenshot_file` is the service's own temporary path,
/// used by `xcrun simctl io`; neither is ever client text.
pub(super) fn action_commands(
    platform: DevicePlatform,
    action: &DeviceActionInput,
    tool_id: &str,
    screenshot_file: Option<&Path>,
) -> Option<Commands> {
    match platform {
        DevicePlatform::Android => android_commands(action, tool_id),
        DevicePlatform::Ios => ios_commands(action, tool_id, screenshot_file),
    }
}

// UNVERIFIED: `emulator -list-avds`.
// UNVERIFIED: `adb devices`.
// UNVERIFIED: `adb -s <serial> emu avd name`.
// UNVERIFIED: `adb -s <serial> shell getprop ro.build.version.release`.
// UNVERIFIED: `adb -s <serial> shell dumpsys input` for `SurfaceOrientation`.
// UNVERIFIED: `adb -s <serial> shell cmd device_state print-states` and
// `print-state` (AOSP `DeviceStateManagerShellCommand` names these).
// UNVERIFIED: `adb -s <serial> emu sensor get hinge-angle0`.
// UNVERIFIED: `adb -s <serial> emu fold` and `adb -s <serial> emu unfold`.
// UNVERIFIED: `adb -s <serial> emu sensor set hinge-angle0 <angle>`.
// UNVERIFIED: `adb -s <serial> exec-out screencap -p`.
// UNVERIFIED: `adb -s <serial> shell settings put system ...`.
// UNVERIFIED: `adb -s <serial> shell input keyevent ...`.
// UNVERIFIED: `adb -s <serial> emu kill`.
fn android_commands(action: &DeviceActionInput, serial: &str) -> Option<Commands> {
    let adb = |rest: &[&str]| {
        let mut argv = vec!["-s".to_owned(), serial.to_owned()];
        argv.extend(rest.iter().map(|part| (*part).to_owned()));
        argv
    };
    let argv = match action {
        DeviceActionInput::Screenshot { .. } => {
            vec![adb(&["exec-out", "screencap", "-p"])]
        }
        DeviceActionInput::Rotate { value } => {
            // User rotation is locked first, then set; the client owns cycling.
            let rotation = match value {
                orbyn_contracts::DeviceOrientation::Portrait => "0",
                orbyn_contracts::DeviceOrientation::LandscapeLeft => "1",
                orbyn_contracts::DeviceOrientation::PortraitUpsideDown => "2",
                orbyn_contracts::DeviceOrientation::LandscapeRight => "3",
            };
            vec![
                adb(&[
                    "shell",
                    "settings",
                    "put",
                    "system",
                    "accelerometer_rotation",
                    "0",
                ]),
                adb(&[
                    "shell",
                    "settings",
                    "put",
                    "system",
                    "user_rotation",
                    rotation,
                ]),
            ]
        }
        DeviceActionInput::Button { value } => {
            let key = match value {
                DeviceButton::Home => "KEYCODE_HOME",
                DeviceButton::Back => "KEYCODE_BACK",
                DeviceButton::Recents => "KEYCODE_APP_SWITCH",
            };
            vec![adb(&["shell", "input", "keyevent", key])]
        }
        DeviceActionInput::PowerOff { .. } => vec![adb(&["emu", "kill"])],
        DeviceActionInput::Fold { .. } => vec![adb(&["emu", "fold"])],
        DeviceActionInput::Unfold { .. } => vec![adb(&["emu", "unfold"])],
        DeviceActionInput::Hinge { angle } => {
            let angle = angle.to_string();
            vec![adb(&["emu", "sensor", "set", "hinge-angle0", &angle])]
        }
        DeviceActionInput::HingePreset { preset } => match preset {
            HingePreset::Closed => vec![adb(&["emu", "fold"])],
            HingePreset::HalfOpen => vec![adb(&["emu", "sensor", "set", "hinge-angle0", "130"])],
            HingePreset::Open => vec![adb(&["emu", "unfold"])],
        },
    };
    Some(Commands { argv })
}

// UNVERIFIED: `xcrun simctl list devices --json`.
// UNVERIFIED: `xcrun simctl io <udid> screenshot --type=png <file>`.
// UNVERIFIED: `xcrun simctl ui <udid> home`.
// UNVERIFIED: `xcrun simctl shutdown <udid>`.
fn ios_commands(
    action: &DeviceActionInput,
    udid: &str,
    screenshot_file: Option<&Path>,
) -> Option<Commands> {
    let simctl = |rest: &[&str]| {
        let mut argv = vec!["simctl".to_owned()];
        argv.extend(rest.iter().map(|part| (*part).to_owned()));
        argv
    };
    match action {
        DeviceActionInput::Screenshot { .. } => {
            let file = screenshot_file?;
            let file = file.to_str()?;
            Some(Commands {
                argv: vec![simctl(&["io", udid, "screenshot", "--type=png", file])],
            })
        }
        DeviceActionInput::Button {
            value: DeviceButton::Home,
        } => Some(Commands {
            argv: vec![simctl(&["ui", udid, "home"])],
        }),
        // iOS has no back or recents button in this contract.
        DeviceActionInput::Button { .. } => None,
        DeviceActionInput::PowerOff { .. } => Some(Commands {
            argv: vec![simctl(&["shutdown", udid])],
        }),
        // Rotate, fold and hinge are Android-only; iOS is never foldable.
        DeviceActionInput::Rotate { .. }
        | DeviceActionInput::Fold { .. }
        | DeviceActionInput::Unfold { .. }
        | DeviceActionInput::Hinge { .. }
        | DeviceActionInput::HingePreset { .. } => None,
    }
}
