# Devices contract

What the web Device panel needs from the service: a list of Android AVDs and iOS simulators, and typed controls for them (toolbar
actions, fold, unfold, hinge). **Beta, hidden**: the web side is built against a mock (see "As built"); the service is not built. Eng 4 owns the service side. A name here changes only if the service
PR says so. Rows: H20-H25 and H31 in [parity](parity.md) (H22 is the tabs and float, H24 and H25 the fold and hinge controls, H31 tool versions).

Source: T3 Code at `9168ca73d` (MIT), ideas and field names only, no code copied. T3's contract is `packages/contracts/src/device.ts`
(`DeviceSummary`, `DeviceActionInput`), its fold client `apps/web/src/components/device/deviceFold.ts` and its hinge control
`packages/client-runtime/src/device/duoControl.ts`.

## Device

`GET /api/devices` answers `{devices: Device[], platforms: PlatformStatus[]}`. The list is rebuilt from the platform tools on each
call (short cache allowed, at most 2 s); the service never stores it.

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string, max 256 | Stable per platform: AVD name for Android, simulator UDID for iOS. Prefixed `android:` or `ios:` so ids never collide. |
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

`GET /api/devices/{id}` answers one `Device` plus `{actions: ActionType[]}`, the action types this device supports right now. The web
shows only what `actions` lists (T3: "the drawer only shows what the platform can do").

### Foldable detection

A device is `foldable` only when the service can prove it from the tool, never from the name.

- Android: AVD config declares a foldable or multi-display hardware profile (to confirm: `hw.device.name`, `hw.sensors.posture_list`
  or `hinge` keys in the AVD's `config.ini`, or `emulator -avd <id>` fold support). For a booted device, a read of its posture
  through the emulator console confirms it. T3 asks the emulator for `{supported, posture, hingeAngle}` and treats `supported: false`
  as not foldable.
- iOS: a simulator whose device type is a foldable model (T3 supports iPhone Duo; to confirm what `xcrun simctl list devicetypes`
  exposes for it). No simulator is foldable by default.

If the check fails or times out the device is listed with `foldable: false`; detection never fails the list.

## Actions

`POST /api/devices/{id}/actions` takes a typed body `{type, ...fields}` (JSON, max 4 KiB) and answers `{ok: true, device: Device}`
with the state read back after the change (T3 reads settings back rather than trusting the request). Unknown `type` or fields are
`invalid_action`. `{id}` in the path is the only device selector; the body has no device id.

| `type` | Fields | Platforms | T3 |
| --- | --- | --- | --- |
| `screenshot` | none; answers `{ok, image}` where `image` is a PNG, base64, max 16 MiB | both | save screenshot |
| `rotate` | `value`: `portrait`, `landscape_left`, `portrait_upside_down` or `landscape_right` | both | rotate; `setOrientation` |
| `button` | `value`: `home` (both), `back` or `recents` (Android only) | see value | Home, Back, Recents |
| `power_off` | none; boots nothing, shuts the device down | both | power off |
| `fold` | none; sets posture `closed` | foldable | Fold device |
| `unfold` | none; sets posture `opened` | foldable | Unfold device |
| `hinge` | `angle`: integer 1 to 179 | foldable | hinge angle control |
| `hinge_preset` | `preset`: `closed`, `half_open` or `open` | foldable | Closed, Half open, Open |

Not in this contract: tap, swipe and typing (streaming and input belong to the preview stream, H22 and H23), the tools drawer (H26:
appearance, text size, location, permissions, push) and the agent CLI (H27). They get their own contract.

### Hinge presets

Presets are named so the web never sends raw numbers for them. **All three values are to confirm against adb and the emulator
console before Eng 4 builds on them.**

| Preset | Angle | Maps to |
| --- | --- | --- |
| `closed` | 0 | same as `fold` (posture `closed`). Outside the 1 to 179 `hinge` range, so only the preset or `fold` reaches it. |
| `half_open` | 130 | to confirm: 130 is the owner's preset; T3 itself takes any angle in the hinge range. |
| `open` | 180 | same as `unfold` (posture `opened`). Outside the 1 to 179 range, so only the preset or `unfold` reaches it. |

Open questions: whether Android reports `half_opened` for 130 or only by angle, and what the emulator accepts at exactly 0 and 180.
Until confirmed, the service may answer `action_unsupported` for a value the tool rejects, and the web shows the message.

`fold` and `unfold` are required on every foldable. `hinge` and `hinge_preset` are offered (listed in `actions`) only when the tool
can set an angle. Failure leaves the previous posture; the answer's `device.fold` is what the web shows.

## Person-class and 404

Reads (`GET /api/devices`, `GET /api/devices/{id}`) are local only and not Person-class. **Every write is Person-class: all
`POST /api/devices/{id}/actions` types**, including `screenshot` (it exposes the screen) and `power_off`. A key-bearing client is
refused with `403 person_required`.

On `404` from `GET /api/devices` (a service without this contract) the web opens no Device panel; the "+" entry after
Review and comments is drawn disabled with a short reason, and no error is shown. On `404 device_not_found` from a `{id}` route the web removes that device from its list and refreshes. An empty `devices`
array with `available: false` platforms shows the platform message, not a hidden feature.

## Security

| Rule | Requirement |
| --- | --- |
| Argv only | Every platform command is spawned as an argv array. No shell, no `sh -c`, no string interpolation, no `adb shell` with composed strings. |
| Command allowlist | A fixed table maps each action `type` to its exact `adb`, `emulator` or `xcrun simctl` argv. Anything else is `action_unsupported`. No client text becomes a command name, subcommand or flag. |
| Device ids | `{id}` is matched, as a whole string, against the list the service itself produced in the same call (or the last 2 s cache). Unlisted ids are `device_not_found`, and the client's text is never passed to a command; the service passes its own stored platform id. |
| Values | `angle` is an integer 1 to 179; enums are matched against the fixed sets above; the body is decoded against the typed schema before any process starts. |
| Timeouts | List: 10 s. Action: 12 s (T3's fold request uses 12 s). Screenshot: 15 s. On timeout the process is killed (whole process group) and the code is `tool_timeout`. |
| Output caps | Tool stdout and stderr are capped at 1 MiB each, screenshots at 16 MiB. Over the cap: kill and `tool_output_too_large`. |
| No secrets in logs | Log the action type, device id, exit status and duration only. Never log request bodies, screenshots, paths from `$HOME`, or tool environment. Error `message` text is fixed copy, never raw tool output. |
| Tool absent | If `adb` or `xcrun` is missing, that platform is `available: false` with `tool_missing`, and its actions answer `409 tool_missing`. The other platform keeps working. |
| One writer | One action per device at a time; a second is `409 device_busy`. |
| Scope | Local devices only. No hub proxy, no shell-exec route, no SSH, no remote hosts. (T3's own docs call a shell-exec hub route a hazard; this contract has none.) |

## Errors and codes

Body: `{code, message, ...}`. The web acts on `code` and shows `message`.

| Status | `code` | When |
| --- | --- | --- |
| 400 | `invalid_action` | Unknown `type`, bad or missing field, angle outside 1 to 179, body over 4 KiB |
| 403 | `person_required` | A write without a Person session |
| 404 | `device_not_found` | `{id}` is not in the list the service produced |
| 404 | (route absent) | Service without devices; the web hides the feature |
| 409 | `tool_missing` | `adb` or `xcrun` not installed, or not the right toolchain (Android: platform-tools and emulator; iOS: Xcode) |
| 409 | `device_not_booted` | Action needs a booted device (`screenshot`, `rotate`, `button`, `fold`, `unfold`, `hinge`) |
| 409 | `device_busy` | Another action is running on this device |
| 422 | `not_foldable` | `fold`, `unfold`, `hinge` or `hinge_preset` on a device with `foldable: false` |
| 422 | `action_unsupported` | The platform or tool cannot do this action or value (T3 `unsupported` and `helper_missing`) |
| 502 | `tool_failed` | The tool exited non-zero; `message` is fixed copy |
| 504 | `tool_timeout` | A timeout above elapsed |
| 413 | `tool_output_too_large` | Output cap above exceeded |

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
  booted: Screenshot and Power off only; Rotate and Home are disabled and marked Planned. iOS devices are never foldable in the UI and
  show no fold or hinge control. Controls appear only when the device's `GET /api/devices/{id}` `actions` lists them. A device that is not
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
  service's sentence and reads the list again. All writes go through the pending Person route
  `POST /api/devices/{id}/actions` (`web/src/api/personKey.ts`).
- **Text.** Device names, tab labels, versions, tool names and messages are cut (names 60, messages 160 to 200 characters) and rendered
  as text; states, platforms, orientations and postures are looked up in `Map`s so a service-chosen key such as `__proto__` is
  Unknown (or, for orientation, not shown). Vendor links are used only when they are `http(s)` URLs. Device ids come only from the list.

- **Device tools drawer (H26; device-tools-appearance, -text, -accessibility, -location, -permissions, -push,
  -push-android, -platforms).** The Device tools Planned pill under the device opens a drawer over the panel. It has the six tabs
  the frames draw (Appearance, Text, Accessibility, Location, Permissions, Push) and an info action with the "Device tools by
  platform" table. Every control is labelled Not verified; Push on Android says "Not available on Android" and offers nothing to send,
  while the rest of the tabs keep working. Appearance, text size, show elements, location, permissions and the push payload are typed
  changes on the Planned client (`DeviceToolsPlannedApi`): there is no service route, so the production client answers `null` and the
  drawer draws one Planned note with no controls. The mock (`createDevicesPlannedMock`) supplies the frame state for tests and the
  preview. Nothing here changes Orbyn's own appearance, and the payload is kept per device.
- **Agent permission (H27; device-agent-permission, -lead, -granted, -revoked, -off).** Agent access is off by default and only the
  person's switch turns it on; reading the Planned client never grants anything. One request waits with Deny, Allow once and Allow for
  this thread; the lead answer, the running-as Device chip ("Claude Code · this thread") and the revoked and off messages are drawn from
  the same typed state. The production Planned client answers `null`, so access always resolves off and the permission flow cannot
  happen until the agent-device CLI exists (H42). The frames are preview-only; the setup hub keeps its access step.

**Planned.** The service side of the live screen stream, pointer and typing (phase 2), the Devices settings frames, and the service side
of tool install, version check, agent access, restart and the H26 tools route. The web draws the live frames, the tools drawer and the
agent permission frames only against the Planned client and its dev mock; production shows their Planned state. The service (Eng 4)
has not started; nothing here is verified against `adb`, `xcrun` or a real service.

Guessed where the doc is silent: the screenshot answer is `{ok, device, image}` (the doc says `{ok, image}`; the web reads `image` and
keeps the `device`); the unfold step uses the `unfold` action, not `hinge_preset: open`; the hinge slider starts at the reported angle or 90.
For the Planned H26/H27 client there is no route at all: its state shape, the 12 text-size steps (Default is step 3), the short place list,
Ask/Allow/Deny as reset/grant/deny, the 4000-character push cap, the once/thread agent scopes and the lead answer are the preview's own
stand-ins. The accessibility element outlines need the live screen (phase 2), so the drawer lists the tree but cannot draw the outlines yet.
Not verified: every route and field against a real service, `adb` and `simctl` behavior, the hinge preset values (0, 130, 180) and the
1 to 179 bound.
