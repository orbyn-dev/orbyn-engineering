//! The fake platform tools the device tests share: one Node program that logs every argv
//! and answers from files beside it, so no real adb, emulator or simctl runs. Kept apart
//! from tests.rs because the agent-tool tests install the same fake in a real Core.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

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
    const delay = Number(read('list_delay_ms', '0'));
    if (delay > 0) sleep(delay);
    fs.writeSync(1, 'List of devices attached\n');
    if (read('booted', '1') !== '0') fs.writeSync(1, 'emulator-5554\tdevice\n');
    process.exit(0);
  }
  if (args.includes('emu') && args.includes('avd') && args.includes('name')) {
    if (read('avd_name_fail', '0') === '1') exit(2, 'no avd name');
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
    const delay = Number(read('screenshot_delay_ms', '0'));
    if (delay > 0) sleep(delay);
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
    const fail = Number(read('screenshot_exit', '0'));
    if (fail !== 0) exit(fail, 'screenshot failed');
    if (read('screenshot_timeout', '0') === '1') {
      write('pid', process.pid);
      sleep(30000);
      process.exit(0);
    }
    if (read('screenshot_symlink', '0') === '1') {
      fs.symlinkSync(path.join(dir, 'outside.png'), file);
      process.exit(0);
    }
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

pub(crate) struct Fake {
    pub(super) dir: TempDir,
    script: PathBuf,
}

impl Fake {
    pub(crate) fn new() -> Self {
        let dir = TempDir::new();
        let script = dir.path().join("fake-device-tool.cjs");
        std::fs::write(&script, FAKE).unwrap();
        Self { dir, script }
    }

    pub(crate) fn set(&self, name: &str, value: impl ToString) {
        std::fs::write(self.dir.path().join(name), value.to_string()).unwrap();
    }

    pub(super) fn clear_calls(&self) {
        let _ = std::fs::remove_file(self.dir.path().join("calls.ndjson"));
    }

    pub(crate) fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.path().join("calls.ndjson"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    pub(super) fn called(&self, tool: &str, part: &str) -> bool {
        self.calls().iter().any(|call| {
            call["tool"] == tool
                && call["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg.as_str() == Some(part))
        })
    }

    /// Wait until the fake tool has logged a call whose argv holds `part`. The call is
    /// logged before the fake answers, so this marks the tool reading, not a guess.
    pub(crate) async fn wait_for_call(&self, tool: &str, part: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !self.called(tool, part) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the fake {tool} never ran {part}"));
    }

    /// Wait for the fake tool to have entered the first action. The marker is
    /// written before its delay, so this is not a guess about scheduling.
    pub(super) async fn wait_for_action(&self) {
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

    pub(super) fn tools(&self, adb: bool, emulator: bool, xcrun: bool) -> Tools {
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

    pub(super) fn all_tools(&self) -> Tools {
        self.tools(true, true, true)
    }

    pub(super) fn avd_home(&self) -> PathBuf {
        self.dir.path().join("avd")
    }

    pub(crate) fn devices(&self) -> Devices {
        Devices::for_tests(self.all_tools(), Timeouts::default(), Some(self.avd_home()))
    }

    /// A service whose platform tools are all missing, so the list reports both platforms
    /// unavailable and no device tool may be listed or called.
    pub(crate) fn unavailable_devices(&self) -> Devices {
        self.devices_with(self.tools(false, false, false), Timeouts::default())
    }

    pub(super) fn devices_with(&self, tools: Tools, timeouts: Timeouts) -> Devices {
        Devices::for_tests(tools, timeouts, Some(self.avd_home()))
    }

    pub(super) fn fast_timeouts() -> Timeouts {
        Timeouts {
            list: Duration::from_secs(2),
            action: Duration::from_secs(1),
            screenshot: Duration::from_secs(2),
        }
    }
}

pub(crate) fn fake_png(size: usize) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    if size > bytes.len() {
        bytes.resize(size, b'A');
    }
    bytes
}
