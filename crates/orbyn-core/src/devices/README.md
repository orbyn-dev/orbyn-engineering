# Devices

Local Android AVDs and iOS simulators, the actions each supports now, and one writer
per device. The contract is [docs/devices.md](../../../../docs/devices.md); the routes
are `GET /api/devices`, `GET /api/devices/{id}` and `POST /api/devices/{id}/actions`.

| Module | Owns |
| --- | --- |
| `mod` | `Devices`: list, detail, action validation, the busy set, timeouts and fixed errors |
| `parse` | The tools' text and JSON: `adb devices`, AVDs, device-state names, `SurfaceOrientation`, simctl JSON and AVD `config.ini` |
| `tools` | The fixed argv table; no client text becomes a program, subcommand or flag |

The list is rebuilt from the tools on every read. A missing tool is a platform
status (`tool_missing`), never an error for the other platform; a failing or
timed-out list is `tool_failed` with fixed copy. Device ids are matched as a whole
against the list from the same call, and only the service's own stored tool id
reaches argv: an Android id is the AVD name, while its tool id is the booted
emulator serial (`emulator-5554`); an iOS tool id is the simulator UDID. One action
per device; a second is `device_busy`.

## Verified and not

Verified with fake tools (unit tests in `tests.rs`): every parser, the argv for
every action, platform isolation, list failure and timeout, foldable detection
true/false/failure, orientation and posture reads, read-back after an action,
every refusal, the 4 KiB/1 MiB/16 MiB bounds, process-group timeout, one writer
and the log-line builder.

Not verified: any real `adb`, emulator console or `xcrun simctl` call, on any
platform. Every command is marked `UNVERIFIED` in `tools` and listed in the
contract. The hinge preset angles (0, 130, 180) and the 1..179 bound are unverified,
and whether Android names 130 `half_opened` stays open; the service reports the
tool's posture name and never derives one from the angle.

The crate's [group table](../../README.md#groups) lists every group.
