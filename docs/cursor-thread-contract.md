# Cursor thread contract

What a thread on an engine that never asks before acting (Cursor) needs from the service. **Planned**: the Rust side is the engine
host's HostDriver slice 3 and is not merged. The web side is `web/src/features/cursorthread/contract.ts`; the differences are at the
end. A name here changes only if the service PR says so.

## One mode

A Cursor thread has one mode, **No approvals (Cursor)**. `EngineCapabilities.permission_modes` (orbyn-contracts `engines.rs`) lists the
modes an engine offers; for Cursor it holds only `["no_approvals"]`. The mode picker reads that list and never names an engine. Every
other engine keeps today's rule: a saved mode outside its list runs Supervised and is shown that way.

## Refusal (HTTP 409, at run start only)

Saving a thread with Supervised or Auto edits is allowed. Starting a run on a Cursor thread whose saved or default mode is outside the
list is refused, on every route that starts a run (send, goal and command starts). Body: `{code, message, ...}`; the screens act on
`code` and show `message`.

| `code` | When | Extra fields |
| --- | --- | --- |
| `engine_mode_refused` | the thread's mode is Supervised or Auto edits | `engine`, `thread_id`, `mode`, `allowed` |
| `engine_notice_required` | the project has not acknowledged the notice for this engine | `engine`, `project_id` |

If both apply, the notice comes first.

## One-time notice (once per project)

Stored per project and engine, not per person or per engine alone. Both routes are on the project, local only:

- `GET /api/projects/{project_id}/engines/{engine}/notice` answers `{engine, project_id, acknowledged, acknowledged_at}`. An engine with no
  notice answers 404.
- `POST /api/projects/{project_id}/engines/{engine}/notice/acknowledge`, empty body, **Person-class** (the key goes with it), answers the
  same object and writes one audit row. Acknowledging twice changes nothing.

## Run without approvals

`POST /api/threads/{id}/send-without-approvals`, **Person-class**, local only, same body as send. Runs exactly one turn with No approvals
and stores nothing: the thread's saved mode stays, so the next send is refused again. It writes an audit row. It does not skip the
notice: an unacknowledged project still answers `engine_notice_required`.

## Crew

`EngineStatus` gains `crew_role: "any" | "reviewer_only"` and `crew_role_reason: string | null`. Cursor is `reviewer_only` with the reason
"Not yet confined for writing work" until the Cursor confinement probe passes under the host prefix; only then does it become `any`.

## Differences from `contract.ts`

- The notice moves from `/api/engines/{engine}/notice` to the project routes above, and `NoticeState` gains `project_id`.
  `notice(engine)` and `acknowledge(engine)` therefore take a project id too.
- `engine_notice_required` carries `project_id`.
- Everything else (`no_approvals`, `engine_mode_refused` and its fields, `crew_role`, `crew_role_reason`, `send-without-approvals`) matches.
- Move `send-without-approvals` and `notice/acknowledge` from `PENDING_PERSON_ROUTES` to `PERSON_ROUTES` in `web/src/api/personKey.ts`
  in the same change that lands the routes.
