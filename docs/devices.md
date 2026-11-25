# Devices contract

What the web Device panel needs from the service: a list of Android AVDs and iOS simulators, and typed controls for them (toolbar
actions, fold, unfold, hinge). **Built, not run against real tools**: the web side is built against a mock and the service side (list, detail and typed actions; not the live stream, tools drawer or agent access) is built (see "As built"). Eng 4 owns the service side. A name here changes only if the service
PR says so. Rows: H20-H25 and H31 in [parity](parity.md) (H22 is the tabs and float, H24 and H25 the fold and hinge controls, H31 tool versions).

Source: T3 Code at `9168ca73d` (MIT), ideas and field names only, no code copied. T3's contract is `packages/contracts/src/device.ts`
(`DeviceSummary`, `DeviceActionInput`), its fold client `apps/web/src/components/device/deviceFold.ts` and its hinge control
`packages/client-runtime/src/device/duoControl.ts`.

## Device

`GET /api/devices` answers `{devices: Device[], platforms: PlatformStatus[]}`. The list is rebuilt from the platform tools on each
call (short cache allowed, at most 2 s); the service never stores it.

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string, max 256 | Stable per platform: AVD name for Android, or `android:<serial>` when the tool cannot read the AVD name; simulator UDID for iOS. Prefixed `android:` or `ios:` so ids never collide. |
| `name` | string | Display name, such as `Pixel 8` or `iPhone 17 Pro`. |
| `platform` | `android` or `ios` | |
| `state` | `booted` or `shutdown` | T3 has `booted: boolean`; this contract names the state so a transition can be added later. These are the only two the service sends: a freshly started device reads as `shutdown` until booted, and `booting` is never sent. |
| `version` | string | OS version, empty when unknown (T3 `version`). |
| `orientation` | `portrait`, `landscape_left`, `portrait_upside_down` or `landscape_right`, or absent | The current orientation, when the device is booted and the tool reports it (T3 `setOrientation`); absent when it is not booted or cannot be read. The web shows it when present and assumes none when absent. |
| `foldable` | boolean | See detection below. |
| `fold` | object or absent | Only when `foldable` and `state` is `booted`: `{posture, hinge_angle}`. |

`fold.posture` is `closed`, `half_opened`, `opened`, `flipped` or `tent` (T3's Android set; `flipped` and `tent` are read only).
`fold.hinge_angle` is a number in degrees or `null` when the tool does not report it. Booted devices with no readable posture have
`fold: {posture: null, hinge_angle: null}`.

`orientation` is read only and is the device's current orientation in this order: `portrait`, `landscape_left`, `portrait_upside_down`,
`landscape_right`. `rotate` sends the next value after it; with no reported orientation the web sends the first next one,
`landscape_left`, and shows none rather than inventing a start.

`PlatformStatus` is `{platform, available, code?, message?}`. `available: false` carries a code from the table below (T3
`DevicePlatformAvailability`). Physical devices, SSH hosts, Expo or Simlock pools are not in the beta: the list holds local AVDs and
simulators only.

When a booted emulator's AVD name cannot be read, that call lists booted devices by `android:<serial>` only: the shutdown AVD rows
are omitted, so the same running emulator is not drawn a second time as a shutdown row.

`GET /api/devices/{id}` answers the device's own fields **flat**, plus `actions`: `{id, name, platform, state, version, foldable, fold?, orientation?, actions: ActionType[]}`. The action types are what the device supports right now; the web reads `detail.id` and `detail.actions` at the top level and shows only what `actions` lists (T3: "the drawer only shows what the platform can do").

### Foldable detection

A device is `foldable` only when the service can prove it from the tool, never from the name.

- Android, booted: `adb -s <serial> shell cmd device_state print-states` names the supported states, and `print-state` gives the
  current one. At least one state must be a fold posture (`CLOSED`, `HALF_OPENED`, `OPENED`, `FLIPPED` or `TENT`); the current
  state's name, never its angle, is the posture. A failed read means `foldable: false`.
- Android, shutdown: the AVD's `config.ini` must declare `hw.sensors.posture*`, `hw.sensors.hinge*` or `hw.device.foldable=true`.
  The display name is never used. The AVD folder is `ANDROID_AVD_HOME`, `ANDROID_USER_HOME/avd`, or `$HOME/.android/avd`.
- iOS: never foldable. T3 supports an iPhone Duo device type; this contract has no fold control on iOS.
- **Not verified against a real adb or emulator.** The state names, `print-states` output and `config.ini` keys above are the
  shapes the parsers accept, not observed output.

If the check fails or times out the device is listed with `foldable: false`; detection never fails the list.

## Actions

`POST /api/devices/{id}/actions` takes a typed body `{type, ...fields}` (JSON, max 4 KiB) and answers `{ok: true, device: Device}`
with the state read back after the change (T3 reads settings back rather than trusting the request). Unknown `type` or fields are
`invalid_action`. `{id}` in the path is the only device selector; the body has no device id.

| `type` | Fields | Platforms | T3 |
| --- | --- | --- | --- |
| `screenshot` | none; answers `{ok, device, image}` where `image` is a PNG, base64, capped at 16 MiB before encoding | both | save screenshot |
| `rotate` | `value`: `portrait`, `landscape_left`, `portrait_upside_down` or `landscape_right` | Android; iOS answers `action_unsupported` | rotate; `setOrientation` |
| `button` | `value`: `home` (both), `back` or `recents` (Android only) | see value | Home, Back, Recents |
| `power_off` | none; boots nothing, shuts the device down | both | power off |
| `fold` | none; sets posture `closed` | foldable | Fold device |
| `unfold` | none; sets posture `opened` | foldable | Unfold device |
| `hinge` | `angle`: integer 1 to 179 | foldable | hinge angle control |
| `hinge_preset` | `preset`: `closed`, `half_open` or `open` | foldable | Closed, Half open, Open |

Not in this contract: tap, swipe and typing (streaming and input belong to the preview stream, H22 and H23), the tools drawer (H26:
appearance, text size, location, permissions, push) and the agent CLI (H27). They get their own contract.

### Hinge presets

Presets are named so the web never sends raw numbers for them. **Not verified against a real adb or emulator: all three values,
and the 1 to 179 `hinge` bound below, are still to confirm.**

| Preset | Angle | Maps to |
| --- | --- | --- |
| `closed` | 0 | same as `fold` (posture `closed`). Outside the 1 to 179 `hinge` range, so only the preset or `fold` reaches it. |
| `half_open` | 130 | to confirm: 130 is the owner's preset; T3 itself takes any angle in the hinge range. |
| `open` | 180 | same as `unfold` (posture `opened`). Outside the 1 to 179 range, so only the preset or `unfold` reaches it. |

Open questions: whether Android reports `half_opened` for 130 or only by angle, and what the emulator accepts at exactly 0 and 180.
The service never derives a posture name from an angle: it reports the name the tool gave, and `half_opened` at 130 stays an open
question. `action_unsupported` is only for a platform/action combination the service refuses (iOS rotate and `button` `back`/`recents`);
a tool that rejects a value exits non-zero and the answer is `502 tool_failed`.

`fold` and `unfold` are required on every foldable. `hinge` and `hinge_preset` are listed for **every booted foldable**; there is no
further capability check. A tool that rejects a value is `502 tool_failed`, not `action_unsupported`. Failure leaves the previous
posture; the answer's `device.fold` is what the web shows.

## Person-class and 404

Reads (`GET /api/devices`, `GET /api/devices/{id}`) are local only and not Person-class. **Every write is Person-class: all
`POST /api/devices/{id}/actions` types**, including `screenshot` (it exposes the screen) and `power_off`. A service started with a
desktop shell has a person key: a request without it is `403 person_required`. A shell-less service has no person key at all, so every
write is `403 desktop_required`. Both refusals happen before the body is read and before any process starts.

The three routes are always on, like the preview and media routes; there is no feature flag. An older service without them answers
`404`: `GET /api/devices` opens no Device panel, the "+" entry after Review and comments is drawn **disabled with a short reason**, and
no error is shown. On `404 device_not_found` from a `{id}` route the web removes that device from its list and refreshes. An empty
`devices` array with `available: false` platforms shows the platform message, not a hidden feature.

## Security

| Rule | Requirement |
| --- | --- |
| Argv only | Every platform command is spawned as an argv array. No shell, no `sh -c`, no string interpolation, no `adb shell` with composed strings. |
| Command allowlist | A fixed table maps each action `type` to its exact `adb`, `emulator` or `xcrun simctl` argv. Anything not in the table is refused; a platform/action combination the service does not implement is `action_unsupported`. No client text becomes a command name, subcommand or flag. |
| Device ids | `{id}` is matched, as a whole string, against the list the service itself produced in the same call. Unlisted ids are `device_not_found`, and the client's text is never passed to a command; the service passes its own stored tool id: the booted AVD's adb serial (`emulator-5554`) for Android, or `android:<serial>` when the AVD name cannot be read; the simulator UDID for iOS. |
| Values | `angle` is an integer 1 to 179; enums are matched against the fixed sets above; the body is decoded against the typed schema before any process starts. |
| Timeouts | List: 10 s per list read. Action command: 12 s (T3's fold request uses 12 s). Screenshot: 15 s. An action reads the list once before the command and once again after it, so an HTTP action can take about 32 s (12 s plus two 10 s list reads). On a command timeout the process group is killed and the code is `tool_timeout`. |
| Output caps | Tool stdout and stderr are capped at 1 MiB each. Android screenshots stream through a 16 MiB stdout cap; over it the process is killed and the code is `tool_output_too_large`. iOS `simctl io` writes a file, so its size is checked **after simctl exits**; the 15 s screenshot timeout bounds how long simctl can write it. An oversized iOS file answers `tool_output_too_large`, but the process has already exited, so there is no kill. |
| No secrets in logs | Log the action type, device id, exit status and duration only. Never log request bodies, screenshots, paths from `$HOME`, or tool environment. Error `message` text is fixed copy, never raw tool output. |
| Tool absent | Android needs both `adb` and `emulator`; iOS needs `xcrun`. If a platform's tool is missing, that platform is `available: false` with `tool_missing`, and its actions answer `409 tool_missing` with the fixed copy below. The other platform keeps working. |
| One writer | One action per **tool id** at a time; a second is `409 device_busy`, even if the device's wire id changed between lists. |
| Scope | Local devices only. No hub proxy, no shell-exec route, no SSH, no remote hosts. (T3's own docs call a shell-exec hub route a hazard; this contract has none.) |

## Errors and codes

Body: `{code, message, ...}`. The web acts on `code` and shows `message`.

| Status | `code` | When |
| --- | --- | --- |
| 400 | `invalid_action` | Unknown `type`, bad or missing field, angle outside 1 to 179, body over 4 KiB |
| 403 | `person_required` | A write on a service with a person key, with no valid person key on the request |
| 403 | `desktop_required` | A write on a shell-less service, which has no person key at all |
| 404 | `device_not_found` | `{id}` is not in the list the service produced |
| 404 | (route absent) | Service without devices; the web draws the disabled entry, with no error |
| 409 | `tool_missing` | `adb`/`emulator` or `xcrun` not installed, or not the right toolchain (Android: platform-tools and emulator; iOS: Xcode) |
| 409 | `device_not_booted` | Action needs a booted device (every action; a shutdown device lists none) |
| 409 | `device_busy` | Another action is running on this device |
| 422 | `not_foldable` | `fold`, `unfold`, `hinge` or `hinge_preset` on a device with `foldable: false` |
| 422 | `action_unsupported` | The platform/action combination is not implemented (iOS rotate, `button` `back`/`recents`) |
| 502 | `tool_failed` | The tool exited non-zero or its result could not be read; `message` is fixed copy |
| 504 | `tool_timeout` | A timeout above elapsed |
| 413 | `tool_output_too_large` | Output cap above exceeded |

### Fixed copy

The service's `message` is exactly one of these strings; raw tool output never reaches a client. A list that fails or times out is
not an HTTP error: that platform answers `available: false` with code `tool_failed` and the last row's message.

| `code` | Message |
| --- | --- |
| `invalid_action` | "That action is not valid." |
| `person_required` | "This action needs the person at the desktop app." |
| `desktop_required` | "This action is only available in the desktop app." |
| `device_not_found` | "That device is no longer there." |
| `tool_missing` (Android) | "Android tools were not found. Install Android platform-tools and the emulator." |
| `tool_missing` (iOS) | "Xcode was not found. Install Xcode, then try again." |
| `device_not_booted` | "Start the device first." |
| `device_busy` | "Another action is running on this device." |
| `not_foldable` | "This device does not fold." |
| `action_unsupported` | "This device cannot do that." |
| `tool_failed` | "The device tool could not do that. Try again." |
| `tool_timeout` | "The device did not answer in time." |
| `tool_output_too_large` | "The device sent more than Orbyn can read." |
| list `tool_failed` | "Couldn't read the devices. Try again." |

## Commands (argv)

Every command is spawned as an argv array. The device id and tool id are the service's own, and only a validated enum or the
1-to-179 angle fills a client value. **Not verified on a real machine:** every row below.

| When | Argv |
| --- | --- |
| List AVDs | `emulator -list-avds` |
| List running Android | `adb devices` |
| Booted AVD's name | `adb -s <serial> emu avd name` |
| Android version | `adb -s <serial> shell getprop ro.build.version.release` |
| Android orientation | `adb -s <serial> shell dumpsys input` (`SurfaceOrientation`) |
| Android fold support/state | `adb -s <serial> shell cmd device_state print-states`, `adb -s <serial> shell cmd device_state print-state` |
| Hinge angle read | `adb -s <serial> emu sensor get hinge-angle0` |
| List simulators | `xcrun simctl list devices --json` |
| `screenshot` Android | `adb -s <serial> exec-out screencap -p` |
| `screenshot` iOS | `xcrun simctl io <udid> screenshot --type=png <service temp file>` |
| `rotate` Android | `adb -s <serial> shell settings put system accelerometer_rotation 0`, then `adb -s <serial> shell settings put system user_rotation <0-3>` |
| `button` Android | `adb -s <serial> shell input keyevent KEYCODE_HOME` / `KEYCODE_BACK` / `KEYCODE_APP_SWITCH` |
| `button` iOS home | `xcrun simctl ui <udid> home` |
| `power_off` Android | `adb -s <serial> emu kill` |
| `power_off` iOS | `xcrun simctl shutdown <udid>` |
| `fold` / preset `closed` | `adb -s <serial> emu fold` |
| `unfold` / preset `open` | `adb -s <serial> emu unfold` |
| `hinge` / preset `half_open` | `adb -s <serial> emu sensor set hinge-angle0 <angle>` (`half_open` uses 130) |

iOS `rotate` and `button` `back`/`recents` are `action_unsupported`; iOS is never foldable, so its fold actions are `not_foldable`.

## Not in the beta

- **3D device model.** Owner ruling: no 3D. T3's 3D view of the iPhone Duo (and its 3D assets) is excluded; the web builds 2D fold and
  hinge controls only: Fold, Unfold, a hinge slider (1 to 179) and the three presets.
- Physical devices, SSH hosts, Expo hosted sessions and Simlock pools (H28 to H30), and mobile viewing (H32).
- Stance poses for the Duo (`book`, `laptop`, `tent`, table and face-up controls): T3 has them, this contract reports `tent` and
  `flipped` as read-only postures and sets none of them.

## Planned service pieces

The frames also draw tool inspection and install, a version check, an agent-access toggle, restarting a frozen or crashed device,
the live screen and its input, and the agent that may be driving a device. This document does not define those routes, so
`web/src/features/devices/planned.ts` declares them as clearly marked typed clients. `DevicesPlannedApi` holds status, tools, check,
install, setAccess, restart and versions. Its phase-2 surface is additive at the end of the file: `DevicesLivePlannedApi` holds
`stream` (PNG frames and the agent's cursor), `pointer` (tap, swipe and scroll in normalized screen coordinates), `typeText` (text,
with `\n` for Return) and `agentState` (read, `take_over`, `hand_back`). **There is no production implementation**: every method
answers `null` and sends nothing, so the UI draws the frame's Planned or unavailable state. The dev-only mock
(`web/mock/devices.ts`, `createDevicesPlannedMock`) answers for tests and the preview. No service route exists for any of them, and
Eng 4 has not built them. `frozen` and `crashed` are likewise target states of the frames; the service contract has `booted` and
`shutdown` only.

## As built

**Web (Beta, hidden until the service reports devices; built against the mock and not run against a real service).**
`web/src/features/devices` holds the typed client interface (`contract.ts`), the real HTTP client (`api.ts`), the planned client
(`planned.ts`), the panel (`DevicesPanel.tsx`), the per-device view (`DeviceView.tsx`), tabs (`DeviceTabs.tsx`), the float
(`FloatingDevice.tsx`), the setup pages (`setup/`) and the dev-only live frames (`DevicesLivePreview.tsx`). The mock is
`web/mock/devices.ts` (`createDevicesMock`, `createDevicesPlannedMock`); the dev-only route `#/preview/devices?frame=<id>`
installs it, and `scripts/assert-no-previews.mjs` fails a build that ships the route, the mock or the Planned preview screens.

- **Launcher (device-launcher, device-launcher-disabled).** Devices sits after Review and comments with a Beta pill and opens the
  Devices tab. A phone does not draw the row. When `GET /api/devices` answers `404`, the row is drawn disabled with the reason ("Devices
  are not available on this service.") and no tab opens; any other failure leaves it out. With no available platform, the row stays
  enabled only while the Planned setup client answers; it opens the setup hub. Otherwise it is disabled with the platform's sentence.
- **Tab host (device-panel-list, device-none, device-picker, device-reading-*, device-gone, device-running,
  device-frozen, device-crashed, device-shutdown, device-narrow-panel, device-narrow-window).** The first open shows the setup hub
  when no platform is available and the Planned setup client answers, otherwise the list (a remembered last device opens that device
  instead). The list is the picker: nothing is selected until a row is
  chosen. An empty list says "No devices are listed."; an unavailable platform shows its sentence above the other platform's devices.
  While the per-device detail read runs the panel says "Reading what this device can do…" and draws no toolbar; a failed detail read
  says "Could not read what this device can do." for any code. A `404 device_not_found` from that read or a write removes the device,
  clears its tab, shows the service's sentence and reads the list again. Running, Not responding, Stopped and Shut down get the bar
  and stage the frames draw; any other state word, including `booting`, is shown as Unknown and gets no Starting stage. The frozen
  and crashed overlays call the Planned restart method (no production implementation). The live screen, Record, Type text and Device
  tools stay Planned pills until the live client answers; Record and Device tools always do.
- **Per-device tabs and float (device-tabs, device-tab-rename, device-float, device-float-docked-hint).** Each open device is a tab
  inside the Devices tab; closing a tab never shuts the device down. `+` lists the other devices from the service and opens one. A tab
  menu holds Float over chat, Rename, Device tools (Planned), Setup and tools…, Agent access… (Planned) and Shut down. Double click or
  Rename edits the person's label inline (Return saves, Escape cancels, an empty label goes back to the device name). Float opens a
  fixed window over the chat with a draggable title, a resize corner and Dock. Tab labels, the open tabs, the last device and the float
  place are per-person UI state in `orbyn.prefs.v1`; `readJson`/`writeJson` catch browser-storage failures and damaged values are cleaned
  on load.
- **Setup hub (device-setup-hub, -platform-windows, -tools-ios(-missing), -tools-android, -installing, -failed, -access, -done).**
  Platform, Tools and Agent access, then a ready summary with Open. iOS is dimmed with its reason off a Mac. Orbyn installs no
  toolchain: each tool row says Ready, Missing or Needs an update, Show steps opens the vendor's instructions with the command and its
  domain, and Check again re-reads. agent-device is optional and is the one row Orbyn installs; a failed install shows the plain line
  and the real log with Try again, and Continue stays available because it is optional. Agent access is off until turned on; the step
  always resolves off in production (Planned). The hub is also reachable from the tab menu after setup.
- **Tool versions (device-tool-versions, -checking, -offline, -steps).** A row per tool with its installed and latest tested version,
  Ready, Update available or Not checked; Show steps opens the vendor's steps with a Copy command and an official link; Check now
  re-reads. Offline keeps the installed versions and hides any update claim. With no Planned client every row and the check are Planned.
- **Toolbar and fold (H24, H25; existing).** Android, booted: Back, Home and Recents (`button`), Rotate, Screenshot, Power off. iOS,
  booted: Screenshot and Power off only; Rotate and Home are disabled and marked Planned. The service does implement iOS `button home`
  (`xcrun simctl ui <udid> home`); the web hides it. iOS devices are never foldable in the UI and show no fold or hinge control. Controls appear only when the device's `GET /api/devices/{id}` `actions` lists them. A device that is not
  `booted` shows no controls. The bar shows the orientation the service reports (Portrait, Landscape left, Portrait upside down,
  Landscape right), and Rotate sends the next one in the contract's order; with no reported orientation it sends `landscape_left`
  (the first next from portrait) and the bar shows none. Fold, Unfold (one shows at a time by the returned posture; both when it is unreported), a hinge slider 1
  to 179 with a Set angle button, and the presets Closed, Half open and Open (`hinge_preset`, never a raw number). The panel shows the
  posture and angle the service answered (`Open · 180°`), whatever the request was, and never assumes `half_opened` at 130. The
  fold frames (`device-fold-*`) stay for phase 3.
- **Live screen and input (device-input-touch, -text, -android, device-agent-driving, -tools, -paused; H22, H23, H27; Planned).**
  `DeviceView` asks `devicesLivePlannedApi()` for a stream when a device is booted. In production that client answers `null` and sends
  nothing, so the stage keeps "No screenshot yet. The live screen is Planned." while Screenshot, Rotate, the Android buttons, Power off
  and the fold controls keep working. The dev mock answers with PNG frames: the stage draws the frame, a tap/swipe/scroll sends
  normalized pointer coordinates, `Type text` opens a field under the device (Send, or Return sends `\n`), and a driving agent shows its
  cursor and a chip with Take over; touching the screen or taking over pauses it and Hand back returns control. The rail is dimmed while
  an agent drives. `initialTyping`/`initialText` are the dev preview's only entrance to the typing frame; the app never passes them. The
  preview frames are `DevicesLivePreview.tsx` behind `#/preview/devices?frame=<id>`: touch, text, Android controls, agent driving, the
  agent's four thread tools and paused. Record, the tools drawer and agent permission stay Planned.
- **One click, one write.** A ref guards every write, the controls are disabled while one runs, and a failure shows the app's own
  sentence plus the service's message, capped, with Try again. `404 device_not_found` drops the device, clears its tab, shows the
  service's sentence and reads the list again. All writes go through the Person route
  `POST /api/devices/{id}/actions` (`web/src/api/personKey.ts`).
- **Text.** Device names, tab labels, versions, tool names and messages are cut (names 60, messages 160 to 200 characters) and rendered
  as text; states, platforms, orientations and postures are looked up in `Map`s so a service-chosen key such as `__proto__` is
  Unknown (or, for orientation, not shown). Vendor links are used only when they are `http(s)` URLs. Device ids come only from the list.

- **Device tools drawer (H26; device-tools-appearance, -text, -accessibility, -location, -permissions, -push,
  -push-android, -platforms).** The Device tools Planned pill under the device opens a drawer over the panel. It has the six tabs
  the frames draw (Appearance, Text, Accessibility, Location, Permissions, Push) and an info action with the "Device tools by
  platform" table. Every control is labelled Not verified; Push on Android says "Not available on Android" and offers nothing to send,
  while the rest of the tabs keep working. Appearance, text size, location, permissions and the push payload are typed changes on the
  Planned client (`DeviceToolsPlannedApi`), and show elements is one too while the live screen runs: there is no service route, so the
  production client answers `null` and the drawer draws one Planned note with no controls. The mock (`createDevicesPlannedMock`) supplies
  the frame state for tests and the preview. Show elements stays in place but is disabled until the device's live stream runs, with
  "Outlines need a live screen." as its row line and switch tooltip and "No screen is live." in place of the element rows; when the stream
  goes away while it is on, the drawer turns it off once through the Planned client. Nothing here changes Orbyn's own appearance, and the
  payload is kept per device.
- **Agent permission (H27; device-agent-permission, -lead, -granted, -revoked, -off).** Agent access is off by default and only the
  person's switch turns it on; reading the Planned client never grants anything. One request waits with Deny, Allow once and Allow for
  this thread; the lead answer, the running-as Device chip ("Claude Code · this thread") and the revoked and off messages are drawn from
  the same typed state. The production Planned client answers `null`, so access always resolves off and the permission flow cannot
  happen until the agent-device CLI exists (H42). The frames are preview-only; the setup hub keeps its access step.

**Planned.** The service side of the live screen stream, pointer and typing (phase 2), the Devices settings frames, and the service side
of tool install, version check, agent access, restart and the H26 tools route. The web draws the live frames, the tools drawer and the
agent permission frames only against the Planned client and its dev mock; production shows their Planned state. These services are not
started; the device list, detail and actions service is built but not run against `adb`, `xcrun` or a real device.

Resolved where the doc was silent: the screenshot answer is `{ok, device, image}`; Rotate takes the value the client chose (the service does not cycle); the unfold step uses the `unfold` action, not `hinge_preset: open`; the hinge slider starts at the reported angle or 90.
For the Planned H26/H27 client there is no route at all: its state shape, the 12 text-size steps (Default is step 3), the short place list,
Ask/Allow/Deny as reset/grant/deny, the 4000-character push cap, the once/thread agent scopes and the lead answer are the preview's own
stand-ins. The accessibility element outlines need the live screen (phase 2): Show elements stays disabled with "Outlines need a live screen."
until that stream runs, the element rows read "No screen is live." instead, and a stream that goes away turns the switch off once.
Not verified: every route and field against a real service, `adb` and `simctl` behavior, the hinge preset values (0, 130, 180) and the
1 to 179 bound.

**Service (built, not run against real tools).** `crates/orbyn-core/src/devices` owns the list, detail and action calls, the fixed
argv table and the one-writer set; `crates/orbynd/src/routes/devices.rs` owns the three routes. The contracts are `Device`,
`PlatformStatus`, `DeviceList`, `DeviceDetail`, `DeviceActionType`, `DeviceActionInput` and `DeviceActionOutput` in
`crates/orbyn-contracts/src/devices.rs`.

- **Verified, with fake `adb`, `emulator` and `xcrun` programs:** every parser, the argv for every action and refusal (unknown
  type, extra field, angle 0/180/1000/-1, body over 4 KiB, unlisted ids answered `device_not_found`, injection-shaped ids
  starting no process), platform isolation when a tool is missing, a list failure or timeout, foldable detection true/false/failure, orientation and posture
  reads, state read-back after an action, one writer keyed on the tool id, the 1 MiB/16 MiB caps, iOS screenshot directory
  cleanup and symlink refusal, and a process-group kill on timeout.
- **Not verified:** any real `adb`, emulator console or `xcrun simctl` call, on any platform. The hinge presets (0/130/180), the
  1 to 179 bound, and whether Android calls 130 `half_opened` are among the open points above. The command table marks every
  argv as unverified.
- **Always on, no feature flag**, like the neighbouring preview and media routes; a client that reaches an older service still
  gets the `404` for which the web draws the disabled entry.

## Agent tools (groups 1 and 3)

Nine MCP tools let an agent ask about and drive this machine's devices. All are **effect** tools,
like the pull write tools: each call goes through the app approval gate on its own, so a person
allows or denies it, or the thread's permission mode or a rule answers; nothing is auto-approved.
The gate summary names the action and, for everything but the list, the one device it will touch,
so the confirmation says what will happen (for example "Power off device android:Pixel_8"). A Crew
worker is refused every one with the same words the pull write tools use. Each call the gate allows
writes one audit row **before** the device call it describes (architecture rule 6), named
`device.tool.<action>` (`list`, `screenshot`, `fold`, `unfold`, `rotate`, `button`, `hinge`,
`hinge_preset`, `power_off`), with the calling engine as actor, a fixed empty detail and the device
id as target: none for the list, the id when it is a short id with no path separator or control
character, and `-` otherwise, so a path can never reach the row. A call the gate denies writes the
gate's own deny row and no device row. The row never carries the answer, an image or a path.

| Tool | Arguments | Answers |
| --- | --- | --- |
| `orbyn_device_list` | none | The same list `GET /api/devices` answers, as JSON; gate summary "List the devices on this machine". |
| `orbyn_device_screenshot` | `device_id` | The same effect as `POST /api/devices/{id}/actions` with `{"type":"screenshot"}`, through the service's own `Devices::act`, so the id is matched against the list the service rebuilt and only the service's stored tool id reaches a process. The answer is the action route's own `{ok, device, image}`, the image as base64 PNG; gate summary "Take a screenshot of device <device_id>". An unknown id is the service's `device_not_found`. |
| `orbyn_device_fold` | `device_id` | The same effect as the action route's `{"type":"fold"}`: a booted foldable device folds to posture `closed`. Gate summary "Fold device <device_id>". |
| `orbyn_device_unfold` | `device_id` | `{"type":"unfold"}`: the posture becomes `opened`. Gate summary "Unfold device <device_id>". |
| `orbyn_device_rotate` | `device_id`, `value` | `{"type":"rotate","value":...}`, where `value` is `portrait`, `landscape_left`, `portrait_upside_down` or `landscape_right`; the service sends the value the caller chose and does not cycle. Gate summary "Rotate device <device_id> to <value>". |
| `orbyn_device_button` | `device_id`, `value` | `{"type":"button","value":...}`, where `value` is `home` on both platforms and `back` or `recents` on Android only. Gate summary "Press the <value> button on device <device_id>". |
| `orbyn_device_hinge` | `device_id`, `angle` | `{"type":"hinge","angle":...}`, where `angle` is a whole number from 1 to 179. Gate summary "Set the hinge angle of device <device_id> to <angle> degrees". |
| `orbyn_device_hinge_preset` | `device_id`, `preset` | `{"type":"hinge_preset","preset":...}`, where `preset` is `closed`, `half_open` or `open`; the service owns the angle (0, 130 and 180). Gate summary "Set the hinge preset of device <device_id> to <preset>". |
| `orbyn_device_power_off` | `device_id` | `{"type":"power_off"}`: the answer's device is read back as `shutdown`. Gate summary "Power off device <device_id>". |

Every action above goes through the service's own `Devices::act`, so the id is matched against the
list the service rebuilt, only the service's stored tool id reaches a process, and the answer is the
action route's own `{ok, device}` read back after the change. The `value`, `preset` and `angle`
arguments are checked against the fixed sets before the gate is asked, so an invalid action never
becomes an approval request. A wrong id, a shutdown device, a device that does not fold and an
unsupported platform action are the service's own codes and fixed copy: `device_not_found`,
`device_not_booted`, `not_foldable`, `action_unsupported`, and `tool_missing` when that platform's
tools are gone; a failing tool is `tool_failed`, `tool_timeout` or `tool_output_too_large`.

A screenshot over the tool's 192 KiB image cap is still taken and read back, but the answer leaves
the image out and says its size in `image_base64_bytes` and `note`; the reply itself is capped at
256 KiB, so a longer image would be cut mid-JSON. The audit row never carries either value.

The tools are listed only when this machine has at least one platform's tools, as the list decides
that (Android: `adb` and the emulator; iOS: `xcrun`). On a machine with none they are absent from
`tools/list` and a call is not found, like any unknown tool; the check is the service's own
`Devices::tools_available`, read once per process.

Group 2's `orbyn_device_open` and `orbyn_device_close` are not built. They belong to a thread's
device panel and its live view, and this service has no device-panel state, no live-view route and
no boot command, so the two tools are blocked on that unbuilt surface (the device panel and live
view are H20 to H23). Building them would add a new route and UI, so nothing stands in for them.
`power_off` is built because the service already had the power-off action; it adds no new service
capability, though it is new gated agent authority: it goes through the same approval gate as the other
tools.
