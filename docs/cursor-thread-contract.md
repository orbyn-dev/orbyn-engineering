# Cursor thread contract

What a thread on an engine that never asks before acting (Cursor) needs from the service. **Built** in HostDriver slice 3: the Rust
side (`orbyn-contracts` `engines.rs` and `threads.rs`, `orbyn-core` `agents/engines/notice.rs` and `conversation/runs/admission.rs`,
`orbynd` `routes/runs.rs` and `routes/engines.rs`). The web side is `web/src/features/cursorthread/contract.ts`; the differences are at
the end.

It all applies only while Cursor runs on the engine host (`ORBYN_ENGINE_HOST_CURSOR=1`, with `ORBYN_ENGINE_HOST` set, the same pattern
as `ORBYN_ENGINE_HOST_CLAUDE=1`). With the flag off Cursor is the ACP engine it was: every mode, no notice, no refusal, no
`reviewer_only`. No real Cursor turn has run against any of this: there is no key and no terms decision yet, and the tests use a
stand-in engine.

## One mode

A Cursor thread has one mode, **No approvals (Cursor)**, spelled `no_approvals` on the wire (`PermissionMode::NoApprovals`).
`EngineCapabilities.permission_modes` (orbyn-contracts `engines.rs`) lists the modes an engine offers; for Cursor on the host it holds
only `["no_approvals"]`. `PermissionMode::ALL` stays the four old modes: an engine offers `no_approvals` only by naming it. The mode
picker reads that list and never names an engine. Every other engine keeps today's rule: a saved mode outside its list runs Supervised
and is shown that way. Cursor runs `no_approvals` whatever the thread saved (once past the refusal below); Orbyn's own gates treat it
like Supervised, so nothing is loosened. Choosing `no_approvals` is the loosest change, so it takes the person key where raising a
mode does (`POST /api/threads/{id}/runs`, `PATCH /api/threads/{id}`).

## Refusal (HTTP 409, at run start only)

Saving a thread with Supervised or Auto edits is allowed. Starting a run on a Cursor thread whose saved or default mode is Supervised
or Auto edits is refused, on every path that starts a run: send, goal and command starts (`POST /api/threads/{id}/runs`, the AG-UI
route), a queued message (at queueing, and again when it would start; a refused one stays queued) and a preparation retry. Auto and
Full access promise no ask, so they are not refused; the engine runs them as `no_approvals`. Body: `{code, message, ...}` with the
extra fields beside `code` and `message` (no `detail`); the screens act on `code` and show `message`, a plain sentence.

| `code` | When | Extra fields |
| --- | --- | --- |
| `engine_mode_refused` | the thread's mode is Supervised or Auto edits | `engine`, `thread_id`, `mode`, `allowed` |
| `engine_notice_required` | the project has not acknowledged the notice for this engine | `engine`, `project_id` |

`mode` is the saved mode (`supervised` or `auto_edits`); `allowed` is the engine's `permission_modes`. If both apply, the notice comes
first. The notice and the mode refusal read the engine's own `permission_modes`, so any engine that offers `no_approvals` gets them;
only Cursor has a notice.

## One-time notice (once per project)

Stored per project and engine, not per person or per engine alone, as the setting `engine_notice:{engine}:{project_id}` holding the
time it was acknowledged. Nothing secret is in it. Both routes are on the project, local only (a remote service answers
`403 local_only` on the write):

- `GET /api/projects/{project_id}/engines/{engine}/notice` answers `{engine, project_id, acknowledged, acknowledged_at}`. An engine with no
  notice answers 404.
- `POST /api/projects/{project_id}/engines/{engine}/notice/acknowledge`, empty body, **Person-class** (the key goes with it), answers the
  same object and writes one audit row (`engine_notice.acknowledge`, target the engine, detail `{project_id}`). Acknowledging twice
changes nothing and writes nothing. An unknown project also answers 404.

## Run without approvals

`POST /api/threads/{id}/send-without-approvals`, **Person-class**, local only, same body as send. Runs exactly one turn with No approvals
and stores nothing: the thread's saved mode stays, so the next send is refused again. It answers `202` with the run, as a send does. A
`permission_mode` in the body is refused (`400`), because it would be stored; model and effort apply as on a send. It writes an audit
row (`thread.send_without_approvals`, target the thread, detail `{engine, project_id}`) after the notice check and before the run. It
does not skip the notice: an unacknowledged project still answers `engine_notice_required`, with no audit row. On an engine that asks
it answers `422 unsupported`.

## Crew

`EngineStatus` gains `crew_role: "any" | "reviewer_only"` and `crew_role_reason: string | null`. Cursor on the host is `reviewer_only` with the
reason "Not yet confined for writing work" until the Cursor confinement probe passes under the host prefix; only then does it become
`any`. Every other engine is `any` with a `null` reason (both fields are always sent). The router refuses a Cursor writer (a brief
that writes, in a role that is not a reviewer) with `missing_capability` and the sentence "Cursor is reviewer only: Not yet confined
for writing work."; a reviewer lane may use it. A writer pinned to Cursor is refused, never swapped.

## Key

`CURSOR_API_KEY` reaches the Cursor host only through `Core::engine_spawn_secret("cursor")` into the host's spawn environment, after
the person's opt-in. It is never in argv, a frame, a log, an error or a route answer; the notice routes carry no secret.

## Differences from `contract.ts`

- The notice moves from `/api/engines/{engine}/notice` to the project routes above, and `NoticeState` gains `project_id`.
  `notice(engine)` and `acknowledge(engine)` therefore take a project id too.
- `engine_notice_required` carries `project_id`.
- Everything else (`no_approvals`, `engine_mode_refused` and its fields, `crew_role`, `crew_role_reason`, `send-without-approvals`) matches.
  `crew_role` and `crew_role_reason` are optional in the generated TypeScript (so older fixtures still type-check) and always sent.
- `engine_mode_refused` is sent for Supervised and Auto edits only, as `contract.ts` assumes (`mode` is one of the two).
- The generated types are `EngineNotice` (the notice object), `CrewRole` and `PermissionMode` with `no_approvals`. `NoticeState` and the
  hand-written refusal types in `contract.ts` can be replaced by them or kept: the JSON is the same.
- `send-without-approvals` and `notice/acknowledge` moved from `PENDING_PERSON_ROUTES` to `PERSON_ROUTES` in `web/src/api/personKey.ts`,
  and are rows in `crates/orbynd/tests/routes.json` and `routes/classes.rs`. `GET .../notice` is a `local` row.
