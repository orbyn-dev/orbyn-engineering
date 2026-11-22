//! Local devices: the list of Android AVDs and iOS simulators, what each can do
//! now, and the one action route (docs/devices.md, H20-H25).
//!
//! Every command is an argv array from the fixed table in `tools`; a device id is
//! matched whole against the list this service just built, and only the service's
//! own stored id reaches a process. The tool's output never reaches a client
//! message: errors are fixed copy, and logs carry only the action, device id,
//! exit status and duration.

mod parse;
mod tools;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use orbyn_contracts::{
    Device, DeviceActionInput, DeviceActionOutput, DeviceActionType, DeviceButton, DeviceDetail,
    DeviceList, DevicePlatform, DeviceState, PlatformStatus,
};
use tokio::process::Command;

use crate::error::{CoreError, Result};
use crate::host::process::{CappedOutputError, ManagedChild, resolve};

const LIST_TIMEOUT: Duration = Duration::from_secs(10);
const ACTION_TIMEOUT: Duration = Duration::from_secs(12);
const SCREENSHOT_TIMEOUT: Duration = Duration::from_secs(15);
const STDOUT_CAP: usize = 1024 * 1024;
const STDERR_CAP: usize = 1024 * 1024;
const SCREENSHOT_CAP: usize = 16 * 1024 * 1024;

pub const INVALID_ACTION_MESSAGE: &str = "That action is not valid.";
pub const DEVICE_NOT_FOUND_MESSAGE: &str = "That device is no longer there.";
pub const DEVICE_NOT_BOOTED_MESSAGE: &str = "Start the device first.";
pub const DEVICE_BUSY_MESSAGE: &str = "Another action is running on this device.";
pub const NOT_FOLDABLE_MESSAGE: &str = "This device does not fold.";
pub const ACTION_UNSUPPORTED_MESSAGE: &str = "This device cannot do that.";
pub const TOOL_FAILED_MESSAGE: &str = "The device tool could not do that. Try again.";
pub const TOOL_TIMEOUT_MESSAGE: &str = "The device did not answer in time.";
pub const TOOL_OUTPUT_TOO_LARGE_MESSAGE: &str = "The device sent more than Orbyn can read.";
pub const LIST_FAILED_MESSAGE: &str = "Couldn't read the devices. Try again.";

pub fn tool_missing_message(platform: DevicePlatform) -> &'static str {
    match platform {
        DevicePlatform::Android => {
            "Android tools were not found. Install Android platform-tools and the emulator."
        }
        DevicePlatform::Ios => "Xcode was not found. Install Xcode, then try again.",
    }
}

/// The device list and controls. Cheap to keep for the process: it stores no
/// device list, only the busy writer set.
pub struct Devices {
    tools: Tools,
    timeouts: Timeouts,
    avd_home: Option<PathBuf>,
    busy: Mutex<HashSet<String>>,
}

impl Default for Devices {
    fn default() -> Self {
        Self::new()
    }
}

impl Devices {
    pub fn new() -> Self {
        Self {
            tools: Tools::default(),
            timeouts: Timeouts::default(),
            avd_home: default_avd_home(),
            busy: Mutex::new(HashSet::new()),
        }
    }

    #[cfg(test)]
    pub(super) fn for_tests(tools: Tools, timeouts: Timeouts, avd_home: Option<PathBuf>) -> Self {
        Self {
            tools,
            timeouts,
            avd_home,
            busy: Mutex::new(HashSet::new()),
        }
    }

    /// The list rebuilt from the platform tools. A missing or failing tool is a
    /// platform status, never an error for the other platform or for the route.
    pub async fn list(&self) -> DeviceList {
        let (android, ios) = tokio::join!(self.android_devices(), self.ios_devices());
        let mut devices = Vec::new();
        let mut platforms = Vec::new();
        for (platform, result) in [
            (DevicePlatform::Android, android),
            (DevicePlatform::Ios, ios),
        ] {
            match result {
                Ok(mut found) => {
                    devices.append(&mut found);
                    platforms.push(PlatformStatus {
                        platform,
                        available: true,
                        code: None,
                        message: None,
                    });
                }
                Err(PlatformFailure::Missing) => platforms.push(PlatformStatus {
                    platform,
                    available: false,
                    code: Some("tool_missing".to_owned()),
                    message: Some(tool_missing_message(platform).to_owned()),
                }),
                Err(PlatformFailure::Failed) => platforms.push(PlatformStatus {
                    platform,
                    available: false,
                    code: Some("tool_failed".to_owned()),
                    message: Some(LIST_FAILED_MESSAGE.to_owned()),
                }),
            }
        }
        DeviceList { devices, platforms }
    }

    /// One device and the actions it supports now.
    pub async fn detail(&self, id: &str) -> Result<DeviceDetail> {
        let reference = DeviceRef::parse(id).ok_or_else(device_not_found)?;
        let list = self.list().await;
        if let Some(error) = unavailable_error(&list, reference.platform) {
            return Err(error);
        }
        let device = list
            .devices
            .into_iter()
            .find(|device| device.id == reference.id)
            .ok_or_else(device_not_found)?;
        let actions = actions_for(&device);
        Ok(DeviceDetail { device, actions })
    }

    /// One action. The device id must be in the list this call builds; the only
    /// platform id that reaches argv is the one the tool just reported.
    pub async fn act(&self, id: &str, input: DeviceActionInput) -> Result<DeviceActionOutput> {
        let started = Instant::now();
        let action = input.action_type();
        let reference = DeviceRef::parse(id);
        if !input.is_valid() {
            log_action(
                action.as_str(),
                reference
                    .as_ref()
                    .map_or("invalid", |reference| reference.id.as_str()),
                None,
                started.elapsed(),
            );
            return Err(invalid_action());
        }
        let Some(reference) = reference else {
            log_action(action.as_str(), "invalid", None, started.elapsed());
            return Err(device_not_found());
        };
        let busy = match self.try_busy(&reference.id) {
            Ok(busy) => busy,
            Err(error) => {
                log_action(action.as_str(), &reference.id, None, started.elapsed());
                return Err(error);
            }
        };
        let mut exit = None;
        let result = self.act_inner(&reference, &input, &mut exit).await;
        log_action(action.as_str(), &reference.id, exit, started.elapsed());
        drop(busy);
        result
    }

    fn try_busy<'a>(&'a self, id: &str) -> Result<BusyGuard<'a>> {
        let mut busy = self
            .busy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !busy.insert(id.to_owned()) {
            return Err(device_busy());
        }
        Ok(BusyGuard {
            busy: &self.busy,
            id: id.to_owned(),
        })
    }

    async fn act_inner(
        &self,
        reference: &DeviceRef,
        input: &DeviceActionInput,
        exit: &mut Option<i32>,
    ) -> Result<DeviceActionOutput> {
        let list = self.list().await;
        if let Some(error) = unavailable_error(&list, reference.platform) {
            return Err(error);
        }
        let device = list
            .devices
            .into_iter()
            .find(|device| device.id == reference.id)
            .ok_or_else(device_not_found)?;
        if device.state != DeviceState::Booted {
            return Err(device_not_booted());
        }
        if matches!(
            (reference.platform, input),
            (
                DevicePlatform::Ios,
                DeviceActionInput::Rotate { .. }
                    | DeviceActionInput::Button {
                        value: DeviceButton::Back | DeviceButton::Recents
                    }
            )
        ) {
            return Err(action_unsupported());
        }
        let folds = matches!(
            input,
            DeviceActionInput::Fold { .. }
                | DeviceActionInput::Unfold { .. }
                | DeviceActionInput::Hinge { .. }
                | DeviceActionInput::HingePreset { .. }
        );
        if folds && !device.foldable {
            return Err(not_foldable());
        }

        let tool = match device.platform {
            DevicePlatform::Android => self.tools.adb.get(),
            DevicePlatform::Ios => self.tools.xcrun.get(),
        }
        .ok_or_else(|| tool_missing(device.platform))?;
        let platform_id = device
            .id
            .split_once(':')
            .map_or_else(|| device.id.clone(), |(_, raw)| raw.to_owned());

        if matches!(input, DeviceActionInput::Screenshot { .. }) {
            let bytes = self
                .capture_screenshot(&tool, &device, &platform_id, exit)
                .await?;
            let image = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let device = self.read_back(reference, &device, false).await?;
            return Ok(DeviceActionOutput {
                ok: true,
                device,
                image: Some(image),
            });
        }

        let commands = tools::action_commands(device.platform, input, &platform_id, None)
            .ok_or_else(action_unsupported)?;
        self.execute(
            &tool,
            device.platform,
            &commands.argv,
            self.timeouts.action,
            STDOUT_CAP,
            exit,
        )
        .await?;
        let device = self
            .read_back(
                reference,
                &device,
                matches!(input, DeviceActionInput::PowerOff { .. }),
            )
            .await?;
        Ok(DeviceActionOutput {
            ok: true,
            device,
            image: None,
        })
    }

    async fn read_back(
        &self,
        reference: &DeviceRef,
        before: &Device,
        power_off: bool,
    ) -> Result<Device> {
        let list = self.list().await;
        if let Some(error) = unavailable_error(&list, reference.platform) {
            return Err(error);
        }
        if let Some(device) = list
            .devices
            .into_iter()
            .find(|device| device.id == reference.id)
        {
            return Ok(device);
        }
        if power_off {
            let mut device = before.clone();
            device.state = DeviceState::Shutdown;
            device.fold = None;
            device.orientation = None;
            return Ok(device);
        }
        Err(device_not_found())
    }

    async fn capture_screenshot(
        &self,
        tool: &Tool,
        device: &Device,
        platform_id: &str,
        exit: &mut Option<i32>,
    ) -> Result<Vec<u8>> {
        if device.platform == DevicePlatform::Android {
            let commands = tools::action_commands(
                device.platform,
                &DeviceActionInput::Screenshot {},
                platform_id,
                None,
            )
            .ok_or_else(action_unsupported)?;
            let bytes = self
                .execute(
                    tool,
                    device.platform,
                    &commands.argv,
                    self.timeouts.screenshot,
                    SCREENSHOT_CAP,
                    exit,
                )
                .await?;
            return png(bytes);
        }

        // `simctl io` writes a file, so the service names a private one and removes
        // it after reading, capped at the same 16 MiB.
        let path = std::env::temp_dir().join(format!("orbyn-device-{}.png", crate::util::new_id()));
        if path.to_str().is_none() {
            return Err(tool_failed());
        }
        let _cleanup = TempPath(path.clone());
        let commands = tools::action_commands(
            device.platform,
            &DeviceActionInput::Screenshot {},
            platform_id,
            Some(&path),
        )
        .ok_or_else(action_unsupported)?;
        self.execute(
            tool,
            device.platform,
            &commands.argv,
            self.timeouts.screenshot,
            STDOUT_CAP,
            exit,
        )
        .await?;
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.len() > SCREENSHOT_CAP as u64 => Err(tool_output_too_large()),
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {
                let bytes = std::fs::read(&path).map_err(|_| tool_failed())?;
                png(bytes)
            }
            _ => Err(tool_failed()),
        }
    }

    async fn execute(
        &self,
        tool: &Tool,
        platform: DevicePlatform,
        commands: &[Vec<String>],
        timeout: Duration,
        stdout_cap: usize,
        exit: &mut Option<i32>,
    ) -> Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut last = Vec::new();
        for argv in commands {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(tool_timeout());
            }
            let output = self
                .run_tool(tool, argv, remaining, stdout_cap)
                .await
                .map_err(|error| action_error(error, platform))?;
            *exit = output.status.code();
            if !output.status.success() {
                return Err(tool_failed());
            }
            last = output.stdout;
        }
        Ok(last)
    }

    async fn run_tool(
        &self,
        tool: &Tool,
        argv: &[String],
        timeout: Duration,
        stdout_cap: usize,
    ) -> std::result::Result<RunOutput, RunError> {
        let mut command = Command::new(&tool.program);
        command
            .args(&tool.prefix)
            .args(argv)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::host::child_env::for_tool(command.as_std_mut());
        let child = match ManagedChild::spawn(command) {
            Ok(child) => child,
            Err(CoreError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(RunError::Missing);
            }
            Err(_) => return Err(RunError::Io),
        };
        match tokio::time::timeout(timeout, child.output_capped(stdout_cap, STDERR_CAP)).await {
            Ok(Ok(output)) => Ok(RunOutput {
                status: output.status,
                stdout: output.stdout,
            }),
            Ok(Err(CappedOutputError::TooLarge)) => Err(RunError::TooLarge),
            Ok(Err(CappedOutputError::Io(_))) => Err(RunError::Io),
            Err(_) => Err(RunError::Timeout),
        }
    }

    async fn android_devices(&self) -> std::result::Result<Vec<Device>, PlatformFailure> {
        let Some(adb) = self.tools.adb.get() else {
            return Err(PlatformFailure::Missing);
        };
        let Some(emulator) = self.tools.emulator.get() else {
            return Err(PlatformFailure::Missing);
        };
        match tokio::time::timeout(
            self.timeouts.list,
            self.android_devices_inner(&adb, &emulator),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PlatformFailure::Failed),
        }
    }

    async fn android_devices_inner(
        &self,
        adb: &Tool,
        emulator: &Tool,
    ) -> std::result::Result<Vec<Device>, PlatformFailure> {
        let attached = self.required(adb, &["devices"]).await?;
        let listed = self.required(emulator, &["-list-avds"]).await?;
        let avds = parse::avd_names(&listed);
        let mut devices: Vec<Device> = Vec::new();
        let mut positions: HashMap<String, usize> = HashMap::new();
        for name in avds {
            let id = format!("android:{name}");
            if !parse::valid_id(&id) {
                continue;
            }
            positions.insert(name.clone(), devices.len());
            devices.push(Device {
                id,
                name: name.clone(),
                platform: DevicePlatform::Android,
                state: DeviceState::Shutdown,
                version: String::new(),
                foldable: self.avd_foldable(&name),
                fold: None,
                orientation: None,
            });
        }

        for serial in parse::adb_emulator_serials(&attached) {
            let name = self
                .text(adb, &["-s", &serial, "emu", "avd", "name"])
                .await
                .and_then(|text| parse::first_line(&text))
                .filter(|name| parse::valid_id(&format!("android:{name}")))
                .unwrap_or_else(|| serial.clone());
            let id = format!("android:{name}");
            if !parse::valid_id(&id) {
                continue;
            }
            let version = self
                .text(
                    adb,
                    &[
                        "-s",
                        &serial,
                        "shell",
                        "getprop",
                        "ro.build.version.release",
                    ],
                )
                .await
                .and_then(|text| parse::first_line(&text))
                .unwrap_or_default();
            let orientation = self
                .text(adb, &["-s", &serial, "shell", "dumpsys", "input"])
                .await
                .and_then(|text| parse::surface_orientation(&text));
            let (foldable, fold) = self.android_fold(adb, &serial).await;
            let device = Device {
                id,
                name: name.clone(),
                platform: DevicePlatform::Android,
                state: DeviceState::Booted,
                version,
                foldable,
                fold,
                orientation,
            };
            if let Some(index) = positions.get(&name).copied() {
                devices[index] = device;
            } else {
                positions.insert(name, devices.len());
                devices.push(device);
            }
        }
        Ok(devices)
    }

    async fn android_fold(
        &self,
        adb: &Tool,
        serial: &str,
    ) -> (bool, Option<orbyn_contracts::DeviceFold>) {
        let Some(states) = self
            .text(
                adb,
                &["-s", serial, "shell", "cmd", "device_state", "print-states"],
            )
            .await
            .map(|text| parse::device_states(&text))
        else {
            return (false, None);
        };
        if !parse::supports_fold(&states) {
            return (false, None);
        }
        let current = self
            .text(
                adb,
                &["-s", serial, "shell", "cmd", "device_state", "print-state"],
            )
            .await
            .and_then(|text| parse::current_state_id(&text));
        let angle = self
            .text(adb, &["-s", serial, "emu", "sensor", "get", "hinge-angle0"])
            .await
            .and_then(|text| parse::hinge_angle(&text));
        (true, parse::fold_from_states(&states, current, angle))
    }

    async fn ios_devices(&self) -> std::result::Result<Vec<Device>, PlatformFailure> {
        let Some(xcrun) = self.tools.xcrun.get() else {
            return Err(PlatformFailure::Missing);
        };
        match tokio::time::timeout(self.timeouts.list, async {
            let text = self
                .required(&xcrun, &["simctl", "list", "devices", "--json"])
                .await?;
            parse::simctl_devices(&text).ok_or(PlatformFailure::Failed)
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(PlatformFailure::Failed),
        }
    }

    async fn required(
        &self,
        tool: &Tool,
        argv: &[&str],
    ) -> std::result::Result<String, PlatformFailure> {
        let argv = owned(argv);
        match self
            .run_tool(tool, &argv, self.timeouts.list, STDOUT_CAP)
            .await
        {
            Ok(output) if output.status.success() => {
                Ok(String::from_utf8_lossy(&output.stdout).into_owned())
            }
            Ok(_) | Err(RunError::Timeout | RunError::TooLarge | RunError::Io) => {
                Err(PlatformFailure::Failed)
            }
            Err(RunError::Missing) => Err(PlatformFailure::Missing),
        }
    }

    async fn text(&self, tool: &Tool, argv: &[&str]) -> Option<String> {
        let argv = owned(argv);
        match self
            .run_tool(tool, &argv, self.timeouts.list, STDOUT_CAP)
            .await
        {
            Ok(output) if output.status.success() => {
                Some(String::from_utf8_lossy(&output.stdout).into_owned())
            }
            _ => None,
        }
    }

    fn avd_foldable(&self, name: &str) -> bool {
        let Some(avd_home) = &self.avd_home else {
            return false;
        };
        let Some(path) = parse::avd_config_path(avd_home, name) else {
            return false;
        };
        std::fs::read_to_string(path)
            .map(|text| parse::config_declares_foldable(&text))
            .unwrap_or(false)
    }
}

struct BusyGuard<'a> {
    busy: &'a Mutex<HashSet<String>>,
    id: String,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        let mut busy = self
            .busy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        busy.remove(&self.id);
    }
}

struct TempPath(PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[derive(Clone, Copy, Debug)]
struct Timeouts {
    list: Duration,
    action: Duration,
    screenshot: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            list: LIST_TIMEOUT,
            action: ACTION_TIMEOUT,
            screenshot: SCREENSHOT_TIMEOUT,
        }
    }
}

#[derive(Clone, Debug)]
struct Tool {
    program: PathBuf,
    prefix: Vec<OsString>,
}

#[derive(Clone, Debug)]
enum ToolSource {
    Named(&'static str),
    #[cfg(test)]
    Fixed(Tool),
}

impl ToolSource {
    fn get(&self) -> Option<Tool> {
        match self {
            Self::Named(name) => resolve(*name).map(|resolved| Tool {
                program: resolved.program,
                prefix: resolved.prefix,
            }),
            #[cfg(test)]
            Self::Fixed(tool) => Some(tool.clone()),
        }
    }
}

#[derive(Clone, Debug)]
struct Tools {
    adb: ToolSource,
    emulator: ToolSource,
    xcrun: ToolSource,
}

impl Default for Tools {
    fn default() -> Self {
        Self {
            adb: ToolSource::Named("adb"),
            emulator: ToolSource::Named("emulator"),
            xcrun: ToolSource::Named("xcrun"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum PlatformFailure {
    Missing,
    Failed,
}

#[derive(Clone, Debug)]
struct DeviceRef {
    id: String,
    platform: DevicePlatform,
}

impl DeviceRef {
    fn parse(id: &str) -> Option<Self> {
        if !parse::valid_id(id) {
            return None;
        }
        let (prefix, _) = id.split_once(':')?;
        let platform = match prefix {
            "android" => DevicePlatform::Android,
            "ios" => DevicePlatform::Ios,
            _ => return None,
        };
        Some(Self {
            id: id.to_owned(),
            platform,
        })
    }
}

struct RunOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
enum RunError {
    Missing,
    Timeout,
    TooLarge,
    Io,
}

fn owned(argv: &[&str]) -> Vec<String> {
    argv.iter().map(|part| (*part).to_owned()).collect()
}

fn unavailable_error(list: &DeviceList, platform: DevicePlatform) -> Option<CoreError> {
    let status = list
        .platforms
        .iter()
        .find(|status| status.platform == platform)?;
    if status.available {
        return None;
    }
    if status.code.as_deref() == Some("tool_missing") {
        Some(tool_missing(platform))
    } else {
        Some(tool_failed())
    }
}

fn actions_for(device: &Device) -> Vec<DeviceActionType> {
    if device.state != DeviceState::Booted {
        return Vec::new();
    }
    match device.platform {
        DevicePlatform::Android => {
            let mut actions = vec![
                DeviceActionType::Screenshot,
                DeviceActionType::Rotate,
                DeviceActionType::Button,
                DeviceActionType::PowerOff,
            ];
            if device.foldable {
                actions.extend([
                    DeviceActionType::Fold,
                    DeviceActionType::Unfold,
                    DeviceActionType::Hinge,
                    DeviceActionType::HingePreset,
                ]);
            }
            actions
        }
        DevicePlatform::Ios => vec![
            DeviceActionType::Screenshot,
            DeviceActionType::Button,
            DeviceActionType::PowerOff,
        ],
    }
}

fn png(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok(bytes)
    } else {
        Err(tool_failed())
    }
}

fn action_error(error: RunError, platform: DevicePlatform) -> CoreError {
    match error {
        RunError::Missing => tool_missing(platform),
        RunError::Timeout => tool_timeout(),
        RunError::TooLarge => tool_output_too_large(),
        RunError::Io => tool_failed(),
    }
}

/// The whole log line for one action: no body, image, path or tool output has a
/// field here, so none can reach the log.
#[derive(Debug)]
struct ActionLog<'a> {
    action: &'a str,
    device: &'a str,
    exit: Option<i32>,
    duration_ms: u64,
}

impl<'a> ActionLog<'a> {
    fn new(action: &'a str, device: &'a str, exit: Option<i32>, duration: Duration) -> Self {
        Self {
            action,
            device,
            exit,
            duration_ms: duration.as_millis() as u64,
        }
    }

    fn fields(&self) -> (&'a str, &'a str, Option<i32>, u64) {
        (self.action, self.device, self.exit, self.duration_ms)
    }
}

fn log_action(action: &str, device: &str, exit: Option<i32>, duration: Duration) {
    let entry = ActionLog::new(action, device, exit, duration);
    let (action, device, exit, duration_ms) = entry.fields();
    tracing::info!(
        action,
        device,
        exit = ?exit,
        duration_ms,
        "device action"
    );
}

fn refusal(status: u16, code: &'static str, message: &str) -> CoreError {
    CoreError::Refused {
        status,
        code,
        message: message.to_owned(),
        detail: serde_json::Value::Null,
    }
}

fn invalid_action() -> CoreError {
    refusal(400, "invalid_action", INVALID_ACTION_MESSAGE)
}

fn device_not_found() -> CoreError {
    refusal(404, "device_not_found", DEVICE_NOT_FOUND_MESSAGE)
}

fn tool_missing(platform: DevicePlatform) -> CoreError {
    refusal(409, "tool_missing", tool_missing_message(platform))
}

fn device_not_booted() -> CoreError {
    refusal(409, "device_not_booted", DEVICE_NOT_BOOTED_MESSAGE)
}

fn device_busy() -> CoreError {
    refusal(409, "device_busy", DEVICE_BUSY_MESSAGE)
}

fn not_foldable() -> CoreError {
    refusal(422, "not_foldable", NOT_FOLDABLE_MESSAGE)
}

fn action_unsupported() -> CoreError {
    refusal(422, "action_unsupported", ACTION_UNSUPPORTED_MESSAGE)
}

fn tool_failed() -> CoreError {
    refusal(502, "tool_failed", TOOL_FAILED_MESSAGE)
}

fn tool_timeout() -> CoreError {
    refusal(504, "tool_timeout", TOOL_TIMEOUT_MESSAGE)
}

fn tool_output_too_large() -> CoreError {
    refusal(413, "tool_output_too_large", TOOL_OUTPUT_TOO_LARGE_MESSAGE)
}

fn default_avd_home() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ANDROID_AVD_HOME") {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("ANDROID_USER_HOME") {
        return Some(PathBuf::from(dir).join("avd"));
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return Some(PathBuf::from(home).join(".android").join("avd"));
    }
    std::env::var_os("ANDROID_SDK_HOME")
        .map(|home| PathBuf::from(home).join(".android").join("avd"))
}
