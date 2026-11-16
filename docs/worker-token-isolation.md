# Keeping person-only routes out of a worker's reach

Design note for [#441](https://github.com/orbyn-dev/orbyn-engineering/issues/441). It decides how
the service stops an engine process that runs as the person from calling routes only the person should call.

## The problem

One credential opens all of `/api`, `/agui` and `/ws`: the per-launch service token (`crates/orbynd/src/http/access.rs`,
`require_auth`). `orbynd` writes it to `<data>/service.json` (`main.rs`, `SERVICE_FILE`) so the `orbyn` CLI can find it
(`bin/orbyn.rs`, `Service::find`), and sends it to the desktop shell on the ready line. The file is mode 0600 on Unix
(`util::write_private_atomic`); on Windows no ACL is set, so it keeps the data folder's. Any process of the same OS user
can read it.

An engine child is such a process. With the token it can call every protected route as the person, including
`POST /api/approvals/{id}/decide` (approve its own request), `/api/threads/{id}/console`, `/api/updates/app/apply` and
the explorer's file actions. Today two flags stand in for a fix:

- Explorer file actions (#601) answer only under the Preview flag (`routes/explorer.rs`, `preview_gate`; core refuses
  too in `project/explorer/write.rs`).
- Crew routes and tools are Preview-only (`docs/crew.md`, slice 4f).
## What exists today

- **macOS Crew workers** start under `sandbox-exec` with the data folder denied (`host::confine`, #527). So a confined
  worker cannot read `service.json`. Windows and Linux workers are refused (hard rule 7). This covers Crew workers only.
- **Every other engine run** (a person's own threads, the lead, a terminal engine) is unconfined on every platform, and
  `crew_confine_workers` can turn the profile off. These can read `service.json`.
- Env: `host::child_env` already strips account names, including `ORBYN_SERVICE_TOKEN`. The token is not in any child's env
  or argv today. The leak is the file.
- Precedent for a credential no file holds: approve-once needs a nonce that only the desktop shell can obtain over the
  stdio channel (`host::presence`, `403 desktop_required` without `--parent-stdin`).

## Options

**(a) Move the token out of a worker's path.**
- A 0600 file outside the data folder gains nothing. An unconfined worker reads any file of its user, and a confined
  macOS worker is protected today only because the file is inside the denied data folder. Moving it out removes that.
- The OS keychain helps on macOS only: an item whose access list trusts only the `orbynd` and `orbyn` signatures makes
  another program prompt. That needs a stable Developer ID identity; CI ad-hoc signs today, so the list would change
  every build ([account threat model, control 1](account-threat-model.md)). Main has only the development key store.
- On Windows, Credential Manager and DPAPI open to any process of the same user. No gain.

**(b) Per-worker scoped token** (task-scoped, small route allowlist, no person routes, revoked at the end).
- Alone it protects nothing: the worker already holds, or can read, the full token, and it needs no HTTP API (it uses the
  per-thread `/mcp/{thread_token}` endpoint). A scoped token only matters once the full one is unreachable.
- It would become useful if a worker must run `orbyn status` or similar. No such need exists now.

**(c) Both.** Closes the gap on signed macOS builds only. Nothing on Windows, where a same-user process can read the
keychain entry, the file or the shell's memory alike.

## Recommendation

Neither (a) nor (b) works on Windows, and the token's problem is what it opens, not where it lives. So take the part of
(b) that holds on every platform and drop the per-worker token:

1. **Two classes of route.** `Local` is today's behavior, opened by the service token (what the CLI and browser mode need).
   `Person` routes also need a **person key**.
2. **The person key never touches disk, env, argv or a URL.** `orbynd` makes it at start and prints it only on the ready
   line the shell reads from the pipe it spawned (the same line that carries the token). The shell hands it to the
   bundled product page through an allowed Tauri command, as `crew_presence` is, and the page sends it in `x-orbyn-person`.
   `Access` keeps only its SHA-256. Without `--parent-stdin`, or for a remote session, a `Person` route answers
   `403 desktop_required`.
3. **A worker that reads `service.json` gets `Local` only.** A route that needs the key refuses it, so reading the file
   is no longer the way to act as the person.
4. **Per-worker tokens are deferred** until a worker needs the HTTP API, then added as a third class with a route allowlist
   and a run-end revoke. **(a) keychain storage** is later hardening for the `Local` token, after Developer ID signing.

Which routes are `Person`: explorer file actions, `approvals/{id}/decide`, questions answer/dismiss, `threads/{id}/console`
and terminal opens, `updates/*/apply`, account sign-in and key routes, Crew's person-only writes (already `local_only`).
The final list is an open question below.

## Rollout

1. Class table and key plumbing, with every protected route classified; an unclassified route fails a route-table test.
   No behavior change: all routes stay `Local`.
2. Shell and page send the key; `Person` is enforced for explorer file actions and approvals only.
3. Remove the explorer Preview gate and the core `crew_preview` check on file actions. Update `docs/architecture.md`
   ("Person only, and gated") in the same commit.
4. Move the remaining person routes over, one group per PR. Crew keeps its flag for readiness; the #441 sentences in
   `docs/crew.md` and `routes/crew.rs` go, and Crew writes need no worker-proof flag.

## Tests

- **Worker-env process.** Start the real service with `--parent-stdin` (as `tests/presence_stdio.rs` does). Spawn the fake
  engine through the real spawn path. Its script reads `<data>/service.json` and calls every `Person` route with the
  token: all `403 person_required`; `Local` routes still answer. Runs on macOS and Windows; the Unix mode check stays Unix-only.
- **The key is nowhere a file reader looks.** After start, search the data folder, the child's env and argv, and the
  service's own argv for the key: none.
- **Class table.** A test walks the router and fails for a route with no class; another fails if a handler listed `Person`
  is reachable without the key. A mutation (check removed) must fail both.

## Platform notes

- **macOS.** Sandbox and keychain stay as above. The key is in the shell's memory, which another same-user process cannot
  normally read it in a hardened-runtime build without a debug entitlement; a development or ad-hoc build gives no such guarantee.
- **Windows.** No sandbox exists for a worker, so Windows workers stay refused. Same-user `OpenProcess` can read the
  shell's memory, so the key is a barrier against file readers and env readers, not against a determined local debugger.
  A Low-integrity or AppContainer worker is the real answer and needs its own spike.
- Both: devtools is on in release builds (`docs/architecture.md`, shell section), so a script run in the page can use the key.

## What still stays open

A process of the same user can read the shell's memory (Windows always; macOS in unsigned builds), attach devtools, drive
the native dialog, or write into the shell's pipe. Anything the person can type in a terminal, a worker can type too, so
`Local` routes remain open to an unconfined worker, including the CLI's. This design removes the person-only routes from
that set; it does not make the data folder unreadable. Browser mode (`orbyn start`) has no key, so it loses `Person`
actions, as it already does for approve-once.

## Flags that can go

- Explorer file actions: the Preview gate and the core check (step 3). This is the one that goes first.
- The #441 wording on Crew's Preview flag (step 4). The flag itself stays while Crew is unfinished.

## Open questions for the owner

1. Accept that browser mode and the CLI cannot use `Person` routes (desktop only)?
2. Is `approvals/{id}/decide` a `Person` route? It is the largest hole today, and the change stops a page-less client
   from approving. It also stops `orbyn` from approving.
3. Split #441: this note closes "worker acts as the person"; the sandbox for Windows workers and the store read stay open.
4. Should the key travel by a Tauri command (proposed) or the webview's initialization script?

## Step 2: what is built

The owner accepted questions 1, 2 and 4 above (browser mode and the CLI cannot use `Person` routes, approvals included, and the
key travels by a Tauri command). This is what step 2 does.

- **The key.** `orbynd serve --parent-stdin` draws a 32-byte random key at start and adds it to the ready line as
  `person_key`. Without `--parent-stdin` there is no key and the line has no such field. `Access` keeps only the key's SHA-256.
  The channel is the pipe the shell spawned, the same stdout the token and `access_key` already use. Nothing writes the key to
  `service.json`, the environment, argv, a URL or a log (`tests/person_key.rs` searches for it in the data folder, the
  service's argv and env from the process table, the engine's own argv and env, and the service log).
- **The shell.** `core_process.rs` keeps the key in the `CoreProcess` it already owns. The `person_key` Tauri command returns it
  to the bundled page only: the same caller check as `crew_presence` (main window, bundled origin), listed in `build.rs` and
  `capabilities/main.json`. It is a command, not an initialization script, so the startup page and a preview window cannot read it.
- **The page.** `web/src/api/personKey.ts` asks the shell once, keeps the key in memory and sends `x-orbyn-person` on the routes in
  `PERSON_ROUTES` only. The header is in the native CORS allow-list (`NATIVE_HEADERS`) and is checked by the route, not by being
  allowed. In a dev or test build with no shell it sends a fixed fake key (`__ORBYN_PREVIEWS__` is false in production, so it is
  not shipped).
- **The service.** `routes/classes.rs` holds `ENFORCED`, a list of `(method, path)` that must also be in `PERSON`. The
  `require_person` layer sits inside `require_auth` (no credential is still `401`) and checks the matched route pattern: an
  enforced route without a valid key is `403 person_required`; a service with no shell is `403 desktop_required`. The compare is
  constant time (digest to digest, `secrets_match`). Enforcing another route is one line in `ENFORCED`, one in `PERSON_ROUTES`
  on the web side, and a test fails for an entry that is not a `Person` row.
- **Enforced now:** `POST /api/projects/{id}/worktrees/{worktree}/file-actions` and `POST /api/approvals/{id}/decide`. The other
  `Person` routes stay classified only until their step. The explorer Preview gate stays until step 3.
- **Development override.** `ORBYN_DEV_DISABLE_PERSON_KEY=1` lets `Person` routes through with no key, so web tests, the
  browser e2e and local development work. It is off by default, read only under `cfg(debug_assertions)`, and not compiled into a
  release build, which logs a warning and ignores the variable. Every Rust integration test and the real-service e2e set it,
  except `tests/person_key.rs`, which sets it to `0`.

### What browser mode and the CLI lose

Browser mode (`orbyn start`, a tab on the service address, a remote session) has no shell, so no key: an enforced route answers
`403 desktop_required`. Today that means no file actions in the explorer and no approving or denying from the browser. The `orbyn`
CLI reads `service.json` and has the token only, so it cannot call an enforced route either, and `orbyn` cannot approve. Both
still read, run threads and use every `Local` route. Each later group moved to `ENFORCED` adds its routes to this list.
