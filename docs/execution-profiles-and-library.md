# Execution profiles and the personal library

Two beta features from the Warp list in [crew.md](crew.md#warp-features): W8 execution profiles and W9 workflows,
prompts and environment sets. Both are local and on this device. Neither needs a server or an account.

This file is the contract. The stores and routes are built against it, and so is the web UI (the profile picker, the
Settings pages and the library panel). Change it by pull request, and say so to whoever builds on the part you change.
Status: **contract written, nothing built.** No route, table or keychain item below exists yet.

Owner rulings this file carries:

- **W8.** A profile is a named preset of engine, model tier, permission mode and Autopilot mode, chosen per thread or per
  crew. There is no read-only profile, because there is no read-only permission mode.
- **W9.** Workflows, prompts and environment sets live in an on-device library only. Orbyn never writes into a target
  repository (hard rule 4 in [architecture.md](architecture.md#hard-product-rules)). Reading an already committed `.orbyn/`
  file is fine. This replaces the "stored in `.orbyn/` in the repository" wording in the W9 row of crew.md.
- Environment-set **values are secrets**. They live in the OS keychain through `orbyn_core::credentials` and nowhere else
  (hard rule 1), and are redacted everywhere.
- Every write route is a **Person** route ([classes.rs](../crates/orbynd/src/routes/classes.rs)).

## Part 1: Execution profiles (W8)

### Shape

```jsonc
// Profile
{
  "id": "01j...",             // lowercase ULID from core (util::new_id); built-ins use "builtin:<slug>"
  "name": "Careful",          // 1 to 60 characters after trimming, no control characters, unique ignoring case
  "description": "",          // 0 to 200 characters
  "engine": "claude",         // an engine id, or null = "keep the thread's engine"
  "model_tier": "builder",    // "lead" | "builder" | "scout"
  "permission_mode": "supervised", // "supervised" | "auto_edits" | "auto" | "full_access" (the PermissionMode enum)
  "autopilot_mode": "off",    // "off" | "on" | "full" (the AutopilotMode enum)
  "builtin": false,           // true: read-only, listed first, cannot be edited or deleted; duplicate it to change it
  "revision": 3,              // starts at 1, +1 on every change; the client sends it back as seen_revision
  "created_at": "RFC 3339",
  "updated_at": "RFC 3339"
}
```

- Four settings, no more. No effort, no model id, no options: effort stays on the thread (the harness caps it) and a model
  id is resolved from the tier at apply time.
- `model_tier` uses the tier words of [crew.md](crew.md#design) (lead, builder, scout). The concrete model comes from the same
  mapping Crew uses: Opus, Sonnet, Haiku on Claude; Sol, Terra, Luna on Codex; the person's own tier mapping for an ACP
  engine. An engine with no mapping offers only `builder`.
- No `read_only` value exists and the server refuses one (`400 invalid_input`).
- Built-ins ship in code, not in SQLite: **Careful** (`supervised`, Autopilot `off`), **Fast** (`auto_edits`, Autopilot
  `on`), **Hands-off** (`full_access`, Autopilot `full`), all `engine: null`, `model_tier: builder`. The values are a proposal
  (open question 1).
- At most 50 saved profiles.

### Where a profile can be applied

| Target | Effect |
| --- | --- |
| New-thread draft | The picker fills the draft's engine, model and permission mode. Creating the thread sends those values; nothing refers to the profile after that except the chip |
| Existing thread | The apply route writes the thread's own settings once |
| Crew | The apply route sets the crew's engine tier, permission mode and its Autopilot mode (scope `crew`, the same store and checks as `POST /api/crew/autopilot/mode`) |

A profile is **copied on apply, not linked.** Editing or deleting a profile never changes a thread or crew that already
used it. Reason: a linked profile could raise a permission mode or an Autopilot mode on running work without the person
there. Apply is a Person route, so raising a mode always needs the person key, as `permission_mode` already does on
`POST /api/threads/{id}/runs` and `PATCH /api/threads/{id}`.

### Precedence with per-thread settings

1. A value the person sets on the thread afterwards (model, permission mode) always wins over the profile. The thread's
   application row is then marked `customized: true` and the chip reads "Careful, edited".
2. Apply overwrites the three thread fields it owns (engine, model, permission mode) and leaves everything else alone:
   effort, options, title, worktree, queue. A model picked from a tier carries no effort or options, per the existing rule
   that they travel only with their model; the thread's remembered effort for the new model applies.
3. An engine switch follows the existing engine-switch rule ([Models](architecture.md#models)): the new engine takes what
   was asked, never the old engine's values.
4. Autopilot mode is a Crew setting today (per crew, repository or default). On a thread outside Crew the profile's
   `autopilot_mode` is **not applied** and the answer lists it in `ignored`. The profile still shows it.
5. A run in flight is not touched. The change takes effect from the next turn's process, as a mode change does today.

### Validation

At save (create, patch) and at apply, against what the engine supports now:

| Check | At save | At apply |
| --- | --- | --- |
| `permission_mode` is one the engine offers (`capabilities.permission_modes`, from `permission_modes::offered`) | Only when `engine` is set. `400 invalid_input` naming the field | Always, for the thread's final engine. `409 mode_not_offered`. A mode is never swapped for the nearest looser one |
| `model_tier` has a model on that engine | Warning only (`warnings[]`), the catalog can change | `409 tier_unmapped`, with the engine named |
| Engine is registered and startable | Warning only | `409 engine_unavailable` |
| Autopilot value is `off`, `on` or `full` | Always | Always |

`warnings[]` is a list of `{field, code, message}` and never blocks a save, so a profile made on one machine state stays
valid on another.

### An engine that is not installed

A profile may name an engine this machine does not have (not found, can't start, disabled for the project, or a
`Broken` version). Rules:

- It saves, and lists with the label "Engine not found" (or "Can't start"), using the same `ProviderFacts` the Providers
  list and the crew router read.
- Apply is refused with `409 engine_unavailable` and the reason. Orbyn **never falls back to another engine**; this matches
  the rule for a disabled or missing configured provider. The picker shows the profile disabled with the reason, and
  offers "Edit profile".
- A thread or crew that already used the profile keeps working. Its own engine is its own business.
- `engine: null` never fails this check; the thread's engine is used.

### SQLite sketch

Migration numbers are the next free ones at build time. At the time of writing main ends at 59, so these are 60 and 61;
a later change that takes 60 first pushes these down, keeping the sequence contiguous.

```sql
-- 60: execution profiles
CREATE TABLE profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  name_key TEXT NOT NULL UNIQUE,            -- lower(trim(name))
  description TEXT NOT NULL DEFAULT '',
  engine TEXT,                               -- NULL = keep the thread's engine
  model_tier TEXT NOT NULL CHECK (model_tier IN ('lead','builder','scout')),
  permission_mode TEXT NOT NULL CHECK (permission_mode IN ('supervised','auto_edits','auto','full_access')),
  autopilot_mode TEXT NOT NULL CHECK (autopilot_mode IN ('off','on','full')),
  revision INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE profile_applications (          -- what a thread or crew last got; no foreign key to profiles on purpose
  scope_kind TEXT NOT NULL CHECK (scope_kind IN ('thread','crew')),
  scope_id TEXT NOT NULL,
  profile_id TEXT NOT NULL,                  -- may name a deleted profile or a built-in
  name_at_apply TEXT NOT NULL,
  revision_at_apply INTEGER NOT NULL,
  customized INTEGER NOT NULL DEFAULT 0,
  applied_at TEXT NOT NULL,
  PRIMARY KEY (scope_kind, scope_id)
);
```

Deleting a thread or crew deletes its application row in the same transaction. Deleting a profile leaves rows; the chip
then shows the stored name with "(removed)".

### Routes

Every route is under the existing bearer or session credential. Crew routes answer 404 with Crew Preview off.

| Method and path | Class | Purpose |
| --- | --- | --- |
| `GET /api/profiles` | Local read | `{ profiles: Profile[], limit: 50 }`, built-ins first, each with `availability` (below) |
| `GET /api/profiles/{id}` | Local read | One `Profile` with `availability` and `warnings[]` |
| `POST /api/profiles/check` | Local read | Takes a draft profile, answers `warnings[]` and `availability`. Saves nothing; used by the editor while typing |
| `POST /api/profiles` | **Person** | Create. Body is the shape minus `id`, `builtin`, `revision`, timestamps. Answers `201 Profile` |
| `PATCH /api/profiles/{id}` | **Person** | Change any of the settings or name. Body adds `seen_revision`; `409 revision_conflict` when stale. Answers the `Profile` |
| `DELETE /api/profiles/{id}` | **Person** | `204`. Built-ins: `403 builtin_profile` |
| `PUT /api/threads/{id}/profile` | **Person** | Apply to a thread, or clear (`profile_id: null`, which only forgets the chip and keeps the values) |
| `POST /api/crew/profile/apply` | **Person** | Apply to a crew. Body `{ crew_id, profile_id }`. Same rules, same audit row as the Autopilot mode route |

`availability`: `{ engine: "ok" | "not_found" | "cannot_start" | "disabled" | "unmanaged" | "none", reason?: string }`.
`none` means `engine` is null.

Apply request and answer:

```jsonc
// PUT /api/threads/{id}/profile   (and the same body, with crew_id, for the crew route)
{ "profile_id": "01j...", "seen_revision": 3 }
// 200
{
  "applied": { "engine": "claude", "model": "claude-sonnet-4-5", "permission_mode": "supervised" },
  "ignored": ["autopilot_mode"],          // fields that do not apply to this target
  "application": { "profile_id": "01j...", "name": "Careful", "revision": 3, "customized": false, "applied_at": "..." }
}
```

`seen_revision` is optional; when present and stale the route answers `409 revision_conflict` and applies nothing, so
the person sees what they are about to apply.

Errors for the family: `400 invalid_input` (field named, value never echoed), `404 not_found` (profile, thread or crew),
`403 desktop_required` (browser mode or CLI on a Person route), `403 builtin_profile`, `409 revision_conflict`,
`409 profile_limit` (50 saved), `409 name_taken`, `409 engine_unavailable`, `409 mode_not_offered`, `409 tier_unmapped`,
`409 run_active` when a thread's engine would change during a run (the model and mode alone change from the next turn).

Audit: each Person write adds a row before the effect, with ids and names only (`profile.created`, `profile.changed`,
`profile.deleted`, `profile.applied`).

Adding the routes also means, per the classes.rs rule: one row in `PERSON`, one in `ENFORCED`, one in
`web/src/api/personKey.ts`, and a row for every route in `tests/routes.json`.

## Part 2: The personal library (W9)

Three kinds of item: workflows, prompts and environment sets. One list each, all on this device, all under the
`/api/library` prefix. Nothing syncs in the beta (open question 6).

### Shared rules

- Ids are lowercase ULIDs from core. Names are 1 to 60 characters, unique per kind ignoring case.
- Limits: 200 workflows, 200 prompts, 50 environment sets, 64 keys per set. A write past a limit is `409 library_limit`.
- Every item has `revision`, `created_at`, `updated_at`. Writes carry `seen_revision` and answer
  `409 revision_conflict` when stale.
- A **parameter** is `{ name, label, kind, default?, required, choices? }`. `name` is `[a-z][a-z0-9_]{0,31}`, unique in
  the item, at most 16 per item. `kind` is `text`, `choice` (with 2 to 20 `choices`, each at most 60 characters) or
  `path` (relative to the project, never absolute, no `..`). `default` is at most 500 characters. Placeholders in the body
  are `{{name}}`; one that names no parameter is `400 invalid_input` at save.

### Workflows

```jsonc
{
  "id": "01j...", "name": "Run the tests", "description": "",       // description at most 200
  "command": "cargo test -p {{crate}} {{filter}}",                  // at most 4,000 bytes, one line or a short script
  "params": [ /* Parameter[] */ ],
  "env_set_ids": [],                                                // at most 4 environment sets, by id
  "revision": 1, "created_at": "...", "updated_at": "..."
}
```

A workflow is text and a list of parameters. Saving one runs nothing. It does **not** make a new way to run a command:

- `POST /api/library/workflows/{id}/render` fills the parameters and answers the exact command text, so the person sees
  what will run. Each value is quoted for the target shell at expansion (a value can never add a second command), a
  required parameter with no value is `400 invalid_input`, and a `choice` value outside its list is refused. The
  answer never contains an environment value.
- `POST /api/library/workflows/{id}/run` renders and starts the command in a terminal in the named project or thread
  checkout, through the same terminal machinery, folder-trust rule (hard rule 5), gate and audit as typing it there. It is
  **Person**: it acts on the person's shell. A Crew worker or any service-token caller is refused. It answers the terminal
  id. A folder that is not trusted is `403 folder_untrusted`.
- B63 workflow recipes (step lists with a dry run) are a different row and stay out of this one.

### Prompts

```jsonc
{ "id": "01j...", "name": "Explain this error", "description": "",
  "body": "Explain {{error}} and propose a fix.",        // at most 32 KiB
  "params": [ /* Parameter[] */ ], "revision": 1, "created_at": "...", "updated_at": "..." }
```

The composer inserts a prompt as text the person can still edit, then sends it as an ordinary message. A prompt never
sends itself. `POST /api/library/prompts/{id}/render` fills the parameters, the same rules as for a workflow.
A prompt body is user text: it is stored as written, and a pattern that looks like a secret in it is **not** a stored
secret. Settings › Secrets (W4) masks it where those rules apply.

### Environment sets

A set is a name and a list of key names. The values are in the keychain.

```jsonc
// EnvSet, as every read returns it
{
  "id": "01j...", "name": "Staging", "description": "",
  "keys": [ { "name": "API_BASE", "present": true }, { "name": "API_TOKEN", "present": false } ],
  "keychain": "ok",                       // "ok" | "unavailable" | "blocked"
  "revision": 4, "created_at": "...", "updated_at": "..."
}
```

- Key names: `[A-Z_][A-Z0-9_]{0,127}`. These names are refused because they change what runs or leak Orbyn's own state:
  `PATH`, `HOME`, `SHELL`, `IFS`, `LD_*`, `DYLD_*`, `NODE_OPTIONS`, `PYTHONPATH`, `BASH_ENV`, `ENV`, `ORBYN_*` and the
  engine switches the child environment already strips or sets (`CLAUDE*`, `CODEX*`, `CURSOR_API_KEY`).
- A value is 1 to 8,192 bytes, no NUL. A line break is allowed.
- `present` is `true` only when the keychain holds a non-empty value. When the keychain is unusable every key is
  `present: false` and `keychain` says why. No length, prefix or suffix is ever reported.

#### Keychain layout

Same backend and rules as `credentials` (`SecretBackend`, generic-password items, `Zeroizing` buffers), in a **separate
namespace** so no engine-key code path can reach a set and no set path can reach an engine key:

| Part | Value |
| --- | --- |
| Service | `app.orbyn.engineering.env-sets` in a signed release, `app.orbyn.engineering.env-sets.dev` in a development or unsigned build, the same split as `DEV_SERVICE` |
| Account | `<set id>/<KEY>`, one item per key |
| Data | The value's bytes |

The backend has no listing call, so the key names live in SQLite (below) and the keychain is asked per key. A new
module beside `credentials` (`credentials::env_sets`, same crate) owns it; the allowlist `ENGINES` is untouched.
The only way out of the keychain is a spawn-time function that returns `SpawnSecret` values (prints nothing in `Debug`,
zeroizes on drop) to the one caller that starts a workflow's command. Nothing else formats, logs or returns a value.

macOS only for now: other platforms answer `keychain: "unavailable"` and a value cannot be saved there (open question 3).

#### SQLite sketch

```sql
-- 61: personal library
CREATE TABLE library_workflows (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, name_key TEXT NOT NULL UNIQUE, description TEXT NOT NULL DEFAULT '',
  command TEXT NOT NULL, params_json TEXT NOT NULL, env_set_ids_json TEXT NOT NULL DEFAULT '[]',
  revision INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE library_prompts (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, name_key TEXT NOT NULL UNIQUE, description TEXT NOT NULL DEFAULT '',
  body TEXT NOT NULL, params_json TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE library_env_sets (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, name_key TEXT NOT NULL UNIQUE, description TEXT NOT NULL DEFAULT '',
  revision INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE library_env_keys (                -- names only. There is no value column, on purpose
  set_id TEXT NOT NULL REFERENCES library_env_sets(id) ON DELETE CASCADE,
  key TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
  PRIMARY KEY (set_id, key)
);
```

A test must fail if any column or table of the library can hold a value (same idea as the answer-key table that cannot
hold a brief).

#### Routes

| Method and path | Class | Purpose |
| --- | --- | --- |
| `GET /api/library` | Local read | Counts and the keychain state: `{ workflows, prompts, env_sets, keychain, limits }` |
| `GET /api/library/workflows`, `/{id}` | Local read | `Workflow[]`, one `Workflow` |
| `POST /api/library/workflows/{id}/render` | Local read | `{ values }` in, `{ command }` out. Saves and runs nothing |
| `POST /api/library/workflows` | **Person** | Create. `201` |
| `PATCH /api/library/workflows/{id}` | **Person** | Change, with `seen_revision` |
| `DELETE /api/library/workflows/{id}` | **Person** | `204` |
| `POST /api/library/workflows/{id}/run` | **Person** | `{ values, thread_id? , project_id? }` (one of the two) → `{ terminal_id }` |
| `GET /api/library/prompts`, `/{id}` | Local read | As workflows |
| `POST /api/library/prompts/{id}/render` | Local read | As workflows, answers `{ text }` |
| `POST /api/library/prompts`, `PATCH`, `DELETE /{id}` | **Person** | As workflows |
| `GET /api/library/env-sets`, `/{id}` | Local read | `EnvSet[]`, one `EnvSet`. Names and `present` only |
| `POST /api/library/env-sets` | **Person** | `{ name, description?, keys?: string[] }`. Creates the set with key names and no values. `201` |
| `PATCH /api/library/env-sets/{id}` | **Person** | Rename, describe, add or remove key names (removing a name deletes its keychain item), with `seen_revision` |
| `PUT /api/library/env-sets/{id}/keys/{key}` | **Person** | Body `{ value }`. Write-only. Answers `{ name, present: true }`, never the value |
| `DELETE /api/library/env-sets/{id}/keys/{key}` | **Person** | Removes the keychain item and keeps the name. Answers `{ name, present: false }` |
| `DELETE /api/library/env-sets/{id}` | **Person** | Removes every keychain item, then the rows. `204` |
| `GET /api/library/export?kinds=` | Local read | The export document (below). Names only for sets |
| `POST /api/library/import` | **Person** | The export document in the body; `dry_run` allowed |
| `GET /api/projects/{id}/library/suggestions` | Local read | Committed `.orbyn/` entries, read only (below) |
| `POST /api/projects/{id}/library/suggestions/copy` | **Person** | Copy one suggestion into the device library |

A **Local read** that returns env-set data answers names and `present` only. A service-token agent can therefore see that
a key exists, never what it is.

Errors for the family: `400 invalid_input` (field named, value never echoed), `404 not_found`, `403 desktop_required`,
`403 folder_untrusted`, `409 revision_conflict`, `409 name_taken`, `409 library_limit`, `409 key_refused` (a refused or
duplicate key name), `503 keychain_unavailable`, `503 keychain_blocked` (locked, denied or a cancelled prompt; retryable),
`502 keychain_failed`. A delete that cannot remove every keychain item leaves the set as it was and answers
`502 keychain_failed`, so it can be retried; rows are removed only after the items are.

Audit: every Person write adds a row before the effect (`library.workflow.created`, `library.env_set.key_set`, ...), with
ids, set names and key names, never a value, a command's text or a prompt's body.

### Redaction rules for environment values

A value is never in: SQLite, an audit row, a log line, a trace, an error, a route answer, an SSE event, an export, a
diagnostics report, a thread or Crew object, or a settings sync record. In detail:

1. It enters only through the body of `PUT /api/library/env-sets/{id}/keys/{key}`. The request body of that route is never
   logged and never echoed; an invalid body is refused naming the field only, as every route already does.
2. It is held as `Zeroizing`, formatted by nothing, and leaves the keychain only as a `SpawnSecret` at the spawn of one
   workflow run.
3. During a run, core registers the run's values as exact literals in a per-run redactor. Terminal frames and the run's
   saved output pass through it before they leave core: a match shows as `[hidden: env <KEY>]`. This is best effort. A
   command that transforms a value (base64, split, reversed) is not caught. The run card says so (open question 4).
4. A Crew worker or any service-token caller cannot start a workflow or write a set (Person routes). The Crew rule "no
   secrets in any object" is unchanged: no library value can reach a Crew object because no Crew path reads the library.
5. A set's key names are not secret but are never put in an error with a value, a path or a command.

### Reading committed `.orbyn/` files

Allowed, read only, never written (hard rule 4). Orbyn offers a repository's own entries beside the device library as
"From this repository". Rules:

- Files: `.orbyn/workflows.json`, `.orbyn/prompts.json`, `.orbyn/env-sets.json`. The shapes are the library shapes above
  without `id`, `revision` or timestamps. `env-sets.json` holds **key names only** ("this project needs `API_BASE` and
  `API_TOKEN`"); a value in it is dropped and reported as a problem, and never shown.
- Read from the project's HEAD commit through Git objects (at most 64 KiB per file), never from the working tree. So an
  edit an agent just made and not committed is not offered, and a symlink cannot point outside the repository.
- Admission is per entry: an invalid entry is dropped with a `problems[]` line (`{ file, index, code }`), the rest is
  served. A file that is too large, not JSON or unknown in version is one problem and no entries.
- Reading follows the folder-trust rule used for the project's other committed reads: untrusted is `403 folder_untrusted`.
- A suggestion is data. It is never run, rendered or inserted until the person picks it. Its command is shown in full
  before a run. Running it goes through `POST .../run` with the person's confirmation like any workflow, since a committed
  file can come from a branch the person did not write.
- `POST .../suggestions/copy` `{ file, index }` copies one entry into the **device** library (a Person write to the
  device's SQLite, subject to the library limits). It writes nothing to the repository. Copying an `env-sets.json` entry
  creates the set with its key names and every key `present: false`.
- There is no "save to repository" action anywhere. A person who wants the team to share an entry edits `.orbyn/` by hand
  and commits it as their own change.

### Import and export

- **Export** is `GET /api/library/export?kinds=profiles,workflows,prompts,env_sets`. Core writes no file. The client saves
  the answer to a place the person picks: the desktop save dialog, or a browser download in browser mode. Core never takes
  a path from the client and never writes inside a project folder.
- **Import** is `POST /api/library/import` with the document the client read from a file the person picked. Core never
  reads a path either. `dry_run: true` answers what would be created, renamed (a name clash becomes `Name (2)`), or refused,
  without writing.
- Document:

```jsonc
{
  "format": "orbyn-library", "version": 1, "exported_at": "RFC 3339",
  "profiles":  [ /* name, description, engine, model_tier, permission_mode, autopilot_mode */ ],
  "workflows": [ /* name, description, command, params, env_set_names */ ],
  "prompts":   [ /* name, description, body, params */ ],
  "env_sets":  [ { "name": "Staging", "description": "", "keys": ["API_BASE", "API_TOKEN"] } ]
}
```

- **Environment values are never exported.** A set exports its name and key names. Importing it creates the set with every
  key `present: false`, and the person fills the values in. A workflow links to sets by name (`env_set_names`), resolved on
  import; an unmatched name is dropped with a `problems[]` line.
- The document is untrusted input: at most 2 MiB, the same field limits as above, unknown fields dropped, a bad entry
  refused singly. Import creates items. It never replaces or deletes one that exists.
- A profile imported with an engine this machine lacks saves with a warning, as in Part 1.

## Open questions for the lead

1. **Built-in profiles.** Are Careful, Fast and Hands-off right as shipped presets, and is "Hands-off" = `full_access` +
   Autopilot `full` too loose to ship as a one-click preset? The proposal keeps Person-key apply as the guard.
2. **Autopilot on a plain thread.** Autopilot is per crew or repository today. The proposal ignores a profile's
   `autopilot_mode` on a thread outside Crew (answer `ignored`). Alternative: define a per-thread Autopilot. That is a
   larger change in the Autopilot group.
3. **Keychain on Windows and Linux.** `credentials` has a macOS backend only. Until others exist, a value cannot be saved
   elsewhere, so env sets work as name lists there. Is that acceptable for the beta, or does the beta wait for the backends?
4. **Output redaction is best effort.** Exact-literal masking misses a transformed value, and Settings › Secrets
   terminal masking is not built yet. Is the run-card warning enough, or should a workflow with an env set be refused
   until terminal redaction (W4) exists?
5. **Rule 1 wording.** Hard rule 1 names engine keys only. Env-set values are a second, owner-approved keychain use. The
   implementing PR must amend rule 1 and the `credentials` module doc. Confirm the separate `env-sets` service name, and the
   release service-name string, which this file proposes and does not take from existing code.
6. **Sync.** crew.md D4 and account.md list profiles, workflows, prompts and env-set names as later sync records. This
   file adds no sync. Env-set names would sync and values never; confirm that stays after the beta.
7. **Beta scope.** The Beta cut section of crew.md still says W8 and W9 are After the beta or Planned in the designs.
   Confirm they are now in the beta, and that the cut text should be updated with the implementing PR.
8. **Where a tier is resolved.** Apply resolves the tier to a model at apply time and stores the model on the thread. A
   crew resolves it per brief through the router. Confirm the crew path should keep using the router and treat the
   profile's tier as the builder default only.
9. **Env sets on engine spawns.** This contract attaches sets to workflow runs only. Attaching a set to a thread's engine
   process (for example a proxy variable) is not included; it would widen where a secret leaves the keychain.
10. **Repository entries and Autopilot.** A committed workflow from a pulled branch is data until the person runs it.
    Confirm no Crew or Autopilot path may run a library or repository workflow in the beta (the proposal: none can).
