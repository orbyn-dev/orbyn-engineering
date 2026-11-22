//! Device service tests. The platform tools are a fake Node program: every call is
//! logged, and the files beside it decide the list, the fold state, screenshots,
//! delays and failures, so no real adb, emulator or simctl runs.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use orbyn_contracts::{
    DeviceActionInput, DeviceActionType, DeviceButton, DeviceOrientation, DevicePlatform,
    DeviceState, FoldPosture, HingePreset,
};
use serde_json::Value;

use super::*;
use crate::testing::TempDir;

const FAKE: &str = r#"
const fs = require('node:fs');
const path = require('node:path');

const [tool, dir, ...args] = process.argv.slice(2);
fs.mkdirSync(dir, { recursive: true });
fs.appendFileSync(path.join(dir, 'calls.ndjson'), JSON.stringify({ tool, args }) + '\n');

const read = (name, fallback) => {
  try { return fs.readFileSync(path.join(dir, name), 'utf8').trim(); }
  catch { return fallback; }
};
const write = (name, value) => fs.writeFileSync(path.join(dir, name), String(value));
const sleep = (ms) => {
  const cell = new Int32Array(new SharedArrayBuffer(4));
  Atomics.wait(cell, 0, 0, ms);
};
const exit = (code, text) => {
  if (text) fs.writeSync(2, text + '\n');
  process.exit(code);
};
const png = (size) => {
  const magic = Buffer.from('89504e470d0a1a0a', 'hex');
  if (size <= magic.length) return magic;
  return Buffer.concat([magic, Buffer.alloc(size - magic.length, 0x41)]);
};
const writeAll = (data) => {
  const bytes = Buffer.isBuffer(data) ? data : Buffer.from(data);
  let offset = 0;
  while (offset < bytes.length) offset += fs.writeSync(1, bytes, offset);
};
const action = (args) =>
  (args.includes('emu') && (args.includes('fold') || args.includes('unfold') || args.includes('kill') || args.includes('set')))
  || (args.includes('settings') && args.includes('put'))
  || (args.includes('input') && args.includes('keyevent'));
const maybeAction = () => {
  if (!action(args)) return false;
  write('action_started', process.pid);
  if (read('action_timeout', '0') === '1') {
    write('pid', process.pid);
    sleep(30000);
    process.exit(0);
  }
  const delay = Number(read('action_delay_ms', '0'));
  if (delay > 0) sleep(delay);
  const code = Number(read('action_exit', '0'));
  if (code !== 0) exit(code, 'tool refused');
  const bytes = Number(read('action_stdout_bytes', '0'));
  if (bytes > 0) writeAll(Buffer.alloc(bytes, 0x78));
  return true;
};

if (tool === 'adb') {
  if (read('android_mode', 'flat') === 'fail') exit(2, 'adb broke');
  if (args[0] === 'devices' && read('android_mode', 'flat') === 'timeout') {
    write('pid', process.pid);
    sleep(30000);
    process.exit(0);
  }
  if (args[0] === 'devices') {
    fs.writeSync(1, 'List of devices attached\n');
    if (read('booted', '1') !== '0') fs.writeSync(1, 'emulator-5554\tdevice\n');
    process.exit(0);
  }
  if (args.includes('emu') && args.includes('avd') && args.includes('name')) {
    const name = read('android_mode', 'flat') === 'foldable' ? 'Pixel_Fold' : 'Pixel_8';
    fs.writeSync(1, name + '\nOK\n');
    process.exit(0);
  }
  if (args.includes('getprop') && args.includes('ro.build.version.release')) {
    fs.writeSync(1, '14\n');
    process.exit(0);
  }
  if (args.includes('dumpsys') && args.includes('input')) {
    fs.writeSync(1, 'Input Reader\n  SurfaceOrientation: ' + read('orientation', '0') + '\n');
    process.exit(0);
  }
  if (args.includes('cmd') && args.includes('device_state') && args.includes('print-states')) {
    if (read('foldable_read', '1') === '0') exit(2, 'no device_state');
    if (read('android_mode', 'flat') === 'foldable') {
      fs.writeSync(1, "Supported states: [\n  DeviceState{identifier=0, name='CLOSED'},\n  DeviceState{identifier=1, name='HALF_OPENED'},\n  DeviceState{identifier=2, name='OPENED'},\n]\n");
    } else {
      fs.writeSync(1, "Supported states: [\n  DeviceState{identifier=0, name='UNFOLDED'},\n]\n");
    }
    process.exit(0);
  }
  if (args.includes('cmd') && args.includes('device_state') && args.includes('print-state')) {
    if (read('android_mode', 'flat') === 'foldable') {
      const fold = read('fold', 'opened');
      fs.writeSync(1, (fold === 'closed' ? '0' : fold === 'half_opened' ? '1' : '2') + '\n');
    } else {
      fs.writeSync(1, '0\n');
    }
    process.exit(0);
  }
  if (args.includes('sensor') && args.includes('get') && args.includes('hinge-angle0')) {
    fs.writeSync(1, read('hinge', '180') + '\nOK\n');
    process.exit(0);
  }
  if (args.includes('exec-out') && args.includes('screencap')) {
    const size = Number(read('screenshot_bytes', '64'));
    writeAll(read('screenshot_png', '1') === '1' ? png(size) : Buffer.from('not a png'));
    process.exit(0);
  }
  if (args.includes('emu') && args.includes('fold')) { write('fold', 'closed'); write('hinge', '0'); if (maybeAction()) process.exit(0); }
  if (args.includes('emu') && args.includes('unfold')) { write('fold', 'opened'); write('hinge', '180'); if (maybeAction()) process.exit(0); }
  if (args.includes('emu') && args.includes('kill')) { write('booted', '0'); if (maybeAction()) process.exit(0); }
  if (args.includes('sensor') && args.includes('set') && args.includes('hinge-angle0')) {
    const angle = args[args.length - 1];
    write('hinge', angle);
    write('fold', angle === '0' ? 'closed' : angle === '180' ? 'opened' : 'half_opened');
    if (maybeAction()) process.exit(0);
  }
  if (args.includes('settings') && args.includes('user_rotation')) {
    write('orientation', args[args.length - 1]);
    if (maybeAction()) process.exit(0);
  }
  if (maybeAction()) process.exit(0);
  process.exit(0);
}

if (tool === 'emulator') {
  if (read('android_mode', 'flat') === 'fail') exit(2, 'emulator broke');
  if (args.includes('-list-avds')) {
    fs.writeSync(1, read('android_mode', 'flat') === 'foldable' ? 'Pixel_Fold\nTablet\n' : 'Pixel_8\nTablet\n');
    process.exit(0);
  }
  process.exit(0);
}

if (tool === 'xcrun') {
  if (read('ios_mode', 'normal') === 'fail') exit(2, 'simctl broke');
  if (read('ios_mode', 'normal') === 'timeout') {
    write('pid', process.pid);
    sleep(30000);
    process.exit(0);
  }
  if (args.includes('list') && args.includes('--json')) {
    const booted = read('ios_booted', '1') !== '0';
    const available = read('ios_available', '1') !== '0';
    const payload = { devices: {
      'com.apple.CoreSimulator.SimRuntime.iOS-26-1': [
        { udid: 'UDID-1', name: 'iPhone 17 Pro', state: booted ? 'Booted' : 'Shutdown', isAvailable: true },
        { udid: 'UDID-2', name: 'iPhone Fold', state: 'Shutdown', isAvailable: available },
      ],
      'com.apple.CoreSimulator.SimRuntime.watchOS-11-0': [
        { udid: 'WATCH-1', name: 'Watch', state: 'Shutdown', isAvailable: true },
      ],
    } };
    fs.writeSync(1, JSON.stringify(payload));
    process.exit(0);
  }
  if (maybeAction()) process.exit(0);
  if (args.includes('io') && args.includes('screenshot')) {
    const file = args[args.length - 1];
    const size = Number(read('screenshot_bytes', '64'));
    fs.writeFileSync(file, png(size));
    process.exit(0);
  }
  if (args.includes('ui') && args.includes('home')) process.exit(0);
  if (args.includes('shutdown')) { write('ios_booted', '0'); process.exit(0); }
  process.exit(0);
}

exit(3, 'unknown tool');
"#;

struct Fake {
    dir: TempDir,
    script: PathBuf,
}

impl Fake {
    fn new() -> Self {
        let dir = TempDir::new();
        let script = dir.path().join("fake-device-tool.cjs");
        std::fs::write(&script, FAKE).unwrap();
        Self { dir, script }
    }

    fn set(&self, name: &str, value: impl ToString) {
        std::fs::write(self.dir.path().join(name), value.to_string()).unwrap();
    }

    fn clear_calls(&self) {
        let _ = std::fs::remove_file(self.dir.path().join("calls.ndjson"));
    }

    fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.path().join("calls.ndjson"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn called(&self, tool: &str, part: &str) -> bool {
        self.calls().iter().any(|call| {
            call["tool"] == tool
                && call["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg.as_str() == Some(part))
        })
    }

    /// Wait for the fake tool to have entered the first action. The marker is
    /// written before its delay, so this is not a guess about scheduling.
    async fn wait_for_action(&self) {
        let marker = self.dir.path().join("action_started");
        tokio::time::timeout(Duration::from_secs(5), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the first action never started");
    }

    fn node() -> PathBuf {
        crate::host::process::resolve("node")
            .map(|resolved| resolved.program)
            .expect("node is installed for the device tests")
    }

    fn tool(&self, name: &str) -> Tool {
        let prefix = vec![
            self.script.as_os_str().to_owned(),
            OsString::from(name),
            self.dir.path().as_os_str().to_owned(),
        ];
        Tool {
            program: Self::node(),
            prefix,
        }
    }

    fn tools(&self, adb: bool, emulator: bool, xcrun: bool) -> Tools {
        let source = |present: bool, name: &str| {
            if present {
                ToolSource::Fixed(self.tool(name))
            } else {
                ToolSource::Named("orbyn-device-test-tool-that-does-not-exist")
            }
        };
        Tools {
            adb: source(adb, "adb"),
            emulator: source(emulator, "emulator"),
            xcrun: source(xcrun, "xcrun"),
        }
    }

    fn all_tools(&self) -> Tools {
        self.tools(true, true, true)
    }

    fn avd_home(&self) -> PathBuf {
        self.dir.path().join("avd")
    }

    fn devices(&self) -> Devices {
        Devices::for_tests(self.all_tools(), Timeouts::default(), Some(self.avd_home()))
    }

    fn devices_with(&self, tools: Tools, timeouts: Timeouts) -> Devices {
        Devices::for_tests(tools, timeouts, Some(self.avd_home()))
    }

    fn fast_timeouts() -> Timeouts {
        Timeouts {
            list: Duration::from_secs(2),
            action: Duration::from_secs(1),
            screenshot: Duration::from_secs(2),
        }
    }
}

fn names(list: &DeviceList) -> Vec<String> {
    list.devices
        .iter()
        .map(|device| device.id.clone())
        .collect()
}

fn find<'a>(list: &'a DeviceList, id: &str) -> &'a Device {
    list.devices
        .iter()
        .find(|device| device.id == id)
        .unwrap_or_else(|| panic!("{id} is in {:?}", names(list)))
}

/// The fake tool's PNG for `size` bytes: the magic, then `A`s.
fn fake_png(size: usize) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    if size > bytes.len() {
        bytes.resize(size, b'A');
    }
    bytes
}

fn call_args(call: &Value) -> Vec<&str> {
    call["args"]
        .as_array()
        .expect("the fake tool logs an argv array")
        .iter()
        .map(|arg| arg.as_str().expect("argv entries are strings"))
        .collect()
}

// Parsers.

#[test]
fn an_adb_device_list_keeps_running_emulators_only() {
    let text =
        "List of devices attached\nemulator-5554\tdevice\nemulator-5556\toffline\nR58M\tdevice\n\n";
    assert_eq!(
        parse::adb_emulator_serials(text),
        vec!["emulator-5554".to_owned()]
    );
}

#[test]
fn an_avd_list_is_one_name_per_line() {
    assert_eq!(
        parse::avd_names(" Pixel_8 \n\nTablet\r\n"),
        vec!["Pixel_8".to_owned(), "Tablet".to_owned()]
    );
}

#[test]
fn simctl_json_names_every_ios_simulator_with_its_runtime_version() {
    let text = r#"{"devices":{
        "com.apple.CoreSimulator.SimRuntime.iOS-26-1":[
            {"udid":"UDID-1","name":"iPhone 17 Pro","state":"Booted","isAvailable":true},
            {"udid":"UDID-2","name":"iPad","state":"Shutdown","isAvailable":true}
        ],
        "com.apple.CoreSimulator.SimRuntime.watchOS-11-0":[
            {"udid":"WATCH-1","name":"Watch","state":"Shutdown","isAvailable":true}
        ]}}"#;
    let devices = parse::simctl_devices(text).unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].id, "ios:UDID-1");
    assert_eq!(devices[0].state, DeviceState::Booted);
    assert_eq!(devices[0].version, "26.1");
    assert_eq!(devices[1].id, "ios:UDID-2");
    assert_eq!(devices[1].platform, DevicePlatform::Ios);
    assert!(!devices[0].foldable);
}

#[test]
fn simctl_json_skips_unavailable_simulators() {
    let text = r#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-26-1":[
        {"udid":"UDID-2","name":"iPad","state":"Shutdown","isAvailable":false}
    ]}}"#;
    assert!(parse::simctl_devices(text).unwrap().is_empty());
}

#[test]
fn a_runtime_that_is_not_ios_has_no_version() {
    assert_eq!(
        parse::runtime_version("com.apple.CoreSimulator.SimRuntime.iOS-17-0").as_deref(),
        Some("17.0")
    );
    assert_eq!(
        parse::runtime_version("com.apple.CoreSimulator.SimRuntime.watchOS-11-0"),
        None
    );
}

#[test]
fn surface_orientation_maps_the_four_rotations() {
    for (value, want) in [
        (0, DeviceOrientation::Portrait),
        (1, DeviceOrientation::LandscapeLeft),
        (2, DeviceOrientation::PortraitUpsideDown),
        (3, DeviceOrientation::LandscapeRight),
    ] {
        assert_eq!(
            parse::surface_orientation(&format!("Input\n SurfaceOrientation: {value}\n")),
            Some(want)
        );
    }
    assert_eq!(parse::surface_orientation("SurfaceOrientation: 9"), None);
}

#[test]
fn device_state_names_are_postures_and_the_angle_never_decides_one() {
    let states = parse::device_states(
        "Supported states: [\n  DeviceState{identifier=0, name='CLOSED'},\n  DeviceState{identifier=1, name='HALF_OPENED'},\n]",
    );
    assert_eq!(
        states,
        vec![(0_i64, "CLOSED".to_owned()), (1, "HALF_OPENED".to_owned())]
    );
    assert_eq!(parse::posture("closed"), Some(FoldPosture::Closed));
    assert_eq!(parse::posture("half-opened"), Some(FoldPosture::HalfOpened));
    assert_eq!(parse::posture("tent"), Some(FoldPosture::Tent));
    assert_eq!(parse::posture("UNFOLDED"), None);
    let fold = parse::fold_from_states(&states, Some(0), Some(130.0)).unwrap();
    assert_eq!(fold.posture, Some(FoldPosture::Closed));
    assert_eq!(fold.hinge_angle, Some(130.0));
}

#[test]
fn a_print_state_id_maps_to_its_supported_name() {
    assert_eq!(parse::current_state_id("1\nOK\n"), Some(1));
    assert_eq!(
        parse::current_state_id("Committed state: DeviceState{identifier=2, name='OPENED'}"),
        Some(2)
    );
    assert!(parse::supports_fold(&[
        (0, "CLOSED".into()),
        (1, "OPENED".into())
    ]));
    assert!(!parse::supports_fold(&[(0, "UNFOLDED".into())]));
}

#[test]
fn foldable_config_uses_sensor_keys_and_not_the_display_name() {
    assert!(parse::config_declares_foldable(
        "hw.sensors.posture_list=closed,half_opened\n"
    ));
    assert!(parse::config_declares_foldable("hw.sensors.hinge0=90\n"));
    assert!(parse::config_declares_foldable("hw.device.foldable=true\n"));
    assert!(!parse::config_declares_foldable(
        "hw.device.name=Pixel Fold\n"
    ));
}

#[test]
fn a_hinge_angle_is_the_first_number_the_tool_prints() {
    assert_eq!(parse::hinge_angle("130.0\nOK\n"), Some(130.0));
    assert_eq!(parse::hinge_angle("not a number"), None);
}

#[test]
fn a_device_id_must_be_prefix_plus_a_safe_name() {
    assert!(parse::valid_id("android:Pixel_8"));
    assert!(parse::valid_id("ios:UDID-1"));
    for id in [
        "Pixel_8",
        "android:",
        "android:--x",
        "android:../x",
        "android:a/b",
        "android:a b..c",
        "android:\u{1}",
    ] {
        assert!(!parse::valid_id(id), "{id}");
    }
}

// List and detail.

#[tokio::test]
async fn the_list_reports_both_platforms_with_states_versions_and_orientation() {
    let fake = Fake::new();
    fake.set("orientation", 2);
    let list = fake.devices().list().await;
    assert_eq!(
        names(&list),
        [
            "android:Pixel_8",
            "android:Tablet",
            "ios:UDID-1",
            "ios:UDID-2"
        ]
    );
    let android = find(&list, "android:Pixel_8");
    assert_eq!(android.state, DeviceState::Booted);
    assert_eq!(android.version, "14");
    assert_eq!(
        android.orientation,
        Some(DeviceOrientation::PortraitUpsideDown)
    );
    assert_eq!(find(&list, "android:Tablet").state, DeviceState::Shutdown);
    let ios = find(&list, "ios:UDID-1");
    assert_eq!(ios.platform, DevicePlatform::Ios);
    assert_eq!(ios.version, "26.1");
    assert!(!ios.foldable);
    assert_eq!(ios.orientation, None);
    let platforms: Vec<_> = list
        .platforms
        .iter()
        .map(|status| (status.platform, status.available))
        .collect();
    assert_eq!(
        platforms,
        [(DevicePlatform::Android, true), (DevicePlatform::Ios, true)]
    );
}

#[tokio::test]
async fn a_missing_android_tool_leaves_ios_working() {
    let fake = Fake::new();
    let list = fake
        .devices_with(fake.tools(false, true, true), Timeouts::default())
        .list()
        .await;
    let android = list
        .platforms
        .iter()
        .find(|status| status.platform == DevicePlatform::Android)
        .unwrap();
    assert!(!android.available);
    assert_eq!(android.code.as_deref(), Some("tool_missing"));
    assert_eq!(
        android.message.as_deref(),
        Some(tool_missing_message(DevicePlatform::Android))
    );
    assert!(
        list.devices
            .iter()
            .any(|device| device.platform == DevicePlatform::Ios)
    );
}

#[tokio::test]
async fn a_missing_xcrun_leaves_android_working() {
    let fake = Fake::new();
    let list = fake
        .devices_with(fake.tools(true, true, false), Timeouts::default())
        .list()
        .await;
    let ios = list
        .platforms
        .iter()
        .find(|status| status.platform == DevicePlatform::Ios)
        .unwrap();
    assert!(!ios.available);
    assert_eq!(ios.code.as_deref(), Some("tool_missing"));
    assert_eq!(
        ios.message.as_deref(),
        Some(tool_missing_message(DevicePlatform::Ios))
    );
    assert!(
        list.devices
            .iter()
            .any(|device| device.platform == DevicePlatform::Android)
    );
}

#[tokio::test]
async fn a_platform_tool_failure_is_available_false_with_fixed_copy() {
    let fake = Fake::new();
    fake.set("android_mode", "fail");
    let list = fake.devices().list().await;
    let android = list
        .platforms
        .iter()
        .find(|status| status.platform == DevicePlatform::Android)
        .unwrap();
    assert!(!android.available);
    assert_eq!(android.code.as_deref(), Some("tool_failed"));
    assert_eq!(android.message.as_deref(), Some(LIST_FAILED_MESSAGE));
    assert!(
        list.devices
            .iter()
            .all(|device| device.platform == DevicePlatform::Ios)
    );
}

#[tokio::test]
async fn a_list_timeout_does_not_fail_the_other_platform() {
    let fake = Fake::new();
    fake.set("android_mode", "timeout");
    let list = fake
        .devices_with(fake.all_tools(), Fake::fast_timeouts())
        .list()
        .await;
    let android = list
        .platforms
        .iter()
        .find(|status| status.platform == DevicePlatform::Android)
        .unwrap();
    assert_eq!(android.code.as_deref(), Some("tool_failed"));
    assert!(
        list.devices
            .iter()
            .any(|device| device.platform == DevicePlatform::Ios)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_list_timeout_kills_the_tool() {
    let fake = Fake::new();
    fake.set("android_mode", "timeout");
    let list = fake
        .devices_with(fake.all_tools(), Fake::fast_timeouts())
        .list()
        .await;
    assert!(
        list.platforms
            .iter()
            .any(|status| status.code.as_deref() == Some("tool_failed"))
    );
    let pid = crate::testing::sleeper_pid(&fake.dir.path().join("pid")).await;
    crate::testing::assert_process_reaped(pid, "the device list timeout").await;
}

#[tokio::test]
async fn foldable_detection_is_true_from_the_device_state_rows() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    fake.set("fold", "half_opened");
    fake.set("hinge", "130");
    let list = fake.devices().list().await;
    let device = find(&list, "android:Pixel_Fold");
    assert!(device.foldable);
    let fold = device.fold.as_ref().unwrap();
    assert_eq!(fold.posture, Some(FoldPosture::HalfOpened));
    assert_eq!(fold.hinge_angle, Some(130.0));
}

#[tokio::test]
async fn foldable_detection_is_false_when_the_state_read_fails() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    fake.set("foldable_read", 0);
    let list = fake.devices().list().await;
    assert!(!find(&list, "android:Pixel_Fold").foldable);
    assert!(find(&list, "android:Pixel_Fold").fold.is_none());
}

#[tokio::test]
async fn foldable_detection_is_false_for_a_flat_device() {
    let fake = Fake::new();
    let list = fake.devices().list().await;
    assert!(!find(&list, "android:Pixel_8").foldable);
}

#[tokio::test]
async fn a_shutdown_foldable_avd_is_detected_from_its_config() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    fake.set("booted", 0);
    let home = fake.avd_home();
    std::fs::create_dir_all(home.join("Pixel_Fold.avd")).unwrap();
    std::fs::write(
        home.join("Pixel_Fold.avd/config.ini"),
        "hw.sensors.posture_list=closed,half_opened,opened\n",
    )
    .unwrap();
    let list = fake.devices().list().await;
    let device = find(&list, "android:Pixel_Fold");
    assert_eq!(device.state, DeviceState::Shutdown);
    assert!(device.foldable);
    assert!(device.fold.is_none());
}

#[tokio::test]
async fn a_foldable_name_in_the_avd_config_alone_is_not_foldable() {
    let fake = Fake::new();
    let home = fake.avd_home();
    std::fs::create_dir_all(home.join("Tablet.avd")).unwrap();
    std::fs::write(
        home.join("Tablet.avd/config.ini"),
        "hw.device.name=Pixel Fold\n",
    )
    .unwrap();
    let list = fake.devices().list().await;
    assert!(!find(&list, "android:Tablet").foldable);
}

#[tokio::test]
async fn posture_is_what_the_tool_reports_not_what_the_angle_suggests() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    fake.set("hinge", "130");
    // The tool reports CLOSED; the angle must not turn it into half_opened.
    fake.set("fold", "closed");
    let list = fake.devices().list().await;
    let fold = find(&list, "android:Pixel_Fold").fold.as_ref().unwrap();
    assert_eq!(fold.posture, Some(FoldPosture::Closed));
}

#[tokio::test]
async fn an_ios_simulator_is_never_foldable() {
    let fake = Fake::new();
    fake.set("android_mode", "flat");
    fake.set("ios_available", 1);
    let list = fake.devices().list().await;
    assert!(!find(&list, "ios:UDID-2").foldable);
    assert!(find(&list, "ios:UDID-2").fold.is_none());
}

#[tokio::test]
async fn detail_lists_the_actions_a_booted_android_supports() {
    let fake = Fake::new();
    let detail = fake.devices().detail("android:Pixel_8").await.unwrap();
    assert_eq!(
        detail.actions,
        [
            DeviceActionType::Screenshot,
            DeviceActionType::Rotate,
            DeviceActionType::Button,
            DeviceActionType::PowerOff
        ]
    );
}

#[tokio::test]
async fn detail_adds_the_fold_actions_only_for_a_foldable() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    let detail = fake.devices().detail("android:Pixel_Fold").await.unwrap();
    assert!(detail.actions.contains(&DeviceActionType::Fold));
    assert!(detail.actions.contains(&DeviceActionType::Hinge));
    assert!(detail.actions.contains(&DeviceActionType::HingePreset));
}

#[tokio::test]
async fn detail_hides_every_action_on_a_shutdown_device() {
    let fake = Fake::new();
    fake.set("booted", 0);
    let detail = fake.devices().detail("android:Pixel_8").await.unwrap();
    assert!(detail.actions.is_empty());
}

#[tokio::test]
async fn detail_for_ios_lists_no_rotate_and_no_fold() {
    let fake = Fake::new();
    let detail = fake.devices().detail("ios:UDID-1").await.unwrap();
    assert_eq!(
        detail.actions,
        [
            DeviceActionType::Screenshot,
            DeviceActionType::Button,
            DeviceActionType::PowerOff
        ]
    );
}

#[tokio::test]
async fn detail_for_an_unknown_id_is_device_not_found() {
    let fake = Fake::new();
    let error = fake.devices().detail("android:Nope").await.unwrap_err();
    assert_eq!(error.code(), "device_not_found");
    assert_eq!(error.status(), 404);
    assert_eq!(error.public_message(), DEVICE_NOT_FOUND_MESSAGE);
}

#[tokio::test]
async fn detail_on_a_missing_tool_is_tool_missing() {
    let fake = Fake::new();
    let error = fake
        .devices_with(fake.tools(false, true, true), Timeouts::default())
        .detail("android:Pixel_8")
        .await
        .unwrap_err();
    assert_eq!(error.code(), "tool_missing");
    assert_eq!(
        error.public_message(),
        tool_missing_message(DevicePlatform::Android)
    );
}

// Actions.

#[tokio::test]
async fn screenshot_answers_base64_png_and_reads_the_device_back() {
    let fake = Fake::new();
    fake.set("orientation", 1);
    let output = fake
        .devices()
        .act("android:Pixel_8", DeviceActionInput::Screenshot {})
        .await
        .unwrap();
    assert!(output.ok);
    assert_eq!(output.device.id, "android:Pixel_8");
    assert_eq!(
        output.device.orientation,
        Some(DeviceOrientation::LandscapeLeft)
    );
    let image = output.image.expect("a screenshot answers an image");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image)
        .expect("the image is base64");
    assert_eq!(bytes, fake_png(64));
    let call = fake
        .calls()
        .into_iter()
        .find(|call| call_args(call).ends_with(&["exec-out", "screencap", "-p"]))
        .expect("the screenshot command ran");
    assert_eq!(
        call_args(&call),
        ["-s", "emulator-5554", "exec-out", "screencap", "-p"]
    );
}

#[tokio::test]
async fn rotate_sets_the_value_and_reads_the_orientation_back() {
    let fake = Fake::new();
    let output = fake
        .devices()
        .act(
            "android:Pixel_8",
            DeviceActionInput::Rotate {
                value: DeviceOrientation::LandscapeLeft,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        output.device.orientation,
        Some(DeviceOrientation::LandscapeLeft)
    );
    let calls = fake.calls();
    let rotations: Vec<_> = calls
        .iter()
        .filter(|call| call["args"].to_string().contains("user_rotation"))
        .collect();
    assert_eq!(rotations.len(), 1);
    assert!(rotations[0]["args"].to_string().contains("\"1\""));
}

#[tokio::test]
async fn button_sends_home_on_android() {
    let fake = Fake::new();
    let output = fake
        .devices()
        .act(
            "android:Pixel_8",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap();
    assert_eq!(output.device.id, "android:Pixel_8");
    assert!(fake.called("adb", "KEYCODE_HOME"));
}

#[tokio::test]
async fn button_sends_back_and_recents_on_android() {
    let fake = Fake::new();
    let devices = fake.devices();
    for (value, key) in [
        (DeviceButton::Back, "KEYCODE_BACK"),
        (DeviceButton::Recents, "KEYCODE_APP_SWITCH"),
    ] {
        fake.clear_calls();
        devices
            .act("android:Pixel_8", DeviceActionInput::Button { value })
            .await
            .unwrap();
        assert!(fake.called("adb", key), "{value:?} -> {key}");
    }
}

#[tokio::test]
async fn button_sends_home_on_ios() {
    let fake = Fake::new();
    fake.devices()
        .act(
            "ios:UDID-1",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap();
    assert!(fake.called("xcrun", "home"));
}

#[tokio::test]
async fn power_off_reads_the_device_as_shutdown() {
    let fake = Fake::new();
    let output = fake
        .devices()
        .act("android:Pixel_8", DeviceActionInput::PowerOff {})
        .await
        .unwrap();
    assert_eq!(output.device.state, DeviceState::Shutdown);
    assert!(output.image.is_none());
    assert!(fake.called("adb", "kill"));
}

#[tokio::test]
async fn power_off_on_ios_shuts_the_simulator_down() {
    let fake = Fake::new();
    let output = fake
        .devices()
        .act("ios:UDID-1", DeviceActionInput::PowerOff {})
        .await
        .unwrap();
    assert_eq!(output.device.state, DeviceState::Shutdown);
    assert!(fake.called("xcrun", "shutdown"));
}

#[tokio::test]
async fn fold_runs_the_emulator_command_and_reads_the_posture_back() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    let output = fake
        .devices()
        .act("android:Pixel_Fold", DeviceActionInput::Fold {})
        .await
        .unwrap();
    assert_eq!(
        output.device.fold.as_ref().unwrap().posture,
        Some(FoldPosture::Closed)
    );
    assert!(fake.called("adb", "fold"));
}

#[tokio::test]
async fn unfold_runs_the_emulator_command_and_reads_the_posture_back() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    let output = fake
        .devices()
        .act("android:Pixel_Fold", DeviceActionInput::Unfold {})
        .await
        .unwrap();
    assert_eq!(
        output.device.fold.as_ref().unwrap().posture,
        Some(FoldPosture::Opened)
    );
    assert!(fake.called("adb", "unfold"));
}

#[tokio::test]
async fn hinge_sets_the_angle() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    let output = fake
        .devices()
        .act("android:Pixel_Fold", DeviceActionInput::Hinge { angle: 90 })
        .await
        .unwrap();
    assert_eq!(output.device.fold.as_ref().unwrap().hinge_angle, Some(90.0));
    let set: Vec<_> = fake
        .calls()
        .into_iter()
        .filter(|call| call["args"].to_string().contains("hinge-angle0"))
        .collect();
    assert!(
        set.iter()
            .any(|call| call["args"].to_string().contains("90"))
    );
}

#[tokio::test]
async fn hinge_presets_use_their_fixed_values() {
    let fake = Fake::new();
    fake.set("android_mode", "foldable");
    let devices = fake.devices();
    for preset in [
        HingePreset::Closed,
        HingePreset::HalfOpen,
        HingePreset::Open,
    ] {
        fake.clear_calls();
        devices
            .act(
                "android:Pixel_Fold",
                DeviceActionInput::HingePreset { preset },
            )
            .await
            .unwrap();
        match preset {
            HingePreset::Closed => assert!(
                fake.calls()
                    .iter()
                    .any(|call| call_args(call).ends_with(&["emu", "fold"])),
                "closed sends fold"
            ),
            HingePreset::HalfOpen => {
                let call = fake
                    .calls()
                    .into_iter()
                    .find(|call| {
                        call_args(call).ends_with(&["emu", "sensor", "set", "hinge-angle0", "130"])
                    })
                    .expect("half_open sends the fixed 130");
                assert_eq!(
                    call_args(&call),
                    [
                        "-s",
                        "emulator-5554",
                        "emu",
                        "sensor",
                        "set",
                        "hinge-angle0",
                        "130"
                    ]
                );
            }
            HingePreset::Open => assert!(
                fake.calls()
                    .iter()
                    .any(|call| call_args(call).ends_with(&["emu", "unfold"])),
                "open sends unfold"
            ),
        }
    }
}

#[tokio::test]
async fn a_non_screenshot_action_answers_no_image() {
    let fake = Fake::new();
    let output = fake
        .devices()
        .act("android:Pixel_8", DeviceActionInput::PowerOff {})
        .await
        .unwrap();
    assert!(output.image.is_none());
}

#[tokio::test]
async fn an_unknown_type_is_refused_by_the_schema() {
    assert!(serde_json::from_str::<DeviceActionInput>(r#"{"type":"tap"}"#).is_err());
}

#[tokio::test]
async fn an_extra_field_is_refused_by_the_schema() {
    assert!(serde_json::from_str::<DeviceActionInput>(r#"{"type":"screenshot","x":1}"#).is_err());
}

#[tokio::test]
async fn hinge_angles_outside_one_to_179_are_invalid_action() {
    let fake = Fake::new();
    let devices = fake.devices();
    for angle in [0, 180, 1000, -1] {
        let error = devices
            .act("android:Pixel_Fold", DeviceActionInput::Hinge { angle })
            .await
            .unwrap_err();
        assert_eq!(error.code(), "invalid_action", "{angle}");
        assert_eq!(error.status(), 400);
        assert_eq!(error.public_message(), INVALID_ACTION_MESSAGE);
    }
    assert!(fake.calls().is_empty(), "a refused value starts no process");
}

#[tokio::test]
async fn injection_shaped_ids_are_refused_before_any_process_starts() {
    let fake = Fake::new();
    let devices = fake.devices();
    for id in [
        "a b",
        "--x",
        "../x",
        "android:../x",
        "android:--x",
        "ios:/tmp",
    ] {
        let error = devices
            .act(
                id,
                DeviceActionInput::Button {
                    value: DeviceButton::Home,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), "device_not_found", "{id}");
    }
    assert!(
        fake.calls().is_empty(),
        "no tool ran for an injection-shaped id"
    );
}

#[tokio::test]
async fn an_unlisted_id_is_device_not_found() {
    let fake = Fake::new();
    let error = fake
        .devices()
        .act(
            "android:Nope",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "device_not_found");
}

#[tokio::test]
async fn rotate_on_ios_is_action_unsupported() {
    let fake = Fake::new();
    let error = fake
        .devices()
        .act(
            "ios:UDID-1",
            DeviceActionInput::Rotate {
                value: DeviceOrientation::Portrait,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "action_unsupported");
    assert_eq!(error.status(), 422);
    assert_eq!(error.public_message(), ACTION_UNSUPPORTED_MESSAGE);
}

#[tokio::test]
async fn back_and_recents_on_ios_are_action_unsupported() {
    let fake = Fake::new();
    let devices = fake.devices();
    for value in [DeviceButton::Back, DeviceButton::Recents] {
        let error = devices
            .act("ios:UDID-1", DeviceActionInput::Button { value })
            .await
            .unwrap_err();
        assert_eq!(error.code(), "action_unsupported", "{value:?}");
    }
}

#[tokio::test]
async fn a_shutdown_device_is_not_booted() {
    let fake = Fake::new();
    fake.set("booted", 0);
    let error = fake
        .devices()
        .act(
            "android:Pixel_8",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "device_not_booted");
    assert_eq!(error.status(), 409);
    assert_eq!(error.public_message(), DEVICE_NOT_BOOTED_MESSAGE);
}

#[tokio::test]
async fn fold_actions_on_a_flat_device_are_not_foldable() {
    let fake = Fake::new();
    let devices = fake.devices();
    for action in [
        DeviceActionInput::Fold {},
        DeviceActionInput::Unfold {},
        DeviceActionInput::Hinge { angle: 90 },
        DeviceActionInput::HingePreset {
            preset: HingePreset::HalfOpen,
        },
    ] {
        let error = devices.act("android:Pixel_8", action).await.unwrap_err();
        assert_eq!(error.code(), "not_foldable");
        assert_eq!(error.status(), 422);
        assert_eq!(error.public_message(), NOT_FOLDABLE_MESSAGE);
    }
}

#[tokio::test]
async fn a_second_action_on_one_device_is_device_busy() {
    let fake = Fake::new();
    fake.set("action_delay_ms", 500);
    let devices = Arc::new(fake.devices());
    let first = {
        let devices = devices.clone();
        tokio::spawn(async move {
            devices
                .act(
                    "android:Pixel_8",
                    DeviceActionInput::Button {
                        value: DeviceButton::Home,
                    },
                )
                .await
        })
    };
    fake.wait_for_action().await;
    let error = devices
        .act(
            "android:Pixel_8",
            DeviceActionInput::Button {
                value: DeviceButton::Back,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "device_busy");
    assert_eq!(error.status(), 409);
    assert_eq!(error.public_message(), DEVICE_BUSY_MESSAGE);
    first.await.unwrap().unwrap();
    devices
        .act(
            "android:Pixel_8",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn a_timed_out_action_is_tool_timeout() {
    let fake = Fake::new();
    fake.set("action_timeout", 1);
    let error = fake
        .devices_with(
            fake.all_tools(),
            Timeouts {
                list: Duration::from_secs(10),
                action: Duration::from_secs(1),
                screenshot: Duration::from_secs(2),
            },
        )
        .act(
            "android:Pixel_8",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "tool_timeout");
    assert_eq!(error.status(), 504);
    assert_eq!(error.public_message(), TOOL_TIMEOUT_MESSAGE);
    #[cfg(unix)]
    {
        let pid = crate::testing::sleeper_pid(&fake.dir.path().join("pid")).await;
        crate::testing::assert_process_reaped(pid, "the device action timeout").await;
    }
}

#[tokio::test]
async fn an_output_over_one_mib_is_tool_output_too_large() {
    let fake = Fake::new();
    fake.set("action_stdout_bytes", STDOUT_CAP + 1);
    let error = fake
        .devices()
        .act(
            "android:Pixel_8",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "tool_output_too_large");
    assert_eq!(error.status(), 413);
    assert_eq!(error.public_message(), TOOL_OUTPUT_TOO_LARGE_MESSAGE);
}

#[tokio::test]
async fn a_screenshot_over_sixteen_mib_is_tool_output_too_large() {
    let fake = Fake::new();
    fake.set("screenshot_bytes", SCREENSHOT_CAP + 1);
    let error = fake
        .devices()
        .act("android:Pixel_8", DeviceActionInput::Screenshot {})
        .await
        .unwrap_err();
    assert_eq!(error.code(), "tool_output_too_large");
}

#[tokio::test]
async fn an_ios_screenshot_file_over_sixteen_mib_is_tool_output_too_large() {
    let fake = Fake::new();
    fake.set("screenshot_bytes", SCREENSHOT_CAP + 1);
    let error = fake
        .devices()
        .act("ios:UDID-1", DeviceActionInput::Screenshot {})
        .await
        .unwrap_err();
    assert_eq!(error.code(), "tool_output_too_large");
}

#[tokio::test]
async fn a_tool_failure_is_tool_failed_with_fixed_copy() {
    let fake = Fake::new();
    fake.set("action_exit", 2);
    let error = fake
        .devices()
        .act(
            "android:Pixel_8",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "tool_failed");
    assert_eq!(error.status(), 502);
    assert_eq!(error.public_message(), TOOL_FAILED_MESSAGE);
}

#[tokio::test]
async fn a_screenshot_that_is_not_a_png_is_tool_failed() {
    let fake = Fake::new();
    fake.set("screenshot_png", 0);
    let error = fake
        .devices()
        .act("android:Pixel_8", DeviceActionInput::Screenshot {})
        .await
        .unwrap_err();
    assert_eq!(error.code(), "tool_failed");
}

#[tokio::test]
async fn an_action_on_a_missing_tool_is_tool_missing() {
    let fake = Fake::new();
    let error = fake
        .devices_with(fake.tools(false, true, true), Timeouts::default())
        .act(
            "android:Pixel_8",
            DeviceActionInput::Button {
                value: DeviceButton::Home,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code(), "tool_missing");
    assert_eq!(
        error.public_message(),
        tool_missing_message(DevicePlatform::Android)
    );
}

#[tokio::test]
async fn ios_screenshot_writes_a_temporary_file_and_answers_png() {
    let fake = Fake::new();
    let output = fake
        .devices()
        .act("ios:UDID-1", DeviceActionInput::Screenshot {})
        .await
        .unwrap();
    let image = output.image.expect("a screenshot answers an image");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image)
        .expect("the image is base64");
    assert_eq!(bytes, fake_png(64));
    let call = fake
        .calls()
        .into_iter()
        .find(|call| {
            let args = call_args(call);
            args.starts_with(&["simctl", "io", "UDID-1", "screenshot", "--type=png"])
        })
        .expect("the screenshot command ran");
    let args = call_args(&call);
    assert_eq!(
        &args[..5],
        ["simctl", "io", "UDID-1", "screenshot", "--type=png"]
    );
    assert_eq!(args.len(), 6);
    assert!(args[5].ends_with(".png"));
}

#[test]
fn an_action_log_holds_only_the_action_device_exit_and_duration() {
    let entry = ActionLog::new(
        "screenshot",
        "android:Pixel_8",
        Some(0),
        Duration::from_millis(7),
    );
    let (action, device, exit, duration_ms) = entry.fields();
    assert_eq!(action, "screenshot");
    assert_eq!(device, "android:Pixel_8");
    assert_eq!(exit, Some(0));
    assert_eq!(duration_ms, 7);
    // The builder has no field for a request body, a screenshot, a path or tool output.
    let shown = format!("{entry:?}");
    assert!(!shown.contains("image"), "{shown}");
    assert!(!shown.contains("body"), "{shown}");
}
