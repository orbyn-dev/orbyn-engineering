# Orbyn account and sync: Phase 1 design note

Design for the first wiring of the Orbyn Engine into this app: an optional account, trusted devices, and personal
settings that follow the person across their computers. It is the design note the [Crew beta plan](crew.md#phase-1-wire-the-engine-and-engineering-first)
asks for. It changes no code. The implementation pull requests follow it, and each one updates
[architecture.md](architecture.md) in the same commit as any rule it changes.

The [threat model](account-threat-model.md) is part of this note. **Signing in stays off until Engineering 1 (the Engine
owner) has reviewed it.**

## Decisions D1 to D11

Each decision comes from the plan. Status: **Confirmed**, **Clarified** (the decision stands, the note makes it
precise) or **Corrected** (the decision as written does not hold against the code or the tools, with the reason).

| # | Status | Decision and reason |
| --- | --- | --- |
| D1 | Confirmed | The account is optional. Signed out is the normal state, and the whole app works in it. Every sync path is a background task that queues, shows its state and never blocks a thread, a run or a settings write. See [Offline-first behaviour](#offline-first-behaviour) |
| D2 | Confirmed | Staging by default, invite-only. The built-in address is the staging one; production is not compiled in until sign-up opens (Engine issue #42, then recovery). Staging is not deployed yet, so no real sign-in can succeed before the owner deploys it |
| D3 | Corrected | The device is **one data folder of `orbynd`**, not one machine. The D11 test runs two data folders on one computer, and `ORBYN_DATA_DIR` already lets a person run several, so each data folder registers as its own device with its own keychain items (see [The device and its keys](#the-device-and-its-keys)). Clarified: the password does leave the web page once, as a field in a request to `orbynd` on loopback (behind the per-launch service token); it never reaches the Engine (OPAQUE) and `orbynd` never stores it. The web UI holds no account key, token or zone key at any time |
| D4 | Corrected | "Project defaults" means the **person-level value** of a key a project may override. Per-project overrides are not synced: project ids and paths are local to a computer. The project file's own layer is never read for sync. Execution profiles, workflows and the rest stay "Next", as planned |
| D5 | Confirmed, extended | The never-sync list also holds what is machine-specific or names a program, path or endpoint, and 23 more keys are held back from v1. See [Never synced](#never-synced) and [Not in v1](#not-in-v1-may-sync-later) |
| D6 | Clarified | Two crates are called `orbyn-contracts`: the Engine's (wire for the Engine's HTTP API) and this repository's (wire between the web UI and `orbynd`). The app never names the Engine's. Engineering's record fields live in the Engine's `orbyn-engineering` crate, and the app depends on that crate directly, next to `orbyn-device-client`, both pinned to the same tag (agreed with Engineering 1). The device client re-exports only `orbyn-sync`'s merge. Nothing about a record is copied into this repository's contracts |
| D7 | Confirmed | Connected apps stay after the beta. The L52 to L55 screens stay `preview-only`, behind `__ORBYN_PREVIEWS__` ([design.md](design.md#planned-with-the-orbyn-platform)) |
| D8 | Confirmed | Crew proof signing uses a local keychain key until after the beta. That key is another asset for the threat model when it lands; it is not part of Phase 1 |
| D9 | Confirmed | The Engine is MIT or Apache-2.0, so the app may depend on it. The wiring pull request adds the dependency to [UPSTREAM.md](../UPSTREAM.md) with its notices. The Engine never depends on app code |
| D10 | Corrected | Confirmed: every trusted device holds every zone key, and no copy may suggest per-app isolation. Corrected: "reading credentials is already on the Autopilot hard-denied list" was **Planned** (slice 4f). Slice 4f now refuses a Crew worker's command that names a credential file, the data folder or a secret, behind the Crew Preview flag. Outside Crew, Orbyn still hard-denies only writes outside the checkout, and the person's file viewer refuses the data folder. Orbyn never hears about an engine's own file reads, so it cannot deny them. The threat model therefore rests on keeping secrets out of files and out of the environment, with the hard-deny code as defence in depth |
| D11 | Clarified | See [The Engine address](#the-engine-address) and [Testing](#testing). Added: the end-to-end test drives the Engine's `scripts/local.sh` (see [Testing](#testing)), and in CI a skipped test must fail |

## Dependency on the Engine

`orbyn-core` depends on the Engine's device client:

```toml
orbyn-device-client = { git = "https://github.com/orbyn-dev/orbyn-engine", tag = "engine-v0.2.4" }
```

It is the only Engine crate the app names (D6). `orbyn-device-client` re-exports every Engine type its public API
exposes (`Engine`, `Fields`, `Stamp`, `Device`, `DeviceApp` and the rest), so the app never depends on `orbyn-sync`,
`orbyn-client`, `orbyn-contracts` of the Engine or `orbyn-engineering`. A test (`account::tests::engine_scan`) fails
when a manifest or a source file names another one, or when the lock holds more than one Engine revision.

- **Pin.** Cargo allows only one of `tag` or `rev` per dependency (it rejects both). The manifest names the tag and the
  committed `Cargo.lock` records the exact revision, so the build is reproducible and a moved tag changes nothing
  until `cargo update` is run on purpose. A small script in the wiring pull request fails CI when the tag no longer
  resolves to the locked revision. CI builds with `--locked`.
- **Order.** The Engine pull request merges and is tagged first. The app pull request bumps the tag, and the two link
  to each other.
- **What it provides** (from the agreed contract; the Engine owns its definition):
  - `DeviceApi`, an object-safe `Send + Sync` trait. The app and its tests hold `Arc<dyn DeviceApi>`.
  - `DeviceClient`, the real implementation over `reqwest` (rustls with ring and webpki roots, an optional platform
    verifier, an injectable client).
  - `KeyStore`, which the app implements. It holds exactly: the device signing key, the agreement key, the device id,
    the account id, the session token and the vault key. The zone-key cache is not in it; it lives in the sealed
    snapshot (see [The device and its keys](#the-device-and-its-keys)).
  - The client does no IO, so the app stores `Engine::snapshot` itself.
  - `Clock`, so tests control time.
  - Sign-up: `check_invite`, `start_sign_up(invite, email)`, `resend_code`, `verify_email_code` (eight characters,
    normalised by the client), `register(password: SecretString)`. Proof of work is solved inside the client, and
    sixty starts an hour become `RateLimited { retry_after }`. The app's account trait folds this into the order the
    screens follow (see [The plan for the adapter](#the-plan-for-the-adapter)): the password step calls
    `start_sign_up` with the password, and the emailed code signs in.
  - Sign-in: `SignInOutcome` is `Signed(Session)`, `SecondFactor(challenge)` or `AwaitingApproval { request_id,
    digits }`. A trusted device lists `approval_requests()` and answers `approve(id)` after the digit comparison, or
    `deny(id)`.
  - Sync, per zone: push and pull, with sealing, zone keys and rotation inside the client. A conflict returns plain
    records and the app merges them with `orbyn-sync`, which the client re-exports. An upload is at most 4 MiB.
  - `devices()` returns `Device { id, name, app, current }` (`app` is `planner` or `engineering`; there is no
    `last_seen`).
  - `status()` is a watch of signed out, signing in, online, `offline { since }` and `paused { reason }`. Live notices
    come over a websocket the client keeps alive and reconnects.
  - Errors: `Offline`, `Unauthorized`, `InviteRequired`, `WrongPassword`, `RateLimited { retry_after }`, `Conflict`,
    `TooLarge`, `InvalidKey`, `Server { retryable }`, `InvalidCode`, `DeletionScheduled`, `ProtectionAdvanced` and
    `AccountGone`.
- **Engine pin.** See [Engine pin](#engine-pin). The app depends on the real client at `engine-v0.2.4` and
  `account::engine` adapts it; real sign-in stays off.

### Engine pin

The Engine release the app pins is tag `engine-v0.2.4`, commit `db006e3bdf4e5dc411af1464665f63c8679172ce` (its main CI
is green). It has the `orbyn-device-client` crate (start, confirm and deny an approval; the new device's join, digits,
finish and deny; `remove_device`; `devices`), the v3 approval message that binds the device's name, platform and `app`
label, and the re-exports that let the app depend on that one crate. `engine-v0.1.0`, `engine-v0.2.0` and `engine-v0.2.3` are
superseded. `engine-v0.2.4` adds one change over `engine-v0.2.3`: `.no_proxy()` on the device client, so
the client never uses HTTP_PROXY, HTTPS_PROXY or ALL_PROXY; a machine that reaches the engine only through a proxy cannot use the account until an explicit proxy option exists.
The explicit proxy option will be an Engine setting plus an app control, never read from the environment, and comes
after the beta and before open sign-up.

**Where the pin lives.** The `tag =` in `crates/orbyn-core/Cargo.toml` and the revision in `Cargo.lock`. The scan test
reads both, so a moved lock or a second Engine crate fails the build. CI builds with `--locked`; that the tag still
resolves to the locked revision is checked against the Engine repository when the pin is bumped, not offline. Sign-in
condition 2 below names `engine-v0.1.0`; `engine-v0.2.4` contains that tag's work, and the condition is not marked
satisfied by this note.

### The plan for the adapter

Decided with the lead session. Built as `account::engine` (see [The adapter as built](#the-adapter-as-built)); real
sign-in stays off.

- **Sync.** The Engine's per-record sync on `orbyn-engineering::Settings` is the source of truth for the merge. The app's
  whole-record `pull` and `push` with a base version are not carried over. The "offline edit lost" list is rebuilt by
  diffing the settings before and after a pass.
- **Sign-up order** follows the Engine: invite, email, password with its confirmation, the emailed code, done. Built
  (Engineer Designs v104): the invite and the email steps send nothing; the password step creates the account and emails
  the code (the device check and the retry limit sit there); the code signs in. The account trait is
  `start_sign_up(invite, email, password)` then `verify_email_code(code) -> SessionInfo`; there is no separate
  `register`. Against the real client the adapter calls `start_sign_up`, `register(password)` and `verify_email_code`
  in the order the Engine needs and keeps the password only until `register` returns. The app keeps the old rules: the
  password is at least twelve characters and nothing else is checked, five wrong emailed codes in a row refuse every code
  until a new one is sent (the Engine counts too; the core is bounded on its own), every failure of the code answers
  `invalid_code`, and the password and the code are never logged, echoed or stored.
- **New device approval** is a two-way check that needs a person at both devices. The trusted device sees Allow and
  Don't allow first; Allow (which asks for the password, D12) shows six digits, and the new device types them. Built
  (Engineer Designs v104): the new device shows no digits, there is no "They don't match" and no skip, and **finish is
  never automatic**: allowing only moves the new device to the digits step, and only the right digits sign it in. The
  adapter never calls `finish_approval` with no digits to skip that step. The digits rule is in
  `orbyn_core::account::digits` and is applied by the stand-in that plays the server: **three tries** (the first wrong
  entry says "2 tries left"), **single use**, a **constant-time compare** that looks at every digit, and **ten minutes**
  (the digits die with the request). The app counts its own tries too (an entry sent to the server is a try, taken before
  the call so two entries at once cannot share one), so it stays bounded when a server does not count. The typed digits
  are never logged, formatted or returned: they cross `AccountApi::finish_approval` as an `ApprovalDigits`, which prints
  `[redacted]`. The trusted device's digits come back once, from `approve`, to the window that confirmed the password;
  the request list never carries them. D12's confirmation and the nonce binding are unchanged.
- **D6 stands.** `orbyn-client` will re-export `DevicePlatform` and `DeviceApp` (Engineering 1 is doing it), so the app
  never names the Engine's contracts crate.
- **"Confirm it's you"** (`verify_password`) maps to the client's `reauthenticate`, a network OPAQUE login. Its copy and
  its error states cover a network failure (`acct-reauth-network`, built): the dialog says Orbyn could not be reached and
  nothing was changed, keeps the password typed, and offers Try again and Cancel.
- **`last_seen`.** The Engine's `Device` has none. Neither the device list nor `AccountDevice` has one, and no screen
  that names a device (the device rows, pair phone, the paired-machine text) shows when it was last seen.
- **Gate.** Real sign-in stays off: the gate is `Off` in release builds, the `LocalOnly` stand-in stays debug and test
  only, and nothing makes a network call under `Off`.

### The `app` label is a hint

A device signs itself in with an `app` label (`planner` or `engineering`), and from v3 the approval binds the name,
platform and label. The label is the device's **own signed claim**: it says what the device asserted at sign-in, never
which app is actually running. It is a hint for the person approving, not proof, and no decision (code or copy) may treat
it as proof of which app is asking. The approve prompt may show it as a hint and no more.

## Where the account module sits

A new capability group in `orbyn-core`, `account`, with its own README and a row in the
[group table](../crates/orbyn-core/README.md#groups) (the rule in [AGENTS.md](../AGENTS.md)). `orbynd` stays transport
only, and `web` gains no lifecycle rule.

| Module | Owns |
| --- | --- |
| `account` | `AccountCenter`: the one handle `Core` holds. Sign-up, sign-in, approvals, devices and sign-out, and the sign-in gate |
| `account::engine_url` | The Engine address and its rules |
| `account::keystore` | The `KeyStore` implementation: OS keychain items, and the sealed file for what is too big for one |
| `account::outbox` | The local store tables and the queue of changes not yet uploaded |
| `account::sync` | One worker per zone |
| `account::settings_record` | The mapping between preferences, keybindings and the `engineering.settings` record |
| `account::status` | Status values and their events |

**Local store and outbox.** The SQLite database stays the source of truth for settings, as today. A forward-only
migration adds the account's non-secret rows (profile id, Engine address, account id, device id, signed-in flag, sync
cursors) and the outbox: one row per changed field with its zone, field name, new value and the wall-clock time the
person made the edit. `set_preferences` ([preferences.rs:323](../crates/orbyn-core/src/settings/preferences.rs)) is the one write path for
preferences; it appends the outbox rows in the same transaction as the write (`change_settings`, `preferences.rs:349`),
so a crash can lose neither. The edit time goes to the client's `tick_at`, so an edit queued while the app was closed
keeps the time it was made.

**A sync worker per zone.** Each zone runs its own task with its own backoff, so one slow or failing zone never holds
the others. Phase 1 has one zone, `engineering.settings`. The worker pulls only zones whose names Engineering owns:
a device holds every zone key (D10) but fetches records only for its own zones. A worker drains the outbox in batches
within the 4 MiB upload, applies incoming changes, and goes back to sleep until a live notice, a local change or the
backoff timer wakes it.

**Merge.** The client gives conflicts back as plain records. The app merges them field by field with `orbyn-sync`:
per-field hybrid logical clock, the larger stamp wins, ties break by canonical JSON. **The apply path applies only the allowlist.** Today `check` in
`preferences.rs` accepts all 62 catalog keys, and the Engine crate drops only its never-synced keys, passing the rest
through as unknown. So a peer could send `default_project_scripts`, which would run commands on the person's other
computers, and a compromised device could use that. The rule: the sync apply path (a private path that does not queue
fields again) applies **only the 11 allowlisted keys plus keybindings**, each through `check` and the per-key rules
under [No escalation across devices](#no-escalation-across-devices). Every other incoming field is kept and passed
on unchanged, never applied locally. *Required test:* each of the 51 catalog keys that is not allowlisted, arriving from
a peer, is not applied (the local value and the outbox are unchanged), and is still kept for the next upload.

**Status over the shell event stream.** The worker publishes status on the existing `GET /api/shell/events` stream
([stream.rs:151](../crates/orbynd/src/routes/stream.rs)) as `ShellEvent::DataChanged { what }` with `what` set to `account`
(status, devices) or `account:conflicts`. A window then reads `GET /api/account` again. This adds no new event
variant. The status values follow the client's, plus the app's own: signed out, signing in, online, syncing, offline
since a time, paused with a reason (the sign-in gate, an `Unauthorized` session, `ProtectionAdvanced`,
`DeletionScheduled`, an invalid keyboard file), and the count of fields waiting.

### Offline-first behaviour

- **Queue.** Every change is saved locally first and queued. Nothing waits for the network.
- **Show state.** The status always says what is true: offline since when, how many changes are waiting, what is paused
  and why.
- **Surface conflicts.** A conflict is shown only when the same setting was changed on two devices while both were
  offline and the older edit lost. The newer change wins automatically; the person sees it afterwards with "Use the
  other value". The merge never asks first.
- **Never block.** No call from a run, a thread, a request handler or the settings write path awaits the network. A
  worker failure ends in a status, never an error to the caller. The Phase 1 test cuts the network mid-upload and
  checks that settings still read and write, the outbox keeps the change, and sync resumes with no duplicate.

How the client's errors become app behaviour:

| Client error | App behaviour |
| --- | --- |
| `Offline`, `Server { retryable: true }` | Status offline or retrying; exponential backoff with jitter; the outbox keeps everything |
| `Server { retryable: false }`, `InvalidKey` | Zone paused with a reason; other zones continue |
| `Conflict` | Merge and upload again; surface only a lost offline edit |
| `TooLarge` | The record is not uploaded; the setting stays local and the status says so |
| `Unauthorized` | Signed out with the reason "session ended". **Key material is destroyed** (device keys, token, vault key and the sealed snapshot are deleted); settings and the outbox stay. Sign in again to resume |
| `RateLimited { retry_after }` | Shown with the wait; no retry before it |
| `ProtectionAdvanced`, `DeletionScheduled` | Paused with that reason |
| `AccountGone` | Signed out locally, data kept, the reason stated |
| `InviteRequired`, `WrongPassword`, `InvalidCode` | Returned to the sign-up or sign-in step that asked |

## The device and its keys

One data folder is one device. First run creates a random, non-secret profile id in the data folder. Keychain items
are named from it, so two data folders on one computer never share keys. The device registers with the `app` label
`engineering` (Engine issue #73). Its name is chosen by the person.

Secrets live in the OS keychain through the app's `KeyStore`, never in a plain file: the Ed25519 signing key, the X25519
agreement key, the device id, the account id, the session token, and a 32-byte vault key. The client does no IO, so the
app stores `Engine::snapshot` as a file in the data folder, sealed with XChaCha20-Poly1305 under the vault key. The
snapshot holds the zone keys (the zone-key cache) and the sync cursors, so **the file is as secret as the keys**. It is
not a keychain item because it can be larger than one Windows credential (2,560 bytes at most). It is sealed with an AEAD that binds the profile id, the purpose and the format version, and the vault-key item also keeps a counter and a hash of the latest snapshot, so an older file put back by an engine is refused. Sign-out, device removal and `Unauthorized` delete the device keys, the token, the vault key and the snapshot; only settings stay, so a removed device does not keep every zone key (D10). The [threat model](account-threat-model.md) says what each of these protects against.

On a platform with no supported keychain (Linux in Phase 1), `KeyStore` reports itself unavailable and sign-in stays
off. The account never falls back to a plain file.

## The Engine address

- A built-in default: `https://api-staging.orbyn.dev`.
- A development override from `ORBYN_ENGINE_URL`, read by `orbynd` into `CoreConfig` the way `ORBYN_ACP_REGISTRY_URL`
  is today (`main.rs:136`). The local Engine stack answers at `http://127.0.0.1:8080`.
- **Plain `http` is accepted only for a parsed IP literal in `127.0.0.0/8` or `::1`**, never the name `localhost`; every
  other address must be `https`. The check runs on every request, and the client follows **no HTTP redirects**
  (`reqwest` follows them by default, so the policy is set to none). The rule is one function in `account::engine_url`, tested with `http://evil.example`,
  `http://127.0.0.1.evil.example`, a URL with user info, and IPv6 forms.
- The address is not a secret and is shown in the status. It is not synced.
- The Engine address is also a network request Orbyn makes unasked, so it is added to the
  [outbound request list](../crates/orbyn-core/src/governance/network.rs) in the wiring pull request.

## The settings record

One record, in zone `engineering.settings`. The Engine's `docs/engineering.md` and the `orbyn-engineering` crate name
the record, its keys and the merge rules and are the contract. The app calls the crate's helpers and never rebuilds a
field name or a rule itself. What follows is the app's side of the agreement.

**Merge and field rules**

- Per-field hybrid logical clock, last writer wins. Only fields that changed are staged.
- `favorite_models` and `hidden_models` have **set semantics**: one field per element, and removing an element writes a
  tombstone, so two devices adding different models both keep theirs.
- Keybindings have one field per (key, `when`) pair, with a removal marker for a rule the person turned off.
- **Field names of elements are hashed**: base32 of the SHA-256 of the element's identity, 16 characters. The real
  identity (the model id, or the key and `when`) is stored in the value. This keeps every name inside the key
  alphabet however long or odd a model id or key chord is.
- **Unknown keys pass through.** A key this version does not know is kept, merged and uploaded unchanged, never
  dropped, so an older app does not erase a newer app's settings.
- **Limits.** A field name is at most 64 characters from `a-z 0-9 _ . :`. A value is at most 8 KiB, a record 64 KiB and
  a list 500 elements. The app checks before it stages. Anything over a limit stays local and the status says how many
  settings were not synced.
- Only **person-level values** sync: the global value of a key, never a project override and never the project file's
  layer.
- `default_theme` syncs; `default_theme_set_at` does not. It stays the local "apply once" stamp
  ([EnvironmentTheme](../crates/orbyn-contracts/src/settings.rs)) and is set when a remote theme is applied.

**Synced keys** (11 of the 62 in the catalog in `preferences.rs`, plus keybindings). This is the list agreed with
Engineering 1, narrowed by the [rule below](#what-never-syncs). Only the person-level value of a key syncs.

| Group | Keys |
| --- | --- |
| Appearance and reading | `default_theme`, `response_streaming_mode`, `preview_appearance`, `preview_zoom`, `open_web_links_in_app`, `quit_confirmation` |
| Model lists | `favorite_models` and `hidden_models` (sets), `model_order` (one field) |
| Branch defaults | `branch_name_prefix`, `branch_name_instructions` |
| Keybindings | One field per (key, `when`) pair |

`branch_name_instructions` is free text that feeds a model prompt on every device; the risk is low and the choice is recorded
here. The other synced keys are harmless.

The Engine crate change that refuses `default_permission_mode`, `default_project_scripts`,
`enable_agent_browser_access` and `enable_agent_device_access` in both directions comes in a small follow-up Engine
pull request after orbyn-engine#120, before tag `engine-v0.1.0`. Until then the app does not stage them, and the apply
path above ignores them.

### Never synced

Twenty-eight keys, by decision:

| Why | Keys |
| --- | --- |
| Describes this computer | `environment_icon`, `keep_awake_during_runs`, `background_activity_profile`, `add_project_base_directory` |
| Programs, paths, endpoints | `provider_instances`, `custom_models`, `observability`, `storage_cleanup`, `worktree_cleanup` |
| Per-computer update and polling | `app_update_mode`, `automatic_git_fetch_minutes`, `provider_health_refresh_minutes` |
| Login and engine availability | `prefer_saved_login`, `disabled_engines` |
| Escalation: what agents may do or run | `default_permission_mode`, `default_project_scripts`, `enable_agent_browser_access`, `enable_agent_device_access`: a loosened permission, a command or an access grant must not cross devices |
| Cost or reach: another device could pick a costlier engine, model or effort, or widen where agents work ([rule](#what-never-syncs)) | `default_engine` (which vendor and price tier a task starts under), `default_model`, `fallback_model`, `text_generation_model`, `default_effort`, `default_model_options` (inert in v1, and every option raises cost or reach), `default_thread_env_mode` (`local` widens where agents work), `auto_thread_titles` and `branch_naming_mode` (a peer could turn model calls back on) |
| Service-owned stamp | `default_theme_set_at` |

### Not in v1, may sync later

The other 23 keys are not synced in v1, so 11 + 28 + 23 = 62:

| Key | Reason |
| --- | --- |
| `auto_settle_after_days`, `auto_settle_on_merge`, `auto_settle_keep_pull_requests` | Project lifecycle defaults; held back until project defaults sync |
| `auto_pull`, `new_worktrees_start_from_origin`, `worktree_submodules` | Change what git does on each computer; held back with project defaults |
| `remove_agent_credits_on_merge`, `pull_request_merge_method`, `review_guides_enabled`, `review_guide_model` | Project and forge defaults; held back with project defaults |
| `project_title_instructions`, `review_guide_instructions`, `source_control_writing_style` | Free-text prompts; wait for the prompts record |
| `enable_provider_update_checks`, `continue_threads_after_update`, `warm_next_turn`, `preview_profiles` | Per-computer behaviour or local browser data |
| `compact_idle_threads`, `idle_compaction`, `snooze_limited_threads`, `auto_resume_limited_threads`, `task_skip_near_limit`, `poll_pull_requests` | Thread and polling behaviour; not requested for v1 |

Also never synced in Phase 1: the secret-redaction settings (Settings › Secrets: which formats are hidden, the person's own patterns and the projects whose agents read values as they are). They sit outside the preference catalog, so the allowlist never reaches them, and turning one off widens what an agent reads; `ServerSettings` (service-wide: auto-settle, snooze presets, model prices and aliases,
title instructions), every per-project override, the Crew ledger, transcripts, diffs, environment-set values,
provider keys, join tokens and any secret (D5).

## Testing

- **Unit tests** use a fake `DeviceApi` in the app's test support. It follows the contract's error set and can lie like
  the Engine's test server: lost answers, a stale version, a failing zone, a slow answer. Covered: sign-up and
  sign-in outcomes, approvals, the outbox surviving a restart, merge of two devices' edits, set semantics with
  tombstones, limits, unknown keys, every row of the error table, and that no password or key appears in any request
  the fake receives, in a log line, or in a status or error value.
- **Local stack.** The Engine's `scripts/local.sh` (Engine pull request #118) runs the stack at
  `http://127.0.0.1:8080`: `setup`, `up`, `invite create --count 1 --label e2e` (prints the invite code once),
  `codes [since]` (log lines `code XXXX-XXXX for <email>`) and `reset` (drops only the `orbyn_local` database). Two
  more helpers are coming, `invite-code` and `latest-code <email>`; until then the test parses the printed lines.
- **Cross-repository end-to-end test**, against the Engine's local compose stack (D11), with two data folders (two
  devices): sign in on A, change a setting on A, see it on B; change offline on both, reconnect, check the merge and the
  surfaced conflict. Test devices use an in-memory `KeyStore`, never the real keychain. A separate macOS-only manual
  check covers the real keychain item ([threat model](account-threat-model.md#checks)).
- **Skips cleanly.** The test probes `ORBYN_E2E_ENGINE_URL` (default `http://127.0.0.1:8080`) and, when nothing answers,
  prints why and passes. With `ORBYN_E2E_REQUIRE_ENGINE=1` (set in CI) a missing Engine fails instead, so a broken
  job cannot hide behind a skip.
- It runs first on the owner's Mac, then as a path-filtered CI job. Whether that job becomes a required check is a
  decision for the lead; adding it removes no existing check.
- **Local stack hygiene.** The test leaves no Docker volume or image behind. Nobody runs a global Docker cleanup
  without asking Products Research first.

## No escalation across devices

A compromised device must not be able to make another one more permissive or run something there.

- **(a) Four keys are never synced** and the Engine crate refuses them in both directions: `default_permission_mode`,
  `default_project_scripts`, `enable_agent_browser_access` and `enable_agent_device_access`.
- **(b) Keybindings.** A synced rule may bind only commands from an allowlist of navigation and view commands. It can
  never bind a command that approves or denies an agent action, changes permissions, or confirms or runs something.
  It can never override the default keys of the approval dialog or of the "Confirm it's you" dialog. A rule that breaks
  this is kept and passed on, never applied.
- **(c) `default_model_options`** is never synced ([What never syncs](#what-never-syncs)), so a field that arrives from a
  peer is kept and never applied. *Test:* it is one of the keys asserted by name in the allowlist test.
- **`branch_name_prefix`** is validated as a git ref part and passed as an argument, never through a shell.

## What never syncs

**Rule (Products Research, normative).** A setting that increases cost or widens what an agent may do never syncs.
Presentation and preference settings (theme, keybindings, composer and project defaults) may. When a case is unclear, the
stricter rule applies, and the case is reported. The apply path enforces it by allowlist, so a new key or option syncs only
by a reviewed change to the lists in this note.

Dropped from the allowlist under this rule, and why:

| Option | Adapter | Why it never syncs |
| --- | --- | --- |
| `fast_mode` | Claude | Faster, costlier processing for every run |
| `service_tier` (such as `priority`) | Codex | A paid tier that raises cost on every run |
| any sandbox, approval or skip-permissions option | Claude, Codex | Widens what an agent may do |
| `mode` and every `config:*` option | ACP agents | ACP agents name their own options, and `mode` is a permission mode; none can be shown to lower cost or reach |

`default_model_options` is never synced, and there is no option filter: the allowlist is the only control, so nothing is
kept in code for it. The tests that cover it are the by-name tests in the allowlist suite: each moved and escalating key
arrives from a peer with a value that passes the value check, is kept in the record, and is not applied.

The rule also moved `default_engine`, `default_model`, `fallback_model`, `text_generation_model`, `default_effort`,
`default_thread_env_mode`, `auto_thread_titles` and `branch_naming_mode` to [Never synced](#never-synced).

## Record rules the apply path enforces

- **Apply once.** A field from the account is applied once. A value this device changed since (over its limit, a keybindings
  file edited by hand, a change that failed to queue) is never put back by an older field.
- **No stamps from the future.** A field stamped later than 24 hours past this device's clock is kept and passed on, never
  applied, and the status counts it. The local clock also never moves more than that far on a remote stamp.
- **First sync.** What a person set before the account was on is stamped older than anything the account holds, so the
  account's newer value wins and a value the account lacks is uploaded. A first sync raises no conflict.
- **Edits beat a stamp from the future.** A local edit waiting to upload wins over a remote field stamped more than 24
  hours ahead, whether the field was merged before the edit or after, and uploads. Without an edit there is nothing to
  lose, and the status counts the held-back field until a later account value replaces it.
- **Record size.** A setting that would take the record over 64 KiB stays local and the status counts it. When merging
  with another device's fields makes the record too large, the largest settings waiting to upload are left out, not the
  other devices' fields. The account keeps the value it already holds for such a setting; it is kept to pass on and never applied here, until a
  later local edit fits. Only a record that is too large without any of this device's changes pauses the zone.
- **Removal markers** of set elements (`favorite_models`, `hidden_models`) older than **90 days** are dropped when the record
  is uploaded. A device that was offline for longer than that and still lists a removed element can bring it back. A marker another
  device compacted away is dropped on the next merge, not uploaded again. The upload carries the whole merged record
  (the Engine contract), so apart from these markers and the settings that did not fit, no other device's value is
  left out.
- **Lists** a peer adds to stop at 500 elements and must pass the same value check as a local list.
- **Keybinding fields** are named from the key as compared (trimmed, lowercase), so `Mod+Y` and `mod+y` are one field.

## Sign-in gate

`CoreConfig` gets an account gate with three values: `Off` (the default: every sign-in and sign-up route answers
`account_disabled` and no request goes to any Engine), `LocalOnly` (only a loopback Engine address, for D11) and `On`.
The wiring pull request ships `Off` and `LocalOnly`. `On` is a separate small change that links the Engine owner's
written review of the threat model.

**Real sign-in stays `Off` until all eight hold.** `LocalOnly` with throwaway accounts on the loopback stack is allowed
now and does not wait for them.

1. orbyn-engine#120 is merged: a recent sign-in is required to remove a device, sign out everywhere, turn two-factor
   off or change protection.
2. The Engine crate follow-up that refuses the four keys in both directions is merged and included in tag
   `engine-v0.1.0`, and the app pins that tag. Until then the crate still lists `default_permission_mode` as a known
   setting, which is safe only because the app never applies it.
3. "Confirm it's you" is built and enforced inside `orbynd`, with a route-table guard so no new account-changing route
   can skip it.
4. Controls 1 to 7 and 5b of the [threat model](account-threat-model.md) are built and their checks pass:
   - the allowlist-only apply test for all 51 non-allowlisted keys;
   - the keybinding test;
   - the per-adapter options test;
   - the child-environment scrub tests;
   - the snapshot tests: older, other-profile and wrong-version files are refused;
   - the key-wipe check on sign-out, device removal and `Unauthorized`. The sign-out and removal dialogs already say
     "This device's keys, removed from the Keychain", so that copy and the wipe must land together: the gate does not
     turn on until the wipe exists;
   - the canary tests for logs and diagnostics.
5. macOS: the keychain API and the Developer ID provisioning are settled and verified on a signed build, and the
   release check shows hardened runtime without `get-task-allow` or `disable-library-validation`.
6. Windows: the Account page says any program you run can act as this device, and the Designer's frames for that note
   and for "Confirm it's you" are merged.
7. Every engine on the receiving machine runs under that machine's own permission mode, whatever engine the synced
   `default_engine` picks.
8. The Engine reviewer approves these documents once more, this time against the built code.

**Distributed builds are unsigned for now, so the gate stays `Off`.** There is no Apple Developer account, so no build we
distribute is signed with a Developer ID or notarized. Item 5 is therefore not met and stays a hard condition: real
sign-in stays `Off` in every build we distribute, `Settings > Account` stays hidden as this design says, and the beta
ships with the gate `Off` for downloads. We do not weaken item 5 and we do not swap the Keychain for a file store to get
around it. Development builds against the local loopback stack keep `LocalOnly`. What turns real sign-in on: a Developer
ID, notarization of the release build, and the release check showing hardened runtime without `get-task-allow` or
`disable-library-validation`, verified on that signed build, together with the other seven conditions. Items 4, 6, 7, 9
and 10 and the loopback two-machine test need no signing and proceed now.

Follow-ups before the gate can be turned on (tracked, not built in the wiring pull requests):

9. When the real client lands: a test that only a loopback address is accepted under `LocalOnly`, and a test that
   the client follows no redirect.
10. A test that request-body logging stays off the account routes, so no password reaches a log.
11. Done in Crew slice 4f: the list in `orbyn_core::account::changes` marks the state-changing routes, Autopilot's
    always-denied rule (N1) refuses a command or request that names one, and `orbynd`'s route-table test fails when a
    route under `/api/account` is in neither list. Governance sees a command or request, not the handler, so this
    covers what reaches the gate. Every state-changing account route must be refusable by that policy, so the list includes the new ones:
    `approvals/:id/approve`, `approvals/:id/deny`, `sign-in/approval`, `connect`, `connect/use-device`,
    `connect/use-account`, `connect/later` and `connect/seen`. A later "Refused on this computer" activity item for
    them is out of scope.
12. Approval on the trusted side, at `engine-v0.2.4`. The real client has no `approve(id)`, `deny(id)` or request list, so
    the app maps its screens onto the Engine's own calls (below). What the Engine guarantees and the app relies on: an
    approval that expired (ten minutes), was answered, was cancelled by either device or was used is gone, and
    `approval_state`, `confirm_approval` and `deny_approval` answer `ApprovalExpired` (a 410), which the adapter shows as
    "this request ended"; and the new device's `finish_approval` accepts only an approver that signed what it accepts
    (an Engineering-labelled device takes only the v3 message), which makes an out-of-date approver fail as a refusal
    ([approval mapping](#approval-mapping)). `deny_approval` carries no reason, so "Codes don't match" cannot reach the
    new device.
13. **Done.** The "Confirm it's you" confirmation is bound to the caller (issue #435). `/confirm` answers with a
    one-time nonce (16 random bytes, in memory only), and the action request sends it back in the `x-orbyn-confirm`
    header: only the window that typed the password has it, so a caller holding only the service token cannot use a
    confirmation it did not obtain. A nonce is for one action and one device, request or choice, is used up by its
    first use, is replaced by a later confirmation, and is cleared by sign-out and a restart. `POST
    /api/account/confirm/cancel` drops a confirmation, and only its own nonce can, so Cancel and Escape in the dialog
    leave no live confirmation behind. It covers every guarded route: remove device, sign out everywhere, letting a
    device in, and the two first-sync choices that overwrite. The desktop page calls the loopback service
    across origins, so the header is allowed in the service's CORS preflight (`x-orbyn-confirm`); allowing it grants
    nothing alone. A dialog the page takes away also gives its confirmation up.
14. Key "this computer already joined this account" by the account id when the real client provides one. `SessionInfo`
    has none yet, so it is the email compared without case or edge spaces: a change of email on the Engine would
    count as another account and start the settings from nothing.

`LocalOnly` is provisionally accepted by Engineering 1 for the local end-to-end test, on three conditions, which are
the rule:

1. **No non-loopback Engine in any release build.** `LocalOnly` accepts loopback hosts only and refuses every other
   address, whatever the build. A test covers refusal of a non-loopback address under `LocalOnly`.
2. **Real sign-in still waits** for approval of the threat model. `LocalOnly` is for throwaway test accounts on the
   local stack.
3. This rule is written here, and the gate's code comment points to it.

Further conditions from the security review: `LocalOnly` is not selectable in release builds (compiled out or
debug-only); the loopback address check above runs on every request, with redirects off; and it uses the in-memory
`KeyStore` only, never the real keychain. A same-user process could run a fake Engine on the loopback port and get the
OPAQUE registration record during sign-up, which allows offline password guessing: fine for test accounts, so **never
use a real password with the local stack**.

## Sensitive account actions

Decision D12 (the Engine side is orbyn-engine#120): removing a device, signing out every session, turning two-factor
off and changing protection need the password typed again within the last 10 minutes, as account deletion already does.
The app asks with a "Confirm it's you" password prompt on every platform, enforced in `orbynd`. In the app a
confirmation is **for one action** (and one device when it removes a device), is **used up by its first use**, lasts at most
10 minutes, is kept in memory only and is cleared by sign-out, sign out everywhere and a restart. **Signing in is not a
confirmation**, so the prompt appears even right after signing in. This combines with the Engine's recent-sign-in rule
by asking for more, never less: the app's check is the re-login the Engine counts, and the Engine still refuses an
action without one of its own. A service token held by an engine cannot reuse a confirmation the person gave for
something else. Approving a device
keeps its two-sided digits plus the password. Signing out only this device stays open. The Account page also says that
on Windows any program you run can act as this device. The Designer is drawing the prompt and that note, so **the UI
for both waits on their frames**.

Autopilot's always-denied list now includes "changing account, device or sign-in state" ([crew.md](crew.md#autopilot)).
Every account route that changes state must be refusable by that policy (Crew slice 4f).

## Out of Phase 1

- Account recovery, password change, sharing, connected-app grants, Advanced protection, remote access and pairing.
- Anything that syncs besides settings: execution profiles, workflows, prompts, environment-set names, Crew defaults,
  trust scores, review memory, and the Crew ledger (never by default).
- Crew proof signing with a device key (D8).
- Production Engine address, open sign-up, and a keychain on Linux.
- Two-factor management and recovery codes in the app: for the beta, turning two-factor on or off lives on orbyn.dev.

## Behaviour the designs fix

From the Account and sync decisions in the plan (2026-11-11, Engineer Designs after orbyn-designs pull request #155).
The app records them here so the backend carries what the screens will need.

- Sign-in takes an authenticator code (six digits) or a two-factor recovery code in the form `XXXXX-XXXXX`: ten codes,
  shown once, plain base32, input ignores case, the hyphen and spaces. **Five wrong codes close the sign-in.**
- New-device approval is a two-way check: a trusted device allows the request and shows six digits, and the new device
  types them (three tries, single use). **The approval expires after 10 minutes.**
- Turning two-factor on or off is not in Orbyn Engineering, and the account-site page for it will not exist in the beta
  (Planner work is paused); signing in with an authenticator or recovery code still works. **Password change and account
  recovery are Planned, not built.**
- There are three code formats: the 8-character email code (a 30-symbol alphabet with no 0, O, 1, I, L or U), the
  six-digit authenticator code, and the recovery code above.
- An account has only an email address: no name field and no "Edit name". Devices have names the person chooses, 1 to
  100 characters.
- The newer change wins automatically. A conflict shows only for the same setting changed on two devices while offline,
  with "Use the other value" afterwards. First sync of a local-only workspace defaults to "Combine them". Devices show
  "Trusted since <date>"; a device under an hour old marked "Too new for some actions" is **Planned**: the Engine has no such rule yet.
- The sidebar footer shows avatar, sync status and gear when signed in; signed out it keeps the Settings row; below the
  collapse width the status moves to a title-bar icon. Only zones that exist are listed in the sync popover.
- Copy says every trusted device can open all of the person's Orbyn data (D10).

## UI waiting for the designs

**Status.** The sign-in routes, the gate and the "Confirm it's you" guard are built
(`account::flow`, `routes/account.rs`, pull request 428). The first screens are built from designs v91
(pull request 429): Settings > Account signed out, sign in with two-factor, sign up (with or without an
invite, the device check, the emailed code), the signed-in page with Sync now, Pause and Resume sync, the sync
line and popover with the title-bar icon, and the "Confirm it's you" dialog. The next pull request (designs v93)
adds the device list and Remove device, Sign out and Sign out everywhere through that dialog, the paused-cause
popovers, and the code-age hint. **New-device approval and "Connect this computer's settings"** are built behind the account module against the
stand-in and have their screens (the Devices page lists the requests; the first sync asks how to join the settings); the real device client (engine-v0.2.4) sits behind the same traits (`account::engine`), unselected while sign-in is off. **Not in it:** renaming a device (the Engine's agreed device client has no rename call); a removed-device notice that names a removal (the Engine answers a removed device and an ended session with the same
401, so `signout-other-device-removed` says neutrally "This device may have been removed, or your sessions ended", once, and
not after the person's own sign out everywhere: the service keeps that in memory, `ended_notice` and `POST /api/account/ended/seen`);
the "Too new for some actions" rule (Planned); and the deletion date in the blocked dialog (the view carries no date).
The designs decided that with the gate `Off` Settings > Account is hidden, the palette has no Account entry, and the
sidebar footer shows no sync status. The sections below are the plan as written before that work.

**Sign-in approval, what a real sign-in can show.** The Engine\'s `deny(id)` and a cancel look the same and carry no
reason, so on the new device the states are: waiting (the time left, no digits), the six digits to type (with "2 tries
left" after a wrong entry), approved (a spinner and "Signing you in…"), "Denied or cancelled." (any close from the other
side, including Cancel on the trusted device\'s digits dialog), "Too many tries. This request is closed." (the tries ran
out) and "This request expired" (ten minutes, counted on this computer\'s clock). There is no "They don\'t match" and no
`codes_differ` answer any more. When the app cannot reach the server with the digits it shows the existing "Couldn\'t
check this request." state (the entry was not used and is not kept); no separate frame draws that case. On the trusted
device a request that is gone reads "Denied or cancelled." too.

**Planned is design-time only.** In the beta a Planned row or label is communication in the frames and nothing
the product shows, unless the owner says otherwise: no Security group (password, two-factor, passkeys), no "Forgot
your password?", no passkey sign-in row. Two-factor sign-in with an authenticator or recovery code works;
enrolling or turning it off is not in Orbyn Engineering.

**No UI is built in the design pull request.** The Designer merged the frames in orbyn-designs pull request #155
(`engineer-designs.html`). A full-size check of those frames is still to come in a separate pull request, and
**building waits for that check**. The frames the UI will be built from:

| Prefix | Frame ids |
| --- | --- |
| `acct-signin*` | `acct-signin`, `acct-signin-checking`, `acct-signin-invite-required`, `acct-signin-narrow`, `acct-signin-network`, `acct-signin-throttled`, `acct-signin-wrong-password` |
| `acct-signup-*` | `acct-signup-invite`, `acct-signup-details` (the email step), `acct-signup-password`, `acct-signup-password-weak`, `acct-signup-password-mismatch`, `acct-signup-password-network`, `acct-signup-check`, `acct-signup-check-failed`, `acct-signup-too-many`, `acct-signup-email-code`, `acct-signup-code-wrong`, `acct-signup-code-expired`, `acct-signup-code-resent`, `acct-signup-code-network`, `acct-signup-done` (built from Engineer Designs v104) |
| `acct-2fa-*` | `acct-2fa-code`, `acct-2fa-wrong`, `acct-2fa-recovery`, `acct-2fa-recovery-wrong` |
| `acct-device-*` | `acct-device-waiting`, `acct-device-enter-digits`, `acct-device-approve-prompt`, `acct-device-approve-digits`, `acct-device-codes-mismatch` (now the wrong-digits state), `acct-device-digits-locked`, `acct-device-approved`, `acct-device-denied`, `acct-device-timeout` (built from Engineer Designs v104; `acct-device-check-failed` was already built) |
| `acct-reauth-*` | `acct-reauth`, `acct-reauth-network` (built from v104: the network failure of "Confirm it's you") |
| `acct-settings-*` | `acct-settings-signed-out`, `acct-settings-signed-in`, `acct-settings-loading`, `acct-settings-error`, `acct-settings-offline`, `acct-settings-narrow` |
| `acct-connect-*` | `acct-connect-workspace`, `acct-connect-progress`, `acct-connect-done` |
| `sync-*` | `sync-line`, `sync-line-narrow`, `sync-popover`, `sync-popover-offline`, `sync-popover-failed`, `sync-paused`, `sync-first-sync`, `sync-conflict`, `sync-conflict-resolved` |
| `devices-*` | `devices-list`, `devices-empty`, `devices-loading`, `devices-error`, `devices-narrow`, `devices-rename`, `devices-this-device-detail`, `devices-waiting-approval`, `devices-remove-confirm`, `devices-remove-too-new`, `devices-removed` |
| `signout-*` | `signout-confirm`, `signout-with-unsynced`, `signout-offline`, `signout-other-device-removed`, `signout-done` |

**Out of the build:** anything marked Planned inside a frame (passkeys, recovery, the "Forgot your password?" path, the
password row), every `acct-password-*` frame, the remote-access `scopes-*` frames, and `sync-popover-sample-later`
(marked "sample for later"). The Planner's `p-signup-*` and `p-sign-in-*` frames are references only.

Two places where the frames and the decisions differ, for the Designer to settle before building:

- `acct-device-waiting` says the request expires after five minutes; the decision says 10.
- `acct-settings-signed-in` lists "the name and email", but an account has only an email address.

## Open questions for Engineering 1

Questions about the snapshot, the dependencies, `LocalOnly`, the local stack and the settings list are answered and
folded in above. These remain, with Engineering 1's security reviewer:

1. The threat model's [open questions](account-threat-model.md#open-questions).


## The adapter as built

`account::engine` puts one `DeviceClient` behind both app traits: `EngineAccount` answers `AccountApi` and `EngineZones`
answers `DeviceApi`. Nothing selects it yet, and it is behind the cargo feature `engine-client`, which no normal build (release included)
enables; the tests switch it on, so a release has neither the adapter nor the dependency. The gate must be `LocalOnly` (debug and test builds only),
`AccountBackendKind::Engine` exists only there, and `connect` accepts a loopback IP literal only (the client's
`is_loopback_literal`) with an in-memory key store, never the keychain. Under `Off` nothing is constructed or called
(tests: `under_the_off_gate_nothing_builds_or_calls_the_real_client`, `only_a_loopback_address_builds_the_real_client`).
No sign-in condition is satisfied by this change.

Where the Engine's API differs from the traits the screens and the worker were written against:

- **Settings model.** The Engine's per-record model is the source of truth for the merge: `push` stages with
  `Engine::stage`, whose merge decides, and answers with the Engine's merged record, which the worker takes into the store
  as a conflict answer. The "offline edit lost" list is the diff of the queued edit against that result. A stamp maps
  one to one (`(millis, counter, device)` with the device as sixteen hex digits, which the store keeps as the zone's
  device name through `DeviceApi::device_tag`). The zone and record names (`ZONE_SETTINGS`, `PERSON_SETTINGS`,
  `PERSON_RECORD_ID`) and `Settings` come from the device client's re-exports, never spelled by hand and never from another
  Engine crate; `Settings::from_fields` drops the settings that never leave a device in both directions. The app's own
  rules (`settings_record`, `policy`, `apply`) still apply on top.
- **Snapshot.** The client does no IO. The Engine's snapshot is held in memory only for now (`SnapshotStore`); sealing it
  under the vault key is part of the change that turns sign-in on.
- **Invite.** The invite step calls `check_invite` (`POST /v1/invites/check`): a good code spends nothing, an unknown, used
  or expired one is `invite_required` (the drawn invite state) and too many wrong tries are `rate_limited` (the drawn
  throttled state). The stand-in does the same (`K7QM-2XWH-9QXA-3C2E` is rate limited).
- **Sign-up.** `begin_sign_up` makes a pending sign-up that is not live until the emailed code; nothing is stored until it
  is verified, and a computer that already belongs to an account is refused (`already_signed_in`), so no account is
  replaced (issue 489, item 5; the stand-in does the same: a sign-up for an address with an account is `invalid_code`).
  A code that was spent when the keys could not be saved is `keys_not_saved`; asking again calls `retry_save` and sends
  no new code. There is no drawn state for it, so the screen shows the message as an error.
- **Confirm it's you** is `reauthenticate`, a network login; a network failure is the existing `account_offline` state.
- **Sign out** forgets the device's keys, as the dialog says; the next sign-in is a new device.
- **Devices.** No `last_seen`. `trusted_since` is the device's creation time. The `app` label is the device's own claim
  and is shown as a hint only.

### Approval mapping

- **Trusted side.** The listed request is an untrusted (`Pending`) entry of `devices()`, its id the device's id, hidden
  after ten minutes (the new device gave up on its own clock). **Allow**, after the password, starts the approval
  (`start_approval`), waits (bounded, 20 s) until the new device has joined and the digits exist (`approval_state`:
  `Waiting`, then `Digits`), confirms (`confirm_approval`), sends the zone keys (`send_keys` for every zone; a failure is
  retried on the next pass) and answers with the digits, which reach the screen only through the approve answer.
  Confirming comes at Allow, not after the new device types, because the new device's `finish_approval` needs the
  approver's signature before it can accept anything; the typed digits are the check.
- **Don't allow** starts an approval and denies it at once (`start_approval`, `deny_approval`), which closes it on the new
  device immediately and needs no recent sign-in. If one cannot be started the error is shown, with the request still
  listed, and Don't allow can be tried again; there is no other fallback. A
  Don't allow that arrives while an Allow is running wins: it is checked before the signature is made (the approval is
  denied) and again after (the device is removed). **Cancel on the digits** removes the new device from the account, the
  only way to close a confirmed approval; the new device then sees "Denied or cancelled." What is kept for a device
  (hidden, confirmed, time left) is dropped once the Engine no longer lists it as waiting.
- **New device.** `NeedsApproval` waits with its own device id as the request id. Each look runs `pending_approval`,
  `join_approval` and `device_digits`; it shows the digits step only once the approver has revealed and signed. The typed
  digits are compared in the app first, with the three tries and a constant-time compare, because the Engine cancels an
  approval on one mismatch; only then `finish_approval(typed)` runs, never `finish_approval(None)`. A refusal there
  (another approver, a signature the new device does not accept, as for an out-of-date approver of an Engineering-labelled
  device) shows as the existing "Denied or cancelled." state; there is no drawn copy that says "update the other device"
  (a gap).
- **Time.** The Engine's `time_left(&approval)` (v0.2.2) is read when Allow confirms and passed on as `expires_in_secs` of
  the approve answer, so the digits dialog counts down from it. It is display only and decides nothing. Everywhere else
  (the request list, the new device's waiting and digits screens, which hold no approval on the trusted side) the screens
  say the request expires after ten minutes. `expires_in_secs` of a listed request is 0.
- **Reveal and ended approvals.** A network blip during the nonce reveal no longer cancels the approval (the client retries
  it); a confirm or deny on an expired, answered or denied approval is `ApprovalExpired`, shown as "That request has
  ended", and never approves.
