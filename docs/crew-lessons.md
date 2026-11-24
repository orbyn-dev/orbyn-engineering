# Crew lessons: the contract (KC5, KC9)

Ideas from Kiro Crew's docs, reviewed for ideas, nothing copied. Device only, behind Crew Preview. This is the contract the web
client builds against; behaviour rules are in `docs/crew.md` ("Kiro Crew features"). Routes answer `404 not_found` when Crew
Preview is off, so a client draws nothing then.

## A lesson

One short plain sentence the crew keeps about one repository (or all of them). It is **data in a brief, never a rule**: it cannot
touch the router, the verifier, an answer key, a lease, a budget or a check.

```json
{
  "id": "les_...",
  "scope": "repo",            // "repo" | "global"
  "repo": "<project id>",     // null when scope is "global"
  "source": "fingerprint",    // "fingerprint" | "escape" | "revert" | "person"
  "text": "Checks like repo_required failed on test files before: ...",   // 1 to 300 characters
  "state": "active",          // "active" | "disabled"
  "hidden_by_cap": false,     // over the cap (50 per repo, 20 global); readable under the Hidden filter
  "in_next_brief": true,      // would go into the next brief: active, in the newest 5 of at most 1500 characters, switch on, no skill
  "edited_by_person": false,
  "names_path": false,        // text names a repo-relative path (repo scope only; promoting such a lesson is refused until edited)
  "uses": 3,                  // briefs that carried it
  "left_out_of": 0,           // briefs it was left out of because the key check failed ("Left out of 1 brief")
  "skill": null,              // the skill's name while a skill made from it exists ("Now a skill"); then in_next_brief is false
  "evidence": [ { "kind": "task", "id": "..." } ],   // at most 5, ids only
  "created_at": "...", "updated_at": "...", "last_used_at": null,
  "rev": 4                    // integer, +1 on every change; writes send it back
}
```

Evidence is ids only, never content: `{ "kind": "task" | "escape" | "merge", "id": "..." }`.

## Routes

All under `/api/crew/lessons`. Reads are **Local**; every write is **Person** (also `ENFORCED`; browser mode answers `403
desktop_required`), audited in the same transaction as the change (`crew.lesson.*`).

| Method and path | Class | Body | Answer |
|---|---|---|---|
| `GET /api/crew/lessons?repo=<project id>&filter=active\|disabled\|hidden\|all` | Local | none | `200 LessonList` |
| `GET /api/crew/lessons/{id}?repo=<project id>` | Local | none | `200 LessonDetail` |
| `POST /api/crew/lessons` | Person | `{ scope, repo?, text }` | `201 Lesson` (add by hand; `source: "person"`) |
| `PATCH /api/crew/lessons/{id}` | Person | `{ text, rev }` | `200 Lesson` (sets `edited_by_person`, re-runs the scan) |
| `POST /api/crew/lessons/{id}/disable` and `/enable` | Person | `{ rev }` | `200 Lesson` |
| `DELETE /api/crew/lessons/{id}?rev=N` | Person | none | `200 { id, undo_until }` (final after 10 s) |
| `POST /api/crew/lessons/{id}/undo` | Person | none | `200 Lesson` while before `undo_until`, else `404 not_found` |
| `POST /api/crew/lessons/{id}/promote` | Person | `{ rev }` | `200 Lesson` (repo to global; only a person promotes) |
| `POST /api/crew/lessons/{id}/demote` | Person | `{ repo, rev }` | `200 Lesson` (global back to one repository) |
| `PUT /api/crew/lessons/settings` | Person | `{ repo, use_lessons }` | `200 { repo, use_lessons }` |

`LessonList`:

```json
{
  "repo": "<project id>",
  "use_lessons": true,                       // the per-repository switch, on by default under Preview
  "brief_cap": { "lessons": 5, "chars": 1500 },
  "counts": { "active": 4, "disabled": 1, "hidden": 0 },
  "lessons": [ /* Lesson, repo lessons then global, newest first; "hidden" filter lists over-cap ones */ ]
}
```

`LessonDetail`: `{ "lesson": Lesson, "evidence": [ { "kind", "id" } ] }`; its `evidence` equals the lesson's own. A global lesson's
`in_next_brief` depends on a repository's switch and cap, so `repo` is required for a global lesson and the value describes that
repository; a repository lesson ignores it. Without it a global lesson answers `in_next_brief: false`.

Rules a client can rely on:

- The switch off stops injection of repository and global lessons for that repository; learning continues; add by hand still works.
  `in_next_brief` is false for every lesson of it.
- Delete is a 10 s Undo and then final; the deleted lesson's fingerprint is remembered so it is not rewritten.
- Nothing promotes by itself. A new lesson from a source is written `active`; at cap the oldest unused one becomes `hidden_by_cap`,
  never deleted.
- Lessons are never synced and are not in any export.

## Errors

JSON `{ "code": "...", "message": "..." }` like every route.

| Status and code | When |
|---|---|
| `400 invalid` | empty text, over 300 characters, missing `repo` for scope `repo`, a path named in a global lesson, promote of a lesson that names a path |
| `400 secret_refused` | the scan found a secret; message is exactly `Not saved: this looks like it holds a secret. Edit it and save again.` |
| `404 not_found` | unknown lesson or repository, Preview off, undo window over |
| `409 stale` | `rev` is not the lesson's current one |
| `403 desktop_required` | a Person route called without the person key |

## Skill proposals (KC9)

A proposal is made by code when 3 or more active lessons of one repository share the same `source`, failing check kind and path
class. The text comes from a fixed template over those lessons' own text; no model, nothing leaves the device. A proposal blocks
nothing and does not feed the needs-you field. Under `/api/crew/skill-proposals`:

A debug build started with `ORBYN_DEV_SEED_PROPOSALS=1` also serves the dev-only seam `POST /api/crew/dev/seed-proposal`; release builds and debug runs without that flag have no such route.

| Method and path | Class | Body | Answer |
|---|---|---|---|
| `GET /api/crew/skill-proposals?repo=<project id>` | Local | none | `200 { proposals: [SkillProposal] }` (every state, newest first) |
| `POST /api/crew/skill-proposals/{id}/approve` | Person | `{ text, rev }` | `200 SkillProposal` (writes the skill; Eng 4 owns where) |
| `POST /api/crew/skill-proposals/{id}/dismiss` | Person | `{ rev }` | `200 SkillProposal` |

```json
{
  "id": "prop_...", "repo": "<project id>",
  "state": "open",                 // "open" | "approved" | "dismissed"; every state is read
  "text": "...",                   // the template text; the person edits it before Approve
  "lessons": [ { "id": "les_...", "evidence": [ { "kind": "task", "id": "..." } ] } ],
  "can_approve": true,             // false when a source lesson was deleted or disabled; the proposal is kept
  "skill": null,                   // { "name": "..." } once approved
  "rev": 1, "created_at": "...", "approved_at": null
}
```

An approved local skill applies to all projects on this device, not only the repository whose lessons proposed it.

Errors: `409 stale` for a wrong `rev`, `409 not_open` for approve or dismiss of a proposal that is not open, `409 name_taken`
when the skill name is held by the shared library or an existing local skill, `400 invalid` for empty approved text or text
over 600 characters or `can_approve: false`, `400 secret_refused` as above. An approved proposal marks its source lessons
"Now a skill" and they leave briefs while that skill exists; if the skill is removed they return to normal.

## EvalHistory samples (KC5 slice 4)

Each task records exactly one sample (migration 88): its brief's kind, whether it carried lessons and which lesson ids it used and left out, then its outcome, stop reason, latest routed engine and finish time when it ends. Device only, never synced or exported, ids and enums only. Nothing reads a sample yet: routing, `score`, `EvalHistory`, the brief, checks and leases are unchanged.

## KC8: resumed after restart

On the Flow task row (`FlowTaskRow`): `resumed: boolean` (the task resumed once after a restart) and the existing `round: number`,
the attempt now running (a resume counts the lost turn as used); "resumed from round N" is `round - 1`. Audit rows `crew.task.resumed` and `crew.task.restart_stopped`. A worker
stopped after its one resume keeps stop reason `engine_error`; its `crew.task.restart_stopped` row says `why: already_resumed`, and a typed `stop_detail: "worker_stopped_twice"` names the second death.
Migration 86 adds that column without rebuilding `crew_tasks` (26 foreign keys point at it), so every old reader still sees `engine_error`.
