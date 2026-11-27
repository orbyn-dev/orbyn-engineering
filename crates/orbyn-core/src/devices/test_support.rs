//! The fake platform tools the device tests share: one Node program that logs every argv
//! and answers from files beside it, so no real adb, emulator or simctl runs. Kept apart
//! from tests.rs because the agent-tool tests install the same fake in a real Core.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use super::*;
use crate::testing::TempDir;

/// The one fake tool program the edge tests also run, so the two suites answer from
/// one file: `crates/orbynd/tests/fixtures/devices-fake-tool.js`.
const FAKE: &str = include_str!("../../../orbynd/tests/fixtures/devices-fake-tool.js");

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

    /// A service with only the iOS tools, so an Android action answers its tool_missing code
    /// while the device tool list itself is still available.
    pub(crate) fn android_less_devices(&self) -> Devices {
        self.devices_with(self.tools(false, false, true), Timeouts::default())
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
