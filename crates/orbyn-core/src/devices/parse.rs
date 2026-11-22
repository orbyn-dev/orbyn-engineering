//! Pure parsers for the platform tools' text. Nothing here starts a process, so
//! every shape can be tested with a string.

use orbyn_contracts::{
    Device, DeviceFold, DeviceOrientation, DevicePlatform, DeviceState, FoldPosture,
};
use serde_json::Value;

/// A booted emulator's serial from `adb devices`. Only `emulator-*` serials are
/// local AVDs; a physical phone is not in this contract's list.
pub(super) fn adb_emulator_serials(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let serial = words.next()?;
            let state = words.next()?;
            (state == "device" && serial.starts_with("emulator-")).then(|| serial.to_owned())
        })
        .collect()
}

/// `emulator -list-avds` prints one AVD name per line.
pub(super) fn avd_names(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The first non-empty line, which is the AVD name in `adb emu avd name`.
pub(super) fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// `DeviceState{identifier=1, name='HALF_OPENED'}` rows from
/// `cmd device_state print-states`, with an output line each.
pub(super) fn device_states(text: &str) -> Vec<(i64, String)> {
    text.lines()
        .filter_map(|line| {
            let identifier = field(line, "identifier")?.parse().ok()?;
            let name = field(line, "name")?;
            Some((identifier, name.to_owned()))
        })
        .collect()
}

/// `cmd device_state print-state` prints the current identifier, and some images
/// print the whole `Committed state:` line instead.
pub(super) fn current_state_id(text: &str) -> Option<i64> {
    if let Some(id) = text
        .lines()
        .find_map(|line| line.trim().parse::<i64>().ok())
    {
        return Some(id);
    }
    text.lines()
        .find(|line| line.contains("Committed state:"))
        .and_then(|line| field(line, "identifier"))
        .and_then(|id| id.parse().ok())
}

/// The tool's own posture name. The angle never decides this: a half-open device
/// can report any angle, and the emulator may call 130 something else.
pub(super) fn posture(name: &str) -> Option<FoldPosture> {
    match name.trim().to_ascii_uppercase().replace('-', "_").as_str() {
        "CLOSED" => Some(FoldPosture::Closed),
        "HALF_OPENED" | "HALF_OPEN" => Some(FoldPosture::HalfOpened),
        "OPENED" | "OPEN" => Some(FoldPosture::Opened),
        "FLIPPED" => Some(FoldPosture::Flipped),
        "TENT" => Some(FoldPosture::Tent),
        _ => None,
    }
}

/// The `SurfaceOrientation` value in `dumpsys input`: the display's rotation.
pub(super) fn surface_orientation(text: &str) -> Option<DeviceOrientation> {
    let value = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("SurfaceOrientation:"))
        .filter_map(|value| value.trim().parse::<u8>().ok())
        .last()?;
    match value {
        0 => Some(DeviceOrientation::Portrait),
        1 => Some(DeviceOrientation::LandscapeLeft),
        2 => Some(DeviceOrientation::PortraitUpsideDown),
        3 => Some(DeviceOrientation::LandscapeRight),
        _ => None,
    }
}

/// `adb emu sensor get hinge-angle0` prints a number, possibly with `OK`.
pub(super) fn hinge_angle(text: &str) -> Option<f64> {
    text.split_whitespace()
        .find_map(|word| word.rsplit('=').next().and_then(|value| value.parse().ok()))
        .filter(|angle: &f64| angle.is_finite())
}

/// `xcrun simctl list devices --json`: the available iOS simulators, with the
/// version from their runtime key. Other runtimes (watchOS, tvOS) are not devices
/// this contract controls.
pub(super) fn simctl_devices(text: &str) -> Option<Vec<Device>> {
    let root: Value = serde_json::from_str(text).ok()?;
    let runtimes = root.get("devices")?.as_object()?;
    let mut devices = Vec::new();
    for (runtime, list) in runtimes {
        let Some(version) = runtime_version(runtime) else {
            continue;
        };
        let Some(list) = list.as_array() else {
            continue;
        };
        for item in list {
            if item.get("isAvailable").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let Some(udid) = item.get("udid").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            let id = format!("ios:{udid}");
            if !valid_id(&id) || name.is_empty() {
                continue;
            }
            let state = match item.get("state").and_then(Value::as_str) {
                Some("Booted") => DeviceState::Booted,
                _ => DeviceState::Shutdown,
            };
            devices.push(Device {
                id,
                name: name.to_owned(),
                platform: DevicePlatform::Ios,
                state,
                version: version.clone(),
                foldable: false,
                fold: None,
                orientation: None,
            });
        }
    }
    Some(devices)
}

/// `com.apple.CoreSimulator.SimRuntime.iOS-17-0` is version `17.0`; a non-iOS
/// runtime has no version and is skipped by the list.
pub(super) fn runtime_version(runtime: &str) -> Option<String> {
    let suffix = runtime.rsplit_once(".iOS-").map(|(_, suffix)| suffix)?;
    if suffix.is_empty() {
        return None;
    }
    let version = suffix.replace('-', ".");
    (!version.is_empty()
        && version
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.'))
    .then_some(version)
}

/// A shutdown AVD's `config.ini` declares a foldable through its posture or hinge
/// sensors or an explicit flag. The display name is never used.
pub(super) fn config_declares_foldable(text: &str) -> bool {
    text.lines().any(|line| {
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        let (key, value) = (key.trim(), value.trim());
        key.starts_with("hw.sensors.posture")
            || key.starts_with("hw.sensors.hinge")
            || (key == "hw.device.foldable" && value.eq_ignore_ascii_case("true"))
    })
}

/// The AVD's config file, when the AVD name is a single safe path component. A
/// tool-supplied name is still not trusted to escape the AVD folder.
pub(super) fn avd_config_path(
    avd_home: &std::path::Path,
    name: &str,
) -> Option<std::path::PathBuf> {
    use std::path::Component;

    let mut components = std::path::Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => {
            Some(avd_home.join(format!("{name}.avd")).join("config.ini"))
        }
        _ => None,
    }
}

/// A device id the service may return and later accept in a path: the platform
/// prefix plus a name the tools can use without becoming argv syntax.
pub(super) fn valid_id(id: &str) -> bool {
    let Some((prefix, raw)) = id.split_once(':') else {
        return false;
    };
    if !matches!(prefix, "android" | "ios") || raw.is_empty() || id.chars().count() > 256 {
        return false;
    }
    !raw.starts_with('-')
        && !raw.contains("..")
        && raw != "."
        && !raw
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
}

/// `DeviceFold` from the tool's posture rows: the current identifier picks the
/// row, and the sensor reports the angle separately.
pub(super) fn fold_from_states(
    states: &[(i64, String)],
    current: Option<i64>,
    angle: Option<f64>,
) -> Option<DeviceFold> {
    let posture = current
        .and_then(|id| states.iter().find(|(identifier, _)| *identifier == id))
        .and_then(|(_, name)| posture(name));
    Some(DeviceFold {
        posture,
        hinge_angle: angle,
    })
}

/// The states that prove a booted device is foldable: at least one supported
/// state has a fold posture name.
pub(super) fn supports_fold(states: &[(i64, String)]) -> bool {
    states.iter().any(|(_, name)| posture(name).is_some())
}

fn field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let start = line.find(&format!("{name}="))? + name.len() + 1;
    let rest = &line[start..];
    if let Some(rest) = rest.strip_prefix('\'') {
        return rest.split('\'').next();
    }
    rest.split([',', '}', ' ']).next().map(str::trim)
}
