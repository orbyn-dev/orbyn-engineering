//! Local devices and the actions a person can take on them (docs/devices.md).
//!
//! The service lists Android AVDs and iOS simulators from the platform tools and
//! never stores the list. An action is typed and carries no device id: the path
//! names the device.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// The platform tools that own a device.
pub enum DevicePlatform {
    Android,
    Ios,
}
}

wire_enum! {
/// The state the platform tool reports. A transition is not a state of its own
/// (T3 has `booted: boolean`).
pub enum DeviceState {
    Booted,
    Shutdown,
}
}

wire_enum! {
/// How a foldable is folded, as the tool names it. `flipped` and `tent` are read
/// only; this contract never sets them.
pub enum FoldPosture {
    Closed,
    HalfOpened,
    Opened,
    Flipped,
    Tent,
}
}

wire_enum! {
/// The display's current rotation.
pub enum DeviceOrientation {
    Portrait,
    LandscapeLeft,
    PortraitUpsideDown,
    LandscapeRight,
}
}

wire! {
/// A foldable's posture and hinge angle. Present only when the device is foldable
/// and booted; a field is `null` when the tool reports no value.
#[serde(deny_unknown_fields)]
pub struct DeviceFold {
    pub posture: Option<FoldPosture>,
    pub hinge_angle: Option<f64>,
}
}

wire! {
/// One local AVD or simulator. Ids are the service's own: `android:<avd>` or
/// `ios:<udid>`. The list is rebuilt from the platform tools on every read.
#[serde(deny_unknown_fields)]
pub struct Device {
    /// At most 256 characters; never comes from the client.
    pub id: String,
    pub name: String,
    pub platform: DevicePlatform,
    pub state: DeviceState,
    /// The OS version, empty when the tool does not report it.
    pub version: String,
    /// Proved from the tool, never from the device's display name.
    pub foldable: bool,
    /// Only when `foldable` and `state` is `booted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fold: Option<DeviceFold>,
    /// The current orientation, when the device is booted and the tool reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub orientation: Option<DeviceOrientation>,
}
}

wire! {
/// Whether a platform's tools exist and the list could be read.
#[serde(deny_unknown_fields)]
pub struct PlatformStatus {
    pub platform: DevicePlatform,
    pub available: bool,
    /// `tool_missing` or `tool_failed` when `available` is false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code: Option<String>,
    /// Fixed copy for the code, never raw tool output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}
}

wire! {
/// `GET /api/devices`.
#[serde(deny_unknown_fields)]
pub struct DeviceList {
    pub devices: Vec<Device>,
    pub platforms: Vec<PlatformStatus>,
}
}

wire_enum! {
/// One action type. The list is closed: the body's `type` selects exactly one.
pub enum DeviceActionType {
    Screenshot,
    Rotate,
    Button,
    PowerOff,
    Fold,
    Unfold,
    Hinge,
    HingePreset,
}
}

wire_enum! {
/// A hardware button an action presses.
pub enum DeviceButton {
    Home,
    Back,
    Recents,
}
}

wire_enum! {
/// A named hinge position. The angle is the service's, never the client's.
pub enum HingePreset {
    Closed,
    HalfOpen,
    Open,
}
}

wire! {
/// The typed body of `POST /api/devices/{id}/actions`. The body never carries a
/// device id; unknown fields are refused. `hinge.angle` is an integer 1 to 179.
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeviceActionInput {
    Screenshot {},
    Rotate { value: DeviceOrientation },
    Button { value: DeviceButton },
    PowerOff {},
    Fold {},
    Unfold {},
    Hinge { angle: i32 },
    HingePreset { preset: HingePreset },
}
}

impl DeviceActionInput {
    pub fn action_type(&self) -> DeviceActionType {
        match self {
            Self::Screenshot { .. } => DeviceActionType::Screenshot,
            Self::Rotate { .. } => DeviceActionType::Rotate,
            Self::Button { .. } => DeviceActionType::Button,
            Self::PowerOff { .. } => DeviceActionType::PowerOff,
            Self::Fold { .. } => DeviceActionType::Fold,
            Self::Unfold { .. } => DeviceActionType::Unfold,
            Self::Hinge { .. } => DeviceActionType::Hinge,
            Self::HingePreset { .. } => DeviceActionType::HingePreset,
        }
    }

    /// The values the schema itself refuses before any process starts.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Hinge { angle } => (1..=179).contains(angle),
            _ => true,
        }
    }
}

impl DeviceActionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Screenshot => "screenshot",
            Self::Rotate => "rotate",
            Self::Button => "button",
            Self::PowerOff => "power_off",
            Self::Fold => "fold",
            Self::Unfold => "unfold",
            Self::Hinge => "hinge",
            Self::HingePreset => "hinge_preset",
        }
    }
}

wire! {
/// `GET /api/devices/{id}`: the device's own fields, flat, plus the actions it
/// supports right now. The web reads `detail.id` and `detail.actions`.
pub struct DeviceDetail {
    #[serde(flatten)]
    #[ts(flatten)]
    pub device: Device,
    pub actions: Vec<DeviceActionType>,
}
}

wire! {
/// The answer to every action. `image` is a base64 PNG, present only for a
/// screenshot and capped at 16 MiB before encoding.
#[serde(deny_unknown_fields)]
pub struct DeviceActionOutput {
    pub ok: bool,
    pub device: Device,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub image: Option<String>,
}
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_action_is_tagged_and_unknown_fields_are_refused() {
        for body in [
            json!({ "type": "screenshot" }),
            json!({ "type": "rotate", "value": "landscape_left" }),
            json!({ "type": "button", "value": "recents" }),
            json!({ "type": "power_off" }),
            json!({ "type": "fold" }),
            json!({ "type": "unfold" }),
            json!({ "type": "hinge", "angle": 90 }),
            json!({ "type": "hinge_preset", "preset": "half_open" }),
        ] {
            serde_json::from_value::<DeviceActionInput>(body).unwrap();
        }
        assert!(serde_json::from_value::<DeviceActionInput>(json!({ "type": "tap" })).is_err());
        assert!(
            serde_json::from_value::<DeviceActionInput>(
                json!({ "type": "screenshot", "extra": true })
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<DeviceActionInput>(
                json!({ "type": "hinge", "angle": 90, "extra": true })
            )
            .is_err()
        );
    }

    #[test]
    fn a_device_detail_is_the_device_fields_plus_actions() {
        let detail = DeviceDetail {
            device: Device {
                id: "android:Pixel_8".into(),
                name: "Pixel 8".into(),
                platform: DevicePlatform::Android,
                state: DeviceState::Booted,
                version: "14".into(),
                foldable: false,
                fold: None,
                orientation: Some(DeviceOrientation::Portrait),
            },
            actions: vec![DeviceActionType::Screenshot, DeviceActionType::Rotate],
        };
        let value = serde_json::to_value(detail).unwrap();
        assert_eq!(value["id"], "android:Pixel_8");
        assert_eq!(value["name"], "Pixel 8");
        assert_eq!(value["actions"], json!(["screenshot", "rotate"]));
        assert!(value.get("device").is_none(), "{value}");
    }

    #[test]
    fn an_action_serializes_as_its_tag_alone() {
        assert_eq!(
            serde_json::to_value(DeviceActionInput::Screenshot {}).unwrap(),
            json!({ "type": "screenshot" })
        );
        assert_eq!(
            serde_json::to_value(DeviceActionInput::Hinge { angle: 90 }).unwrap(),
            json!({ "type": "hinge", "angle": 90 })
        );
        assert_eq!(
            serde_json::to_value(DeviceActionInput::Button {
                value: DeviceButton::Back
            })
            .unwrap(),
            json!({ "type": "button", "value": "back" })
        );
    }

    #[test]
    fn the_hinge_range_belongs_to_the_schema() {
        for angle in [1, 90, 179] {
            let input: DeviceActionInput =
                serde_json::from_value(json!({ "type": "hinge", "angle": angle })).unwrap();
            assert!(input.is_valid());
        }
        for angle in [0, 180, 1000, -1] {
            let input: DeviceActionInput =
                serde_json::from_value(json!({ "type": "hinge", "angle": angle })).unwrap();
            assert!(!input.is_valid(), "{angle}");
        }
    }

    #[test]
    fn an_absent_orientation_and_image_are_not_serialized() {
        let device = Device {
            id: "android:Pixel_8".into(),
            name: "Pixel 8".into(),
            platform: DevicePlatform::Android,
            state: DeviceState::Booted,
            version: "14".into(),
            foldable: false,
            fold: None,
            orientation: None,
        };
        let value = serde_json::to_value(&device).unwrap();
        assert!(value.get("orientation").is_none());
        assert!(value.get("fold").is_none());
        let output = DeviceActionOutput {
            ok: true,
            device,
            image: None,
        };
        assert!(serde_json::to_value(output).unwrap().get("image").is_none());
    }

    #[test]
    fn a_fold_without_a_reading_keeps_its_two_null_fields() {
        let fold = DeviceFold {
            posture: None,
            hinge_angle: None,
        };
        assert_eq!(
            serde_json::to_value(fold).unwrap(),
            json!({ "posture": null, "hinge_angle": null })
        );
    }
}
