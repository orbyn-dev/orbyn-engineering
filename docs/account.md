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
| D5 | Confirmed, extended | The never-sync list also holds what is machine-specific or names a program, path or endpoint, and 26 more keys are held back from v1. See [Never synced](#never-synced) and [Not in v1](#not-in-v1-may-sync-later) |
| D6 | Clarified | Two crates are called `orbyn-contracts`: the Engine's (wire for the Engine's HTTP API) and this repository's (wire between the web UI and `orbynd`). The app never names the Engine's. Engineering's record fields live in the Engine's `orbyn-engineering` crate, so the app depends on that crate as well as on `orbyn-device-client` unless the device client re-exports it (open question 2). Nothing about a record is copied into this repository's contracts |
| D7 | Confirmed | Connected apps stay after the beta. The L52 to L55 screens stay `preview-only`, behind `__ORBYN_PREVIEWS__` ([design.md](design.md#planned-with-the-orbyn-platform)) |
| D8 | Confirmed | Crew proof signing uses a local keychain key until after the beta. That key is another asset for the threat model when it lands; it is not part of Phase 1 |
| D9 | Confirmed | The Engine is MIT or Apache-2.0, so the app may depend on it. The wiring pull request adds the dependency to [UPSTREAM.md](../UPSTREAM.md) with its notices. The Engine never depends on app code |
| D10 | Corrected | Confirmed: every trusted device holds every zone key, and no copy may suggest per-app isolation. Corrected: "reading credentials is already on the Autopilot hard-denied list" is **Planned** (slice 4f), not built. Today Orbyn hard-denies only writes outside the checkout, and the person's file viewer refuses the data folder. Orbyn never hears about an engine's own file reads, so it cannot deny them. The threat model therefore rests on keeping secrets out of files and out of the environment, with the hard-deny code as defence in depth |
| D11 | Clarified | See [The Engine address](#the-engine-address) and [Testing](#testing). Added: the end-to-end test needs two helpers from the Engine owner (create an invite, read the latest email code), and in CI a skipped test must fail |

## Dependency on the Engine

`orbyn-core` depends on the Engine's device client:

```toml
orbyn-device-client = { git = "https://github.com/orbyn-dev/orbyn-engine", tag = "engine-v0.1.0" }
```

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
  - `KeyStore`, which the app implements. It holds the device keys, device id, account id, session token and the
    zone-key cache.
  - `Clock`, so tests control time.
  - Sign-up: `check_invite`, `start_sign_up(invite, email)`, `resend_code`, `verify_email_code` (eight characters,
    normalised by the client), `register(password: SecretString)`. Proof of work is solved inside the client, and
    sixty starts an hour become `RateLimited { retry_after }`.
  - Sign-in: `SignInOutcome` is `Signed(Session)`, `SecondFactor(challenge)` or `AwaitingApproval { request_id,
    digits }`. A trusted device lists `approval_requests()` and answers `approve(id)` after the digit comparison, or
    `deny(id)`.
  - Sync, per zone: push and pull, with sealing, zone keys and rotation inside the client. A conflict returns plain
    records and the app merges them with `orbyn-sync`, which the client re-exports. An upload is at most 4 MiB.
  - `devices()` returns `Device { id, name, app, last_seen, current }` (`last_seen` is coarse RFC 3339, `app` is
    `planner` or `engineering`).
  - `status()` is a watch of signed out, signing in, online, `offline { since }` and `paused { reason }`. Live notices
    come over a websocket the client keeps alive and reconnects.
  - Errors: `Offline`, `Unauthorized`, `InviteRequired`, `WrongPassword`, `RateLimited { retry_after }`, `Conflict`,
    `TooLarge`, `InvalidKey`, `Server { retryable }`, `InvalidCode`, `DeletionScheduled`, `ProtectionAdvanced` and
    `AccountGone`.
- **Does not exist yet.** At this commit the Engine has no `orbyn-device-client` crate, no `docs/engineering.md` and no
  `engine-v0.1.0` tag. Until they land, the app builds the account module against a fake `DeviceApi` that lives in the
  app's test support.

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
per-field hybrid logical clock, the larger stamp wins, ties break by canonical JSON. Incoming fields pass the same
validation as a local write (`check` in `preferences.rs`), through a private apply path that does not queue them again.
A field that fails validation, for example one written by a newer app, is kept and passed through unchanged and is
not applied locally.

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
| `Unauthorized` | Signed out with the reason "session ended". Local data and the outbox stay; sign in again to resume |
| `RateLimited { retry_after }` | Shown with the wait; no retry before it |
| `ProtectionAdvanced`, `DeletionScheduled` | Paused with that reason |
| `AccountGone` | Signed out locally, data kept, the reason stated |
| `InviteRequired`, `WrongPassword`, `InvalidCode` | Returned to the sign-up or sign-in step that asked |

## The device and its keys

One data folder is one device. First run creates a random, non-secret profile id in the data folder. Keychain items
are named from it, so two data folders on one computer never share keys. The device registers with the `app` label
`engineering` (Engine issue #73). Its name is chosen by the person.

Secrets live in the OS keychain through the app's `KeyStore`, never in a plain file: the Ed25519 signing key, the X25519
agreement key, the session token, and a 32-byte vault key. The zone-key cache and the client's sync snapshot hold
zone keys and can be larger than one Windows credential (about 2.5 KiB), so they are sealed with the vault key and
kept as a file in the data folder. The [threat model](account-threat-model.md) says what each of these protects against.

On a platform with no supported keychain (Linux in Phase 1), `KeyStore` reports itself unavailable and sign-in stays
off. The account never falls back to a plain file.

## The Engine address

- A built-in default: `https://api-staging.orbyn.dev`.
- A development override from `ORBYN_ENGINE_URL`, read by `orbynd` into `CoreConfig` the way `ORBYN_ACP_REGISTRY_URL`
  is today (`main.rs:136`). The local Engine stack answers at `http://127.0.0.1:8080`.
- **Plain `http` is accepted only when the host is a loopback address** (`127.0.0.0/8`, `::1` or `localhost`); every
  other address must be `https`. The rule is one function in `account::engine_url`, tested with `http://evil.example`,
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

**Synced keys** (21 of the 62 in the catalog in `preferences.rs`, plus keybindings). This is the list agreed with
Engineering 1. Only the person-level value of a key syncs.

| Group | Keys |
| --- | --- |
| Appearance and reading | `default_theme`, `response_streaming_mode`, `preview_appearance`, `preview_zoom`, `open_web_links_in_app`, `quit_confirmation` |
| Engine and model defaults | `default_engine`, `default_model`, `default_effort`, `default_model_options`, `fallback_model`, `text_generation_model` |
| Model lists | `favorite_models` and `hidden_models` (sets), `model_order` (one field) |
| Thread and branch defaults | `default_permission_mode`, `default_thread_env_mode`, `auto_thread_titles`, `branch_naming_mode`, `branch_name_prefix`, `branch_name_instructions` |
| Keybindings | One field per (key, `when`) pair |

A synced `default_engine` or `default_model` may name an engine or model this computer does not have. The existing
fallback behaviour applies; the value is kept as set. A synced `default_permission_mode` changes what agents may do
on the receiving computer without a prompt there; the threat model lists this as an open question.

### Never synced

Fifteen keys, by decision:

| Why | Keys |
| --- | --- |
| Describes this computer | `environment_icon`, `keep_awake_during_runs`, `background_activity_profile`, `add_project_base_directory` |
| Programs, paths, endpoints | `provider_instances`, `custom_models`, `observability`, `storage_cleanup`, `worktree_cleanup` |
| Per-computer update and polling | `app_update_mode`, `automatic_git_fetch_minutes`, `provider_health_refresh_minutes` |
| Login and engine availability | `prefer_saved_login`, `disabled_engines` |
| Service-owned stamp | `default_theme_set_at` |

### Not in v1, may sync later

The other 26 keys are not synced in v1, so 21 + 15 + 26 = 62:

| Key | Reason |
| --- | --- |
| `default_project_scripts` | A synced command would run on another computer |
| `auto_settle_after_days`, `auto_settle_on_merge`, `auto_settle_keep_pull_requests` | Project lifecycle defaults; held back until project defaults sync |
| `auto_pull`, `new_worktrees_start_from_origin`, `worktree_submodules` | Change what git does on each computer; held back with project defaults |
| `remove_agent_credits_on_merge`, `pull_request_merge_method`, `review_guides_enabled`, `review_guide_model` | Project and forge defaults; held back with project defaults |
| `project_title_instructions`, `review_guide_instructions`, `source_control_writing_style` | Free-text prompts; wait for the prompts record |
| `enable_agent_browser_access`, `enable_agent_device_access` | Grant agents access to this computer's browser and devices |
| `enable_provider_update_checks`, `continue_threads_after_update`, `warm_next_turn`, `preview_profiles` | Per-computer behaviour or local browser data |
| `compact_idle_threads`, `idle_compaction`, `snooze_limited_threads`, `auto_resume_limited_threads`, `task_skip_near_limit`, `poll_pull_requests` | Thread and polling behaviour; not requested for v1 |

Also never synced in Phase 1: `ServerSettings` (service-wide: auto-settle, snooze presets, model prices and aliases,
title instructions), every per-project override, the Crew ledger, transcripts, diffs, environment-set values,
provider keys, join tokens and any secret (D5).

## Testing

- **Unit tests** use a fake `DeviceApi` in the app's test support. It follows the contract's error set and can lie like
  the Engine's test server: lost answers, a stale version, a failing zone, a slow answer. Covered: sign-up and
  sign-in outcomes, approvals, the outbox surviving a restart, merge of two devices' edits, set semantics with
  tombstones, limits, unknown keys, every row of the error table, and that no password or key appears in any request
  the fake receives, in a log line, or in a status or error value.
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

## Sign-in gate

`CoreConfig` gets an account gate with three values: `Off` (the default: every sign-in and sign-up route answers
`account_disabled` and no request goes to any Engine), `LocalOnly` (only a loopback Engine address, for D11) and `On`.
The wiring pull request ships `Off` and `LocalOnly`. `On` is a separate small change that links the Engine owner's
written review of the threat model.

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
- New-device approval is a two-way code comparison on a trusted device. **The approval expires after 10 minutes.**
- Turning two-factor on or off lives on orbyn.dev for the beta. **Password change and account recovery are Planned,
  not built.**
- There are three code formats: the 8-character email code (a 30-symbol alphabet with no 0, O, 1, I, L or U), the
  six-digit authenticator code, and the recovery code above.
- An account has only an email address: no name field and no "Edit name". Devices have names the person chooses, 1 to
  100 characters.
- The newer change wins automatically. A conflict shows only for the same setting changed on two devices while offline,
  with "Use the other value" afterwards. First sync of a local-only workspace defaults to "Combine them". Devices show
  "Trusted since <date>", and one under an hour old is marked "Too new for some actions".
- The sidebar footer shows avatar, sync status and gear when signed in; signed out it keeps the Settings row; below the
  collapse width the status moves to a title-bar icon. Only zones that exist are listed in the sync popover.
- Copy says every trusted device can open all of the person's Orbyn data (D10).

## UI waiting for the designs

**No UI is built in this pull request.** The Designer merged the frames in orbyn-designs pull request #155
(`engineer-designs.html`). A full-size check of those frames is still to come in a separate pull request, and
**building waits for that check**. The frames the UI will be built from:

| Prefix | Frame ids |
| --- | --- |
| `acct-signin*` | `acct-signin`, `acct-signin-checking`, `acct-signin-invite-required`, `acct-signin-narrow`, `acct-signin-network`, `acct-signin-throttled`, `acct-signin-wrong-password` |
| `acct-signup-*` | `acct-signup-invite`, `acct-signup-check`, `acct-signup-check-failed`, `acct-signup-details`, `acct-signup-email-code`, `acct-signup-code-wrong`, `acct-signup-code-expired`, `acct-signup-too-many`, `acct-signup-done` |
| `acct-2fa-*` | `acct-2fa-code`, `acct-2fa-wrong`, `acct-2fa-recovery`, `acct-2fa-recovery-wrong` |
| `acct-device-*` | `acct-device-waiting`, `acct-device-approve-prompt`, `acct-device-approved`, `acct-device-denied`, `acct-device-timeout` |
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

1. Where does the client's `Engine::snapshot` persist? This note assumes the app seals it with the vault key through
   `KeyStore`. Is that the intended contract?
2. Does `orbyn-device-client` re-export `orbyn-engineering` (the record schema), or does the app depend on both?
3. Is `LocalOnly` (sign-in against a loopback Engine) acceptable before review? It is how the end-to-end test runs.
4. How does CI get the Engine stack: build from the pinned tag with compose, or a published image? And which two helpers
   (create an invite, read the latest email code) will the recipe provide?
5. `default_permission_mode` syncs by the agreed list. Is that intended, given a permission raised on one computer
   would apply on another? The threat model asks the same.
6. The threat model's [open questions](account-threat-model.md#open-questions).
