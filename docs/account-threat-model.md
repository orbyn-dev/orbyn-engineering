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
| Session token | OS keychain item, and process memory | It authorises requests to the Engine as this device |
| Vault key (32 bytes) | OS keychain item | Opens the sealed snapshot |
| Sync snapshot (`Engine::snapshot`: the zone-key cache and the cursors) | A file in the data folder, sealed with XChaCha20-Poly1305 under the vault key | It holds zone keys, so it is as secret as the keys. Zone keys open every record in the zone |
| Password | Typed once; held in memory as a `SecretString` for the length of one call | Never stored. It does not reach the Engine (OPAQUE) |
| The data folder (`orbyn.sqlite3`, `service.json`, account files) | The OS app-data folder | Holds all threads, settings, the outbox (private, though not key material) and the per-launch service token handle |

**D10 raises the stakes.** Every trusted device holds every zone key of the account, so a key taken from this app also
opens the person's Planner data, and the reverse. Copy must not suggest per-app isolation.

## The threat

Engines and agents run as the person. A coding engine, a script it writes, an MCP server or a skill it loads, or a
prompt-injected agent can all:

- read any file the person can read, including the data folder;
- read the environment of its own process and, on most systems, of other processes of the same user;
- run commands and open network connections;
- call the local Orbyn API if it finds the service token in `service.json`.

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
  - *Limit:* an unsigned development build has no stable identity, so the list protects nothing there. The release
    build must be signed (the Developer ID work in parity rows P34 and P35).
- **Windows.** Credential Manager encrypts each item for the signed-in user with DPAPI. **It offers no per-program
  rule**: any process running as the same user can read the item. Said plainly, on Windows the keychain defends
  against another user and against a stolen disk, not against an engine. There the other controls carry the weight,
  and the residual risk below stays open.
- **Other platforms.** Phase 1 supports sign-in on macOS and Windows only. Without a keychain the `KeyStore` is
  unavailable and sign-in is off; there is no fallback to a file.
- **What `KeyStore` holds.** Exactly the device signing key, the agreement key, the device id, the account id, the
  session token and the vault key. The zone-key cache is in the sealed snapshot, not in `KeyStore`.
- **Size.** Windows limits a credential to about 2.5 KiB, so the snapshot is not a keychain item: it is one file,
  sealed with the vault key (control 3 covers the file). The client does no IO, so the app writes it.

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
`:2516`). So no account route may hand back secret material (control 4). The sign-in and sign-up routes need the
password, which an engine does not have, and approving a new device needs a pending request that only someone with the
password can start. Sign-out and removing a device are reachable, and are a denial of service, not a disclosure
(see the residual risks).

## What this cannot defend

- **Same-user malware, and a person's own mistakes.** Code running as the person outside Orbyn can do everything an
  engine can, and more. The keychain's access rule on macOS is a speed bump for it, not a wall.
- **An engine with full disk access reading the keychain through the OS prompt.** On macOS, if the person grants
  Orbyn Full Disk Access, the engines it starts inherit it. An engine can then trigger the keychain prompt, and a
  person who clicks Allow hands over the item. The prompt names the requesting program, but the person must read it.
- **Windows.** No per-program control exists in Credential Manager (control 1).
- **A compromised Engine server.** Out of scope here; see the Engine's own threat model. D10 means its failure is
  not isolated per app either.

## Residual risks

1. **One key opens everything.** A device key taken from Engineering opens Planner's zones too (D10), until
   connected-app grants exist. The Account page says every trusted device can open all of the person's Orbyn data.
2. **Windows has no keychain separation between engine and person.** A coding engine on Windows can read the item.
   Mitigation is limited to controls 2 to 4 and a short session token.
3. **The local API token is readable by an engine.** It can sign the device out or remove a device. It cannot read a
   secret. Whether account-changing routes should demand something an engine cannot supply is an open question.
4. **Browser and computer actions.** An agent with the browser or the computer gateway might click Approve in
   Orbyn's own window. These actions always reach a person or a rule (`approvals.rs:101`, `mode_accepts`), but a
   person who allows them broadly weakens this. Approval also needs the digit comparison on the other device.
5. **Text matching is evadable** (control 3, last bullet).
6. **An unsigned or re-signed build** loses the macOS access rule and gets prompts or no protection.
7. **Backups.** Time Machine and similar copy the data folder, including the sealed cache. The vault key is in the
   keychain, which is backed up separately; a backup of both opens the cache.
8. **Local Engine for development** runs over plain `http` on loopback (D11): safe on one computer, wrong anywhere
   else, so the code refuses other `http` addresses.

## Checks

| Control | Check |
| --- | --- |
| Keychain access rule (macOS) | Unit test for the access-control construction; manual `security find-generic-password` from Terminal on a signed build must prompt |
| Environment scrub | `child_env` unit tests for every entry point; a spawned child prints its environment; the arch script rule on new spawns |
| Account files | Mode `0600` and folder `0700` test; write, read and `outside` read refused; an action naming the folder denied in every permission mode; data-folder scan for canary bytes after sign-in |
| No key in logs | Canary tests over errors, status, TRACE capture and diagnostics events; no request-logging layer; contract field test |
| Gate | A test that every sign-in route answers `account_disabled` under `Off` and sends no request |

## Open questions

For Engineering 1, with the lead:

0. `default_permission_mode` is on the agreed synced list. A looser mode set on one device then applies to engines on
   every other device. Is that accepted, or should a raise be held until the person confirms it on the receiving device?

1. Should account-changing routes (sign-out, remove device, approve) require something an engine with the service
   token cannot supply, such as a confirmation from the desktop shell, or is "denial of service only" acceptable?
2. Is the macOS "trust only `orbynd`" list compatible with the desktop shell starting `orbynd` as a sidecar with its own
   signature, and with how releases are signed?
3. Does the Engine want a short session-token lifetime on the desktop client, to reduce the Windows exposure?
