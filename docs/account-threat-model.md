# Orbyn account: threat model

The threat model D10 requires for the first account wiring. It belongs to the [Phase 1 design note](account.md).
**Signing in stays off until Engineering 1 (the Engine owner) has reviewed this document.** The sign-in gate in the design note
ships `Off`, and the gate opens in a separate change that links that review.

Citations are `file:line` at the commit this note was written against (`8dcbc913` on main). A control marked
**exists** is in the code today. A control marked **to build** is what the wiring pull requests must add, with the
check that proves it. Nothing here is a claim that the controls are in place before they are built.

## What is protected

| Asset | Where it lives | Why it matters |
| --- | --- | --- |
| Device signing key (Ed25519) and agreement key (X25519) | OS keychain item | They are this device's identity. With them an attacker signs changes as the device and opens zone-key envelopes sent to it |
| Session token | OS keychain item, and `orbynd` process memory | It authorises requests to the Engine as this device |
| Vault key (32 bytes), with the snapshot counter and hash | One OS keychain item (a 32-byte key plus a counter and a hash fit well inside a Windows credential) | Opens the sealed snapshot, and lets the app refuse an older one |
| Sync snapshot (`Engine::snapshot`: the zone-key cache and the cursors) | A file in the data folder, sealed with XChaCha20-Poly1305 under the vault key | It holds zone keys, so it is as secret as the keys. Zone keys open every record in the zone |
| Password | Typed once; held in memory as a `SecretString` for the length of one call | Never stored. It does not reach the Engine (OPAQUE) |
| `orbynd`'s memory | The running process | Holds the session token and the loaded device keys and zone keys in the clear while the app runs |
| The app's own binaries (`orbynd`, the desktop shell, the web assets) | The install folder | Whoever replaces them sees the next password the person types |
| The data folder (`orbyn.sqlite3`, `service.json`, account files) | The OS app-data folder | Holds all threads, settings, the outbox (private, though not key material) and the per-launch service token handle |

**D10 raises the stakes.** Every trusted device holds every zone key of the account, so a key taken from this app also
opens the person's Planner data, and the reverse. Copy must not suggest per-app isolation.

## The threat

Engines and agents run as the person. A coding engine, a script it writes, an MCP server or a skill it loads, or a
prompt-injected agent can all:

- read any file the person can read, including the data folder;
- read the environment of its own process and, on most systems, of other processes of the same user;
- run commands and open network connections;
- call the local Orbyn API if it finds the service token in `service.json`;
- read the memory of other processes of the same user, where the OS allows it (control 6);
- replace programs the person can write to, including the app's own (control 7).

Orbyn can approve or deny only the actions an engine asks it about. It never hears about an engine's own file reads
(the gate's action kinds are `Command`, `FileChange`, `Tool`, `Computer`, `Browser` and `Network`:
`crates/orbyn-contracts/src/governance.rs:8-15`, and the mapping in `agents/driver/acp/permissions.rs:90-110`). So the
design makes secrets **unreadable by position**, not by a rule an engine could step around.

## Controls

### 1. Secrets live in the OS keychain, behind access rules (to build)

- **macOS.** Each item is created with an access control list that trusts only the `orbynd` executable, by its code
  signature. Another program, including a coding engine started by `orbynd`, gets a system authorisation prompt
  instead of the secret. The items use a fixed service name and an account name derived from the data folder's profile id.
  - *Check:* on a signed build, `security find-generic-password -s <service> -a <account> -w` run from Terminal must
    raise the system prompt, or fail when run without a user. A unit test covers the access-control construction.
  - *Rule and conditions* (Engineering 1): a rule that trusts only `orbynd` works with the desktop shell's sidecar. It
    names `orbynd`'s designated requirement (team ID plus identifier) and survives updates signed with the same
    Developer ID and identifier. It needs Developer ID signing with hardened runtime and the entitlements in control 6
    (the signing work is parity rows P34 and P35).
  - *Development builds.* An unsigned or ad-hoc build has no stable identity, so the list protects nothing there. Such
    builds use a different service name or the in-memory `KeyStore`, and never create an item a release build would
    later trust.
  - *API.* The old trusted-application ACL API is deprecated. The data-protection keychain needs
    `keychain-access-groups` on the sidecar, and **the provisioning that needs under Developer ID is not verified**.
    The implementation PR must settle which API is used and verify it.
  - *"Always Allow"* on the system prompt widens the rule to that program permanently, for the item. The person must
    read the prompt, and the Account page should not teach them to click it.
- **Windows.** Credential Manager encrypts each item for the signed-in user with DPAPI. **It offers no per-program
  rule**: any process running as the same user can read the item. Said plainly, on Windows the keychain defends
  against another user and against a stolen disk, not against an engine. **On Windows any program the person runs can act
  as this device**: it can read the device key and the token (and, per control 6, orbynd's memory). The other controls
  carry the weight there, and the residual risk below stays open. The Account page says so (a design request, below).
- **Other platforms.** Phase 1 supports sign-in on macOS and Windows only. Without a keychain the `KeyStore` is
  unavailable and sign-in is off; there is no fallback to a file.
- **What `KeyStore` holds.** Exactly the device signing key, the agreement key, the device id, the account id, the
  session token and the vault key. The zone-key cache is in the sealed snapshot, not in `KeyStore`.
- **Size.** Windows limits a credential to about 2.5 KiB, so the snapshot is not a keychain item: it is one file,
  sealed with the vault key (control 3 covers the file). The client does no IO, so the app writes it.
- **The snapshot needs rollback protection, not only secrecy.** It carries the newest verified protection setting and
  the removal sequence, which only move forward (Engine `docs/architecture.md`, Keychain: the protection chain and
  removal sequences). An engine can put an older sealed file back. So:
  - the vault-key keychain item also holds a counter and a hash of the latest snapshot, and the app refuses a file
    that is older (lower counter) or does not match;
  - the snapshot is sealed with an AEAD (XChaCha20-Poly1305) whose associated data binds the profile id, the purpose
    (`orbyn/engineering/snapshot`) and the format version, so it cannot be moved to another profile or reused as
    another kind of file.
  - *Check:* tests that an older snapshot, one from another profile and one with a changed format version are each
    refused.
- **Sign-out destroys key material.** After sign-out, device removal or `Unauthorized`, the app deletes the device
  keys, the token and the vault key from the keychain and the sealed snapshot file; only settings stay. Without the
  vault key the old file cannot be read. Otherwise a removed device would keep every zone key (D10).
  - *Check:* after each of the three, the keychain has no item for the profile and the snapshot file is gone.

### 2. No account material in any process environment (to build)

An engine reads environment variables, so none may carry account material, in `orbynd` or in a child.

- **Never put it there.** The password, tokens and keys are never set as environment variables by Orbyn, the desktop
  shell or the tests. The only account variables are `ORBYN_ENGINE_URL` (an address, not a secret) and the test
  switches `ORBYN_E2E_*`.
- **Scrub before every spawn.** `child_env` is already the one place that decides what a spawned program inherits;
  its module comment says "every spawn goes through this file" (`crates/orbyn-core/src/host/child_env.rs:1-4`).
  The plan (`child_env.rs:87`, `plan`) gets a fixed list of account names and the reserved prefixes
  `ORBYN_ACCOUNT_`, `ORBYN_DEVICE_` and `ORBYN_KEYSTORE_`, removed for every kind of child. The three entry points
  all go through it: `for_tool` (`child_env.rs:160`), `for_engine` (`:165`) and `for_terminal` (`:172`).
- **The spawn paths.** Engines are built by `host::process::engine_command` (`host/process.rs:206-217`, which calls
  `for_engine`). Terminals and engine consoles use `for_terminal` (`host/terminal.rs:377`). Git, forge CLIs, editors,
  the browser, the computer sidecar and the ACP unpacker call `for_tool` (for example `source_control/git.rs:81`,
  `project/files.rs:760`, `governance/computer/cdp.rs:335`, `agents/acp_registry.rs:220`). The version probe
  can start with a cleared environment and a short allow-list (`PATH`, `HOME`, `LANG`, `LC_*` and a few Windows names) (`agents/engines/probe.rs:37-52`), which is the
  stronger form.
- **Two spawns skip it today.** The `kill` and `taskkill` helpers (`browser/preview/servers.rs:213` and
  `host/process.rs:364`) inherit the environment. They run for a moment and are never engines. The wiring pull request
  sends them through `for_tool` anyway, so the rule has no exceptions to remember.
- *Checks:* a unit test in `child_env.rs` feeds `plan` a set of variables with every reserved name and asserts each is
  removed for tool, engine and terminal; a test starts a child that prints its environment and asserts it holds no
  reserved name; and `scripts/check_rust_arch.py` (the existing text check for spawns, `docs/refactor.md`) gains a
  rule that a new `Command::new` or `CommandBuilder::new` in `orbyn-core` must be routed through `child_env`.

### 3. The account files are unreadable and hard-denied in Orbyn's own layers (partly exists)

- **File mode.** Account files are written with `util::write_private` or `write_private_atomic` (mode `0600` on Unix,
  the per-user ACL on Windows: `crates/orbyn-core/src/util.rs:105` and `:129`, and `docs/architecture.md:2511`). The
  account folder is `<data folder>/account/`, created `0700`.
- **What the files hold.** Non-secret ids, the outbox, and the sealed cache. Opening the sealed cache needs the vault
  key from the keychain. A file read alone yields no key.
- **Writes: exists.** A run may write only inside its checkout. `WriteScope::permits` resolves `..` and links and
  checks the checkout and the `excluded` list (`governance/approvals.rs:43-57`, `excluded` at `:26`), and
  `outside_scope` denies before policy, "so no rule and no person can widen a run's reach"
  (`approvals.rs:179-203`, called at `:216`). **To build:** every `WriteScope` gets the data folder in `excluded`, so
  a project the person added at a parent of the data folder still cannot reach it.
- **Orbyn's own file reads: exists, extended.** The checkout read routes go through `files::inside` and `open_inside`
  (`project/files.rs:317` and `:342`), which call `permits`, so the new `excluded` entry covers them. The person's
  view of a file outside the checkout already refuses the data folder (`project/outside_files.rs:190-217`, the
  refusal at `:215`; `docs/architecture.md:1520-1546`, which says no engine, MCP tool or agent route reaches that view).
- **Engine reads and commands: best effort only.** Before policy, `request_inner` (`approvals.rs:208`) gets a second
  check beside `outside_scope`: an action whose target, paths or detail name the account folder, `service.json` or a
  reserved variable is denied, with an audit row (rule 6, rule 7 in `docs/architecture.md:22-40`). This matches text, so
  a shell command that builds the path indirectly gets past it. **It is not a boundary.** The boundaries are controls 1
  and 2: there is nothing readable in the folder to take.
- *Checks:* tests that a write, a checkout read and an `outside` read of the account folder are each refused; that an
  action naming `<data folder>/account` is denied with an audit row even with an allow rule and in Full access mode;
  and that no secret appears in a plain file (the test signs in with a fake client and scans the data folder for the
  canary key bytes, base64 and hex).

### 4. No key material in logs, traces, events or the API (to build; the paths exist)

The channels a secret could leak through:

- **Warnings and errors reach diagnostics and the OTLP export.** `orbynd` records every event at warning level and
  above with its fields formatted by `Debug` (`crates/orbynd/src/main.rs:411` and `recent_events.rs:25-37`), and
  hands each to the Maintenance page and to `offer_log` for telemetry export (`operations/diagnostics.rs:146-148`,
  `operations/telemetry_export.rs:208`). So a `warn!` that formats a struct holding a key would be exported.
- **The local trace** holds only names and timings, "payload-free" (`operations/local_trace.rs:1`).
- **The Engine's secret types** wipe on drop and never print their bytes (Engine `docs/architecture.md`, Cryptography:
  "Secret types wipe themselves on drop and never print their bytes").

Controls:

- Passwords and tokens are `SecretString` or the Engine's secret types, whose `Debug` prints no bytes. The account
  module has no type with a derived `Debug` that contains one.
- Account errors and status values carry a kind and a safe message, never a request or response body, a header or a
  key. The module logs the kind and the zone, nothing else.
- `orbynd` has no request-logging layer at this commit (no `TraceLayer` in `crates/orbynd/src`), which matters because
  the sign-in request body holds the password. A test keeps it that way for the account routes.
- No route, event, CLI command or contract type returns a key, a token or a zone key. A test lists every field of the
  account contract types and fails on a name that suggests one.
- *Checks:* a test formats every public account error and status with canary secrets in scope and asserts none
  appears; a test signs in and syncs against the fake client with `tracing` captured at TRACE and asserts no canary in
  any event; and a test reads the diagnostics' recent events after a failed sign-in.

### 5. The local API is not a way around the keychain (to build)

An engine that finds `service.json` holds the service token and can call any `/api` route (`docs/architecture.md:2450-2460`,
`:2516`). Two things follow.

- **It cannot read a secret.** No account route may hand back secret material (control 4).
- **It can make `orbynd` sign.** The engine cannot read the keys, but through the daemon it can, per the Engine's
  architecture (Accounts and devices, Keychain):
  - sign out every session (a bearer token is enough);
  - remove a device;
  - turn two-factor off (a device signature, no code);
  - change the protection setting, including to Advanced, which destroys the escrow copy;
  - record deletes and zone-key rotations.

  That is more than a denial of service. Approving a **new** device is different: it needs a person at both devices (the
  trusted device allows it, with the password, and shows six digits that the new device types), and starting a pending
  device needs the password, so an engine cannot do it alone.

Controls (decision D12 and Engineering 1's answer):

- **Confirm it's you.** Removing a device, signing out every session, turning two-factor off and changing protection
  each need the password typed again within the last 10 minutes, as account deletion already does. The Engine side is
  orbyn-engine#120 and is needed before real-account sign-in (not for the local end-to-end test). The app asks with a
  "Confirm it's you" password prompt on every platform, and the check is in `orbynd`, not only in the page. Approving
  a device keeps its two-sided digits plus the password. Signing out **this device only** stays open.
- **The digits of a new device.** After Allow (which needs the password), the trusted device shows six digits and the new
  device types them; the sign-in is never finished by the allowance alone, and no code path finishes it without the
  digits. The rule is in `orbyn_core::account::digits`: three tries (an entry is a try even when the server counts too;
  the app takes it before the call), single use, a constant-time compare that looks at every digit so the time taken says
  nothing about how many were right, and ten minutes. The digits are six random digits from the operating system's
  generator, drawn without modulo bias, and the generator failing closes the request rather than falling back to a fixed
  value. The digits typed on the new device are never logged, formatted, stored or returned (they cross the account trait
  as an `ApprovalDigits`, which prints `[redacted]`); the digits shown on the trusted device are returned once, to the
  window that typed the password, and are not in the request list. Guessing six digits in three tries is a one in
  three-hundred-thousand chance per request, and a request closes after the third wrong try on both devices. Wrong digits
  do not say which digit was wrong, and the trusted device is not told which were typed.
  The confirmation is bound to the window that typed the password: `/confirm` answers with a one-time nonce (in
  memory, single-use, for one action and one device, request or choice) that the action must send back in a header,
  so an engine holding only the service token cannot run the action first, and cancelling the dialog drops the
  confirmation. An engine can still read the web page if it drives the browser; that is what the password and the
  loopback-only rule are for, and the first sync's overwriting choices need the password too.
- **No native-shell presence option.** It was dropped: an agent's browser or computer tools can drive the web UI, and
  the password is the one thing an engine does not hold.
- **Autopilot.** The always-denied list in the Crew plan now includes "changing account, device or sign-in state".
  Every account route that changes state must be refusable by that policy, so slice 4f can deny it with no special
  case ([crew.md](crew.md#autopilot)).
- *Check:* tests that each of the four routes fails without a password in the last 10 minutes and that the route table
  is annotated so a new state-changing route cannot skip the rule.

### 5b. A peer cannot make this device more permissive

Synced data comes from other devices, any of which may be compromised. The apply path applies only the 11 allowlisted
keys plus keybindings; four keys that escalate are never synced, and so is every key that raises cost (the engine, model
and effort defaults and the model options); keybindings are restricted on apply and there is no option filter, because no option key is synced (design note, [No escalation across devices](account.md#no-escalation-across-devices)). Tests:
every non-allowlisted catalog key from a peer is not applied; a keybinding to an approval or confirm command is not
applied; the 51 non-allowlisted keys, including `default_model_options` and the cost keys, are kept and never applied; the cost and reach keys are asserted by name (see [What never syncs](account.md#what-never-syncs)).

### 6. Orbynd's memory (to build, as a release check)

A same-user process can read the session token and the loaded device and zone keys out of `orbynd`'s memory. On
Windows any process of the same user can. On macOS it takes `lldb` or `task_for_pid`, which hardened runtime blocks
unless the app carries `get-task-allow` or `disable-library-validation`. Controls:

- Release builds use hardened runtime and carry neither entitlement.
- *Release check:* `codesign -d --entitlements - <orbynd>` on the built sidecar must show neither
  `com.apple.security.get-task-allow` nor `com.apple.security.cs.disable-library-validation`, and `codesign -dv` must
  show the `runtime` flag. It runs where the release is verified (parity row P35).
- Secrets are zeroed on drop (the Engine's secret types) and held only as long as a call needs.
- **On Windows memory is as open as Credential Manager**: there is no equivalent of hardened runtime that stops a
  same-user process reading it.

### 7. The app's own binaries (to build; partly the OS)

With a user-owned install, an engine can replace `orbynd` or the web assets and capture the next password the person
types. A Windows per-user install, or a macOS app the person dragged to a folder they own, is writable by the person.

- **macOS** relies on App Management (an app may not modify another's bundle without permission) and on signature
  checks at launch, which fail for a replaced binary. These protect an installed, signed `.app`, not a development build.
- **Windows per-user install is fully exposed** to this, since the install folder belongs to the user. It is fixed
  only by a machine-wide install (the package's per-machine option, parity row P32), which needs administrator rights.
- The Account page could say this for Windows; it is a design request.
- *Check:* the release verification confirms the signature and the install location of the packaged app.

## What this cannot defend

- **Same-user malware, and a person's own mistakes.** Code running as the person outside Orbyn can do everything an
  engine can, and more. The keychain's access rule on macOS is a speed bump for it, not a wall.
- **An engine with full disk access reading the keychain through the OS prompt.** On macOS, if the person grants
  Orbyn Full Disk Access, the engines it starts inherit it. An engine can then trigger the keychain prompt, and a
  person who clicks Allow hands over the item. The prompt names the requesting program, but the person must read it.
- **Windows.** No per-program control exists in Credential Manager (control 1), and none for process memory (control 6) or a user-owned install (control 7).
- **A fake Engine on the loopback port** (local development only). A same-user process could listen where the local Engine runs and, during sign-up, receive the OPAQUE registration record, which allows offline password guessing. That is fine for throwaway test accounts. **Never use a real password against the local stack.**
- **The device's `app` label.** A device names itself `planner` or `engineering` when it signs in, and the v3 approval
  binds its name, platform and that label. It is the device's own signed claim, so a malicious or modified client can
  claim either one. It says what the device asserted, never which app is running. It is a hint for the person
  approving a device and is never proof: no code or copy treats it as an identity check, and the approve prompt shows it
  as a hint at most. What protects the account is the digits typed on the new device and the person's own judgement of
  the device.
- **A compromised Engine server.** Out of scope here; see the Engine's own threat model. D10 means its failure is
  not isolated per app either.

## Residual risks

1. **One key opens everything.** A device key taken from Engineering opens Planner's zones too (D10), until
   connected-app grants exist. The Account page says every trusted device can open all of the person's Orbyn data.
2. **Windows has no separation between engine and person.** A coding engine on Windows can read the item and the
   process memory, and replace a per-user install. Mitigation is controls 2 to 5. **A shorter session token does not
   help there**: the device key sits beside the token in a readable credential, so an engine can act as the device
   without the token. The session token is opaque, 32 bytes; each use pushes its expiry 30 days out, capped at one year
   after sign-in, and there is no refresh token. A device-signed refresh would not help on Windows either. The effort goes
   into control 5.
3. **The daemon will sign what the local API asks, until orbyn-engine#120 and the app's password rule are in.**
   Control 5 lists what an engine can do through it. Real-account sign-in waits for both.
4. **Browser and computer actions.** An agent with the browser or the computer gateway might click Approve in
   Orbyn's own window. These actions always reach a person or a rule (`approvals.rs:101`, `mode_accepts`), but a
   person who allows them broadly weakens this. Approval also needs the six digits, typed on the other device.
5. **Text matching is evadable** (control 3, last bullet).
6. **An unsigned or re-signed build** loses the macOS access rule and gets prompts or no protection.
7. **Backups.** Time Machine and similar copy the data folder, including the sealed snapshot. The vault key is in the
   keychain, which is backed up separately; a backup of both opens the snapshot, and restoring an old one is caught by
   the counter only if the keychain item is newer.
8. **Local Engine for development** runs over plain `http` on loopback (D11): safe on one computer, wrong anywhere
   else, so the code refuses other `http` addresses (see the address rule in the design note).
9. **Memory and binary replacement** (controls 6 and 7) stay open on Windows.

## Checks

| Control | Check |
| --- | --- |
| Keychain access rule (macOS) | Unit test for the access-control construction; manual `security find-generic-password` from Terminal on a signed build must prompt |
| Environment scrub | `child_env` unit tests for every entry point; a spawned child prints its environment; the arch script rule on new spawns |
| Account files | Mode `0600` and folder `0700` test; write, read and `outside` read refused; an action naming the folder denied in every permission mode; data-folder scan for canary bytes after sign-in |
| No key in logs | Canary tests over errors, status, TRACE capture and diagnostics events; no request-logging layer; contract field test |
| Memory | `codesign -d --entitlements` and the `runtime` flag on the release sidecar |
| Snapshot | Older, foreign-profile and wrong-version files refused; keychain and file empty after sign-out |
| Sensitive routes | Four routes refuse without a password in the last 10 minutes |
| Gate | A test that every sign-in route answers `account_disabled` under `Off` and sends no request |

## Open questions

Answered by Engineering 1 and folded in above: sensitive routes (decision D12), the macOS rule with the sidecar, and the
session lifetime. `default_permission_mode` is never synced (a loosened mode must not cross devices), so that question is closed. Still open, with Engineering 1's security reviewer:

1. Which macOS keychain API (control 1), and what provisioning it needs under Developer ID: not verified.

## Design requests (the Designer owns these; nothing is built)

- The "Confirm it's you" password prompt for the four sensitive actions in control 5.
- Account page copy that says, on Windows, any program you run can act as this device.

## Sign-up never makes a live account before the code, and never replaces one

The Engine's sign-up makes a pending sign-up at the password step; the account is live only when the emailed code is
verified. The app's adapter stores nothing until then, refuses a computer that already belongs to an account, and a
sign-up for an address that has an account ends as a wrong code and leaves that account untouched. The development
stand-in behaves the same. (Issue 489, item 5.)
