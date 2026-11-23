# Code Mode

Code Mode lets an engine write one short script that calls many Orbyn tools, instead of making one model
round trip per tool call. It is engine-neutral: it is three tools on the MCP server every engine already
uses (`/mcp/{token}`), and the script runs in a sandbox inside the Rust core. It adds no new way to do
anything. Every call a script makes goes through the same checks as a call the engine makes directly.

This file is the contract. The sandbox crate, the tools, the gate re-entry, pause and resume, and the typed
schemas are built against it, and so is the web UI (the approval card and the run log). Change it by pull
request, and say so to whoever builds on the part you change.

The design follows the progressive-discovery pattern (search, then describe, then run) and Cloudflare's
`codemode` executor shape (MIT). Nothing is copied from it yet; code adapted later keeps its notice.

## Where it runs

```
engine ──MCP──▶ orbyn_code_run(code) ──▶ run task ──▶ sandbox (QuickJS)
                                              │            │ tools.orbyn_thread_send(args)
                                              │◀───────────┘ host function (Rust)
                                              ▼
                            note_tool → Core::call_orbyn_tool(binding, name, args)
                                              │  crew-worker refusal, preview, effect gate, run
                                              ▼
                                         result or refusal ──▶ back into the script
```

The sandbox is a pure JavaScript engine with no way out. The only door is one host function, and the host
function calls the same entry point the MCP service calls
([`tools/mcp/tools/dispatch.rs`](../crates/orbyn-core/src/tools/mcp/tools/dispatch.rs), `call_orbyn_tool`).
The script never holds the MCP token or the run's `McpBinding`: the host function closes over the binding,
so a script cannot call as another run or thread.

## The three tools

All three are listed only when Code Mode is on (a `code_mode` setting per engine profile, default off, the
same way `crew_preview` gates the Crew tools). None has an effect of its own, so none asks the gate: the
effects are the inner calls.

### `orbyn_code_search`

`{ query: string, detail?: "name" | "summary" | "signature", limit?: number }` (limit 1 to 50, default 10).
Returns ranked tool paths for a short natural-language query. A path is the tool name, for example
`orbyn_thread_send`. There is no second naming scheme to learn. `detail` says how much comes back per hit:
the path only, the path with its description, or the path with its TypeScript signature. Ranking is a plain
term match over name, description and argument names; it needs no model. Hits are filtered to the tools the
caller may call (see "Who may call what"), so search never lists or describes a name the caller cannot use.

### `orbyn_code_describe`

`{ paths: string[] }` (up to 16). Returns TypeScript declarations for those tools, generated from the tool
definitions (see "Typed tool API"). An unknown or not-allowed path is listed under `unknown`, exactly as the
caller sent it, not an error: describe never adds a name the caller did not send.

### `orbyn_code_run`

`{ code: string }` starts a run. `{ run_id: string }` resumes one (see "Pause and resume"). Exactly one of
the two. `code` is JavaScript or TypeScript, at most 64 KiB, as the body of an async function: top-level
`await` works, and the script ends with `return value`. TypeScript is only stripped of its types
(`deno_ast`, no type check), so a type error never stops a run.

The reply is one JSON object:

```json
{
  "status": "completed | paused | error",
  "result": "...",
  "logs": ["..."],
  "runId": "...",
  "error": { "kind": "syntax | runtime | limit | denied | lost | cancelled", "message": "...", "line": 12 },
  "pending": [{ "tool": "orbyn_thread_send", "call": 4 }],
  "truncated": false
}
```

- `completed`: `result` is the script's return value as JSON (`undefined` becomes `null`). `error` and
  `pending` are absent.
- `paused`: the run is alive and waiting. `pending` lists what it waits on (a call held by the effect gate,
  or a call still running). `result` is absent. Call `orbyn_code_run` again with the same `runId`.
- `error`: the run ended and will not continue. `error.kind` is `syntax` (the code did not parse),
  `runtime` (a thrown error nobody caught), `limit` (a sandbox limit, see below), `denied` (the script
  stopped on a refusal it did not catch; the message is the gate's reason), `lost` (the run could not be
  resumed) or `cancelled` (the person stopped it). `logs` still holds what the script printed.
- `runId` is always present, so the person and the audit trail can name the run.

`logs` is what the script wrote with `console.log`, `console.warn` and `console.error`, one string per call.

## What a script sees

One global, `tools`. It is a proxy, not a generated function per allowed tool: any function name can be
called, and `RunHost` is the only filter. A name the caller may not use is refused when it is called, so a
script can name a tool it was not shown and still gets the refusal. Names are exactly as on the MCP server:

```ts
const status = await tools.orbyn_worktree_status({});
const results = await Promise.all([a, b].map((id) => tools.orbyn_thread_read({ thread_id: id })));
return results.filter((t) => t.status === "failed").map((t) => t.id);
```

Arguments are plain JSON. Each call returns the tool's result as a value (already parsed, not the JSON text
the MCP reply carries). A refusal or failure throws an `Error` whose `message` is the same public message
an engine would read, with `error.code` set to `"denied"` for a gate refusal and `"invalid"`, `"not_found"`
or `"failed"` otherwise. A script may catch it and go on. A denial is the person's answer: a script that
retries a denied call is making the same request again, and the gate asks again.

Nothing else is reachable: no `fetch`, no network, no filesystem, no `process` or environment, no `require`
or `import`, no `eval` or `Function` constructor, no timers, no credentials. `Date.now()` and `Math.random()`
are deterministic per run (the clock is the run's start time and the generator is seeded from the run id),
which is what makes replay after a restart possible.

## The executor trait

The sandbox lives in its own crate with no dependency on `orbyn-core` (working name `orbyn-code-sandbox`),
behind a small trait, so a different engine (`wasmtime` plus QuickJS, or `deno_core`) can replace
`rquickjs` without touching the tools or the gate. The shape is Cloudflare's `execute(code, providers) ->
{result, error, logs}`, plus yielding so a run can pause:

```rust
#[async_trait]
pub trait CodeExecutor: Send + Sync {
    /// Parses `source` (TypeScript types stripped) and prepares a run. Runs nothing.
    async fn prepare(&self, source: &str, limits: Limits, seed: RunSeed, host: Arc<dyn ToolHost>)
        -> Result<Box<dyn Run>, RunError>;
}

#[async_trait]
pub trait Run: Send {
    /// Drives the script until it finishes, fails, or `until` passes (`Step::Yielded`).
    async fn drive(&mut self, until: Instant) -> Step;
}

#[async_trait]
pub trait ToolHost: Send + Sync {
    async fn call(&self, name: &str, args: Value) -> Result<Value, ToolFailure>;
}

pub enum Step { Done { result, logs, truncated }, Failed { error, logs, truncated }, Yielded }
```

The sandbox crate knows nothing about the gate, audit or Crew. `ToolHost` is the whole interface to Orbyn.

## Sandbox limits

Defaults, all in one `Limits` value, so a test can set them small:

| Limit | Default | Over the limit |
|---|---|---|
| Heap | 64 MiB | `error.kind: "limit"`, message names the heap |
| Stack | 512 KiB | `limit` |
| Script CPU time, per run (time waiting on tool calls does not count) | 10 s | `limit`; the interrupt hook ends it, even in `while (true) {}` |
| Inner calls per run | 200 | the 201st throws `limit` |
| Calls in flight at once | 8 | the rest queue |
| Script size | 64 KiB | the call is refused before it runs |
| Logs | 500 lines, 64 KiB | the oldest is dropped, `truncated: true` |
| One inner result | 256 KiB | the call throws `limit` (it never enters the heap) |
| Run wall clock | 15 min | `limit` |

The interrupt hook is checked on a counter, not a timer thread, so a tight loop cannot outrun it. The
sandbox is dropped as soon as the run ends, whatever the end.

## How inner calls re-enter the core

For each `tools.<name>(args)` the host function does, in order, on the run task:

1. Refuses a name that is not in the callable set (below) with `invalid`, before anything else.
2. `Core::note_tool(binding, name)`: one `tool.call` audit record per inner call, so the audit trail shows
   every call, not just `orbyn_code_run`. The MCP service does this before `call_orbyn_tool` for a direct
   call; an inner call does the same.
3. `Core::call_orbyn_tool(binding, name, args)`: this is where `refuse_for_crew_worker` runs (so a Crew
   worker's script is held to the worker allow list per call), then the project preview, then the effect
   gate for a tool with an effect, then the tool. Gate decisions, rules and Autopilot review behave as for a
   direct call. A call the gate holds for a person is held as that inner tool, with the same summary
   ("Use orbyn_thread_send"), so "always allow this tool" means the tool, never "the script".
4. Applies the same per-call time as a direct call (`budget_for`: 240 s for a tool with an effect, 45 s
   otherwise), but on the run task, not under the outer call (see "Budgets").

The outer `orbyn_code_run` also writes one `code.run` audit record when a run starts: run id, script size and
SHA-256, and the thread and run it came from. The script text lives in the run record (next section), not in
the audit row.

Calls that are **not** routed through `call_orbyn_tool` today are **not** reachable from a script in this
version: `crew_*` tools (they have their own dispatch, `crew_tool_text`), `computer_*` (they drive the
screen), `skill_load`/`skill_search`, `approve`, and the three Code Mode tools themselves (no nesting). A
later change that adds one routes it through its own gate first and updates this list.

### Approvals show the script

Each pending approval created by an inner call carries a reference to its run:
`via: { code_run: runId, call: n, script_sha256 }`, a new optional field on the approval ask. The approval
card reads the script and the calls so far from the run record, so the person sees what the script is doing
around the one action they are asked about. The field is display only: rules and decisions never read it.
A script cannot ask for broader approval than its calls would need on their own.

## Run record

Every run is stored (a new table pair, `code_runs` and `code_run_calls`; the migration takes the next free
number at merge time): run id, thread, run, status, script, start and end time, and per call the index, tool
name, argument digest, status and a result preview (4 KiB) plus the full result when it is at most 256 KiB.
`waited_ms` is the total time the run waited for a person: each approval an inner call asks adds the exact
milliseconds from its creation to its decision or expiry. The sum lives on the run row, not in the call rows:
a call's `duration_ms` is its whole time and does not say how much of it was the gate, a call can ask more
than one approval, and the total must survive a pause, a resume and a replay. Migration 74 adds
`code_runs.waited_ms` and `approvals.created_at_ms`; a run that begins after it starts at `0`, and a row
stored before it stays `null`. The web UI's run log reads it. Records are kept 7 days, then deleted. The
record is what makes resume work across a restart, and the only place a script is kept.

Read access is built: see "As built: run record, routes and the switch" below.

## Pause and resume

An engine's MCP call times out at about a minute on some engines (`TOOL_BUDGET` is 45 s), but a person can
take minutes to approve. So a run does not live inside the outer call.

- `orbyn_code_run(code)` creates the run and starts it as its own task, owned by a run store, not by the
  outer call. The outer call waits until the run finishes or a **slice** ends: 40 s after the call started,
  five seconds under `TOOL_BUDGET`.
- If the run finished: `completed` or `error`. If not: `paused`, with `pending` naming the calls in flight.
  A call held by the gate stays held: the approval is awaited by the run task, so the outer call returning
  does not expire it (a dropped `within` would, which is why the run is not inside it).
- `orbyn_code_run({ run_id })` waits another slice on the same run and answers the same way.
- A run nobody resumes keeps going: reads finish, and a held call waits for its approval until the approval
  times out. The finished state is kept until the run record expires, and a later resume returns it.
- The person can cancel a live (running or paused) run. Its task is aborted, its slot is released, its pending
  approval is denied with "The run was cancelled.", and the run ends as `error: cancelled`. A cancelled run
  cannot be resumed or replayed, before or after a restart; cancelling it again returns the same record.
- The outer call's budget is `TOOL_BUDGET`. It is never `GATED_BUDGET`, because an engine that gives up at
  about 60 s would cut a 240 s call and lose the run's reply.

Only the run's own thread may resume it (the run record names the thread); another thread's id answers as
lost, live or finished, and never confirms that the id exists. A thread has at most 4 live runs, the core 16,
and Crew worker runs across all worker threads at most 8, so workers cannot take the lead's last slots.
A start over a limit is refused with the limit named, and the slot is reserved before the script is prepared
so concurrent starts cannot exceed it. The thread filter is tested by
[`crew/tests/code_mode.rs::a_run_is_resumed_by_its_thread_only`](../crates/orbyn-core/src/crew/tests/code_mode.rs);
the caps and the reservation by
[`tools/mcp/code/tests.rs::many_concurrent_starts_respect_the_thread_and_core_caps`](../crates/orbyn-core/src/tools/mcp/code/tests.rs)
and
[`tools/mcp/code/tests.rs::crew_workers_together_are_capped_and_leave_room_for_the_lead`](../crates/orbyn-core/src/tools/mcp/code/tests.rs).

### Restart and replay

A live run exists only in the memory of one core process. If the core restarts, the run is gone. A resume of
such a run returns `error: lost`, unless the engine calls `orbyn_code_run({ code, run_id })` with the same
code: the core then replays the script from the start with the recorded results. Replay rules:

- Calls are matched by index, tool name and argument digest. A mismatch ends the replay as `error: lost`
  (the script is not deterministic, or it changed).
- A call recorded as finished returns its recorded result or error and **runs nothing again**, so a side
  effect is never repeated. A recorded call without a stored full result (over 256 KiB) ends the replay
  as `lost`. Before a recorded answer is returned the allow list is checked again, so a tightened list
  refuses the call instead of answering it (tested by
  [`crew/tests/code_mode.rs::a_replay_rechecks_the_allow_list_before_answering_from_the_record`](../crates/orbyn-core/src/crew/tests/code_mode.rs)).
- A call that was in flight at the crash and has an effect ends the replay as `lost`: the core cannot know
  whether it happened. A read in flight is run again.
- Past the last recorded call the script runs live, through the gate as usual.
- A cancelled run is never replayed. A restart keeps its `error: cancelled` record, and a resume or a replay
  answers "The run was cancelled." instead of `lost`.

### Cancel

Only the person cancels, through `POST /api/code-runs/{id}/cancel` (Person-class, no body, over 1 KiB refused
with "This route takes no body."). No MCP tool, script or Crew worker reaches it. The web run block offers a
Cancel run button to the person only, hidden where no person key can be sent (a plain browser tab).

- A live run (`running` or `paused`) ends as `error: cancelled` with "The run was cancelled." Its task is
  aborted and its slot is released before the route answers, so a new start may take the place at the cap.
- Every approval the run has pending is denied through the same store path a person's denial takes, with the
  reason "The run was cancelled." and the person as the decider. It is gone from the approval queue.
- The record keeps its calls so far. A call whose effect was already in flight at the abort may still finish
  that effect, but its call row is recorded `failed` with the cancel's reason. The route waits briefly for the
  task to drop, then fails every call still `running`: after it answers, no call is left `running` and no new
  call can record itself. A late answer from a call or a late task finish cannot change the record back to
  `running` or `completed`.
- Cancelling twice returns the same record. A run that already ended (completed or error), including a lost
  run, is not live: the answer is `409 run_finished` with "This run already ended." An unknown id is `404
  not_found`, as for a read. A cancelled run is never resumed or replayed; both answer "The run was
  cancelled.", and that stays true after a restart.
- The cancel writes one `code_run.cancel` audit row with the person as actor.

## Budgets

| Layer | Limit | Rule |
|---|---|---|
| Engine MCP idle timeout | about 60 s on some engines | an outer call never needs more than a slice |
| Outer `orbyn_code_run` call | `TOOL_BUDGET` 45 s, slice 40 s | then `paused` |
| One inner call with an effect | `GATED_BUDGET` 240 s | as a direct call; the gate wait lives in the run task |
| One inner call without an effect | `TOOL_BUDGET` 45 s | as a direct call; the call throws `failed` |
| Script CPU | 10 s | `limit` |
| Run wall clock | 15 min | `limit` |
| Reply size | 256 KiB (`MAX_RESULT_BYTES`) | see below |

A reply over 256 KiB is cut the way every tool reply is. To keep it valid JSON, the core trims in this
order until it fits: `logs` from the oldest line, then `result` is replaced by
`{ "truncated": true, "bytes": N, "preview": "<first 64 KiB as text>" }` and `truncated` is `true`. A script
that needs a large result should reduce it in the script, which is the point of Code Mode.

## Who may call what

The callable set for a run is computed per call, from the binding:

- **Lead and ordinary threads:** the `orbyn_*` tools (the same set `tools/list` shows) except the ones listed under "Not in a script".
- **Crew worker:** only what `crew::workers::allowed` permits: its preview tabs, worktree and environment
  tools, its own attachments, and the three Code Mode tools. Each of the three is one explicit entry in the
  list, never a prefix rule. The list is checked when the worker calls the tool and again on every inner call
  by `refuse_for_crew_worker` inside `call_orbyn_tool`, so a script cannot reach past it by naming a tool it
  was not shown. `orbyn_code_search` and `orbyn_code_describe` never list or declare a tool the worker may
  not call, and their descriptions, signatures and declarations never name one (a name found in a string says
  "another tool"). That is the search-and-describe claim only: `tools/list` is separate, unchanged, and not
  filtered by the worker allow list, so it still shows a worker names it cannot call. Claim A (per call,
  including a name never listed) is tested by
  [`crew/tests/code_mode.rs::a_workers_script_is_held_to_the_allow_list_on_every_call`](../crates/orbyn-core/src/crew/tests/code_mode.rs);
  claim B (search and describe) by
  [`crew/tests/code_mode.rs::a_workers_search_lists_only_tools_it_may_call`](../crates/orbyn-core/src/crew/tests/code_mode.rs)
  and
  [`crew/tests/code_mode.rs::a_workers_describe_declares_only_tools_it_may_call`](../crates/orbyn-core/src/crew/tests/code_mode.rs).
  The redaction of nested strings is tested by
  [`tools/mcp/code/tests.rs::redaction_covers_values_but_not_keys_and_matches_whole_names`](../crates/orbyn-core/src/tools/mcp/code/tests.rs).
  Nesting and the attachment own-thread scope are covered by
  [`crew/tests/code_mode.rs::a_workers_script_cannot_nest_code_mode_tools`](../crates/orbyn-core/src/crew/tests/code_mode.rs)
  and
  [`crew/tests/code_mode.rs::an_attachment_tool_in_a_workers_script_stays_on_its_own_thread`](../crates/orbyn-core/src/crew/tests/code_mode.rs).
- **Not in a script, ever:** `approve`, the three Code Mode tools, and anything that reads or answers the
  approval queue as the person.
- **Not in a script in v1 (a v2 item):** `crew_*`, `computer_*`, `skill_load` and `skill_search`. Each
  needs its own gate path first.

A tool added later is refused to a worker by default; the existing exceptions are the `orbyn_preview_*`
prefix, the bus tools and `crew_finding`. A worker's direct calls are filtered by `crew::workers::ALLOWED`
and those exceptions; a script's inner calls are filtered by the host table (`RunHost`) to the same
`tools/list` tools, then by that worker list on every call. These are two lists, not none: a tool reaches a
worker only by being named in the worker list, and reaches a script only by being in the host table and
admitted by the worker list.

## Typed tool API

The tool table in [`definitions.rs`](../crates/orbyn-core/src/tools/mcp/tools/definitions.rs) gives each
argument only a name and a flat type, and no description. Code Mode needs more, so the table grows (per-argument
descriptions, enums, nested objects and arrays) and both the MCP `inputSchema` and the declarations come
from the same data:

- `tool_specs()` (the Orbyn tools) and `crew::tool_specs()` already produce JSON Schema. A small converter
  turns an `inputSchema` into a TypeScript declaration: `object` to an interface with doc comments from
  `description`, `enum` to a union, `array` to `T[]`, `required` to optional marks.
- `orbyn_code_describe` returns declarations such as:

```ts
/** Send a message to another thread in this project. */
declare function orbyn_thread_send(args: {
  /** The thread to message. */
  thread_id: string;
  /** The message text. */
  text: string;
}): Promise<unknown>;
```

- Results are typed `Promise<unknown>` until a tool has an output schema; a tool that gets one returns its
  declared type. The declarations never promise a shape the tool does not return.

A test checks that every Orbyn tool in `tool_specs()` has a declaration and that a declaration's required
arguments match its `inputSchema`, so the two cannot drift.

## Error texts

`error.message` is shown to the person as is. Limits: "The script used more than its CPU time.", "The script
used more than its memory limit (heap).", "The script used more than its stack limit.", "A script may make 200
calls.", "The run took longer than its time limit.", "The script is over 65536 bytes." `denied` is the gate's
own reason, verbatim (a timed-out approval is a refusal with the gate's timeout reason). `lost`: "This run
was lost when Orbyn restarted. Call orbyn_code_run again with the same code and run_id to replay it.", and
for a failed replay "The script changed or is not repeatable, so the recorded calls can't be replayed." or "A
call that may have changed something was running when Orbyn restarted, so the run can't continue." A
cancelled run ends, resumes and replays with "The run was cancelled."; cancelling a run that already ended
answers `409 run_finished` with "This run already ended." The run record and the runs list API carry
`waited_ms`; the list view itself shows run id, status, start and end, call count and the first 120
characters of the script, and the detail view adds the calls with their 4 KiB previews. A call's full
result is fetched on its own.

## Switches and what is not here yet

- One `code_mode` setting per engine profile (default off) controls whether the three tools are listed. A
  simple task bypasses nothing in the core; the engine decides when to call Code Mode, so the setting is
  the only switch. Codex also has its own code mode that sees this server as `mcp__orbyn_engineering__*`; both work
  and neither depends on the other.
- External MCP servers (so a script can call them too) are a later phase: they will be proxied through this
  server, so their calls pass the same gate. Until then a script reaches only Orbyn's tools.
- Saving a script as a skill needs the person's approval each time and is not part of the first version.

## Invariants

1. A script holds no token, path, environment variable or handle; there is no network or file access.
2. An inner call naming a tool outside the script table is refused before anything else. A live inner call
   in the table is written by `note_tool`, then held to `refuse_for_crew_worker`, then the effect gate, in
   that order; a direct call is recorded first the same way. A Crew worker's script cannot call a tool the
   worker cannot call directly.
3. A refusal is never turned into an allow: a denied inner call throws, and nothing in the run retries it
   behind the person's back.
4. An approval created by an inner call is answered by the person for that tool and arguments, and its
   decision does not change the rules for the script as a whole.
5. A limit ends the run as `error: limit`; it never hangs the core, and a dropped outer call never expires a
   held approval.
6. Replay never repeats a recorded effect, and re-checks the allow list before answering from the record.
7. A run is readable and resumable only from its own thread.
8. Only the person cancels. A cancel ends a live run with the fixed reason, denies its pending approval through the
   person's path, stops its task and frees its slot; a late call answer or task finish cannot make it live, completed
   or resumable again, and a cancelled run is never replayed.

The tests named in this document cover the worker, replay and search/describe claims; the sandbox crate's
tests cover the capability limits.

## Build order

1. This document.
2. The sandbox crate: `rquickjs` with the limits above, an interrupt hook, TypeScript stripped with
   `deno_ast`, the executor trait, tests for each limit and for every capability that must be missing.
3. The three tools on the MCP server and the run store, with a fake executor in tests.
4. Gate re-entry and audit (`note_tool` per inner call, the `code.run` record, the approval `via`
   field) and the Crew worker allow list.
5. Pause and resume, the run record, and replay.
6. The richer tool definitions and the declaration generator.

Each step is its own pull request with its tests. The web UI (approval card with the script, run log) is built
by the web side against the shapes here.

## As built: run record, routes and the switch

Tables `code_runs` and `code_run_calls` (migration 63). Behind `CoreConfig::code_mode` (`ORBYN_CODE_MODE=1`): with it off every
route below answers `404`.

- **Recording.** A run's row is written before its task starts (id, thread, script, SHA-256, the seed and start time a replay needs,
  whether the thread is a Crew worker's). Each inner call writes a `running` row with its index, tool, argument digest (SHA-256 of
  the arguments' JSON, never the arguments) and whether the tool has an effect, then ends as `ok`, `failed` or `denied` with the first
  4 KiB of the result and its size; the whole result is kept when it is at most 256 KiB, as `{ "code", "message" }` for a failure. The
  end of the run keeps the result (whole up to 256 KiB, else `{ truncated, bytes, preview }`), the newest 500 log lines and the error.
  `paused` is stored when an outer call returns before the end, `running` again when a call resumes it. A start refused for a limit
  leaves no record. `waited_ms` starts at `0`; every approval an inner call asks adds its creation-to-decision or
  creation-to-expiry milliseconds to it (migration 74, `approvals.created_at_ms`), and an old row stays `null`.
- **Restart.** At start the core marks every run that was `running` or `paused` as `error` with kind `lost` and the lost message; its
  calls stay as they were, so a call that was running shows as `running`. A cancelled run keeps its `error: cancelled` record, and a
  resume or replay of it still answers "The run was cancelled." Records older than 7 days are deleted at start and on every new run.
- **Replay.** `orbyn_code_run({ code, run_id })` replays a run of this thread that ended as `lost` when the code's SHA-256 equals the
  record's. The run keeps its id, seed and start time, `resumes` goes up by one, and the rules above under "Restart and replay" hold:
  calls match by index, tool and argument digest; a finished call answers from the record and runs nothing; a mismatch, a missing
  stored answer, or an effect call that was running ends the run as `lost` with the doc's texts, and once a replay has failed every
  later call of the script is refused. A read that was running is run again; past the last recorded call the script runs live through
  the gate. Replay of a run that did not end as lost, or of another thread's run, is refused as lost (the id of another thread's run is
  never confirmed); a cancelled run answers "The run was cancelled." instead and is never replayed. A resume with `run_id` alone of a
  finished run that left memory answers from its record.
- **Routes.** Reads are `Local`; a run id is a key and only ids of letters, digits, `_` and `-` up to 64 characters reach the store.

| Route | Person | Answers |
|---|---|---|
| `GET /api/code-runs/{id}` | no | `CodeRunRecord` (`404` when there is none); `result` over 64 KiB is `{ truncated, bytes, preview }`; `pending` is the calls still running |
| `GET /api/threads/{id}/code-runs` | no | `{ runs: CodeRunSummary[] }`, newest first, at most 100; `script_head` is the first 120 characters |
| `GET /api/engines/{id}/code-mode` | no | `{ enabled }`; off until a person turns it on |
| `PUT /api/engines/{id}/code-mode` | **yes** | `{ enabled }`, what is stored; in `ENFORCED` and `PERSON_ROUTES` |
| `POST /api/code-runs/{id}/cancel` | **yes** | no body (over 1 KiB is `400 invalid_input`); the cancelled `CodeRunRecord`; `409 run_finished` for an ended run, `404 not_found` for an unknown id; in `ENFORCED` and `PERSON_ROUTES` |

- **The switch.** `code_mode:<engine id>` in the settings table, `on` or `off`. The three tools are listed and callable only when the
  service flag is on and the calling engine's switch is on. `waited_ms` is the recorded total, `0` for a run that never waited and
  `null` for a row stored before migration 74. `resumes` counts both resumes after a pause and replays. Run statuses are `running`,
  `paused`, `completed` and `error`; the error kinds are the six above. Types: `CodeRunRecord`, `CodeRunSummary`, `CodeRunList`,
  `CodeModeSetting` in `orbyn-contracts`.
- **Approval `via`.** `Approval.via` is `{ code_run, call, script_sha256 }` or `null`, stored as JSON in `approvals.via` (migration 68).
  The run host sets it from a task-local around each inner call (`governance::approvals::with_via`), and the gate copies it onto the
  approval it creates; `call` is the call's index in the run, `script_sha256` the hash of the script that is running. The card reads the
  script and the calls from the run record by `code_run`. No rule, decision or audit row reads it; a script's call asks exactly what a
  direct call asks. The wire type is `ApprovalVia` in `orbyn-contracts`.
- **Cancel.** `Core::code_run_cancel` ends a live run as `error: cancelled` with "The run was cancelled.", denies its pending
  approvals with the person's own denial path and reason, aborts the script task, releases its slot under the run-store lock, fails
  every call still running, and writes one `code_run.cancel` audit row. It also covers a start still in `prepare`: the reservation is
  marked, the script is never spawned, and the slot is released. The route awaits the aborted task (at most 2 s) and then fails the
  calls left, so no call is still `running` when it answers. The record is written before the task is released, and the
  store only ends a `running` or `paused` run and a `running` call, so a late answer cannot flip either back. The task also records
  the cancel before a finish already in progress can write `completed` (unless the finish's own write already won; then cancel
  answers `409 run_finished`). Cancelling a cancelled run returns its record; a completed,
  errored or lost run is `409 run_finished`; an unknown id is `404 not_found`. The route is `POST /api/code-runs/{id}/cancel`, a
  body-less Person-class write in `ENFORCED`, `PERSON_ROUTES`, `routes.json` and `web/src/api/personKey.ts`.
- **Cancel in the web.** The run block draws the person's Cancel run button; no other web screen draws a stop, so the runs list
  has no row action.

### Tested against the real service

`crates/orbynd/tests/code_mode_switch.rs` drives the switch and read routes over HTTP against a real
`orbynd`: `the_switch_is_off_by_default_for_a_known_engine_and_an_unknown_one`,
`the_switch_writes_what_it_stored_and_survives_a_restart`, `a_refused_switch_write_is_not_stored`,
`an_extra_field_is_ignored_by_the_switch_write`,
`a_shell_less_service_answers_desktop_required_and_writes_nothing`,
`with_code_mode_off_every_client_route_is_not_offered`,
`a_thread_with_no_runs_and_an_unknown_thread_both_answer_an_empty_list`,
`unknown_and_malformed_run_ids_are_404_not_500` and
`a_real_run_is_listed_and_read_with_the_contract_shape`. The last creates a run through the
service's own `/mcp/{token}` endpoint and its real QuickJS executor. The read routes check an id's
shape, not what it names: an unknown engine id with a valid shape answers `200 {"enabled":false}`
and an unknown thread id answers `200 {"runs":[]}`. The `PUT` body is not strict: serde ignores an
extra field, so that write succeeds; a missing or non-bool `enabled` is `400 invalid_input`, as is a
body over the router's 4 MiB limit.

## As built: web UI

The web side is in `web/src/features/codemode/`, built from the Engineer frames `codemode-*`. It reads the service's own routes (see
above) through a small client (`api.ts`) with the generated `CodeRunRecord`, `CodeRunSummary`, `CodeRunList` and `CodeModeSetting`
types; `model.ts` is the one place that reads them. With the service flag off every route answers `404`, and the client reads that as
"Code Mode is not offered": the setting keeps its Planned pill and a switch that cannot be pressed, the runs list draws nothing, a run
card draws from the engine's own reply, and nothing shows an error. The dev mock (`web/mock/codemode.ts`, scenario `codemode`, and
`mock.codeMode.setOffered(false)` for the 404 case) serves the same shapes for `#/preview` and tests only; it is not in a production build.

- **Setting.** Settings › Providers › an engine has a Code Mode group with one switch, off by default, read when the page opens and
  written once per click. The Planned pill goes once the service has answered the read; on a `404` it stays.
- **Run card.** An `orbyn_code_run` call in the thread is drawn from its run: running, paused, completed, and the six error kinds, with the
  script excerpt (5 lines, opened a 400-line page at a time), calls (3 and a "Show all" line past 8, at most 200), logs (newest 500),
  and the result (first 64 KiB). The run record is read every two seconds while the run is not over; without it only the engine's reply
  is drawn, and there is no call list. A resumed run is one record shown on each call that names it.
- **Approval.** An approval with `via` (the service sends it for approvals a script's inner call asked) shows the call, the script and the calls so far, "1 of N" with the next ones held to one line,
  the answered and expired forms, and what Always allow covers. `via` is read only to draw: the buttons, the decision and the
  rules never read it.
  The card has an "Open the run" button (drawn only for a run id of letters, digits, `_` and `-`, up to 64): it goes to the approval's thread (`#/threads/<id>`, no new route), scrolls to the run's block (`id="cm-run-<run id>"`) and focuses it, without smooth scroll under reduced motion; when the block is not in the transcript it says so and does nothing else.
- **Reading the record.** A call's `status` on the wire is `running`, `ok`, `failed` or `denied`; there is no "waiting". The card reads a
  `running` call of a `paused` run that `pending` names as waiting for a person, and a `running` call of a run that ended as `error` (a
  restart) as interrupted. `result`, `logs_total` and `waited_ms` are read as the contract has them (`{ truncated, bytes, preview }`,
  a count when the record kept fewer lines, `null` for a row stored before waits were recorded), and an error `kind` it does not
  know is drawn as a runtime error. `cancelled` is one of the kinds it knows; the card and row say "Cancelled".
- **Search and describe.** Drawn as rows in the thread, from the reply read leniently (the exact reply shapes are not in this document).
- **Untrusted text.** The script, logs, results and call previews are React text, never markup or links, never logged, never opened.
- **Lost runs.** The run card shows the core's `lost` message as it is, with no text of its own around it, and the calls the record
  holds. The row note is "lost", and the footer says "Lost · 4 calls recorded", or "Lost · 4 calls done, 1 unknown" when a call was
  running at the restart (that call reads "Running when Orbyn restarted" and is never counted as done). The three frames (restart,
  a replay that could not match, a call in flight) differ only in the message and the calls.
- **Runs list.** `CodeRunsList` draws the per-thread list (id, status, times, call count, the first 120 characters of the script,
  "Runs are kept for 7 days") from `GET /api/threads/{id}/code-runs` (`CodeRunList`) and opens a run as the same
  block the thread draws. It sits under the Tools tab, not in a tab of its own, because where it opens is not settled. The service
  has the Person cancel route; this list draws no Stop yet. An unknown status is shown as Unknown and cannot be opened.
- **Not built.** The Tools tab for a Crew worker ("allowed for this worker") has no data behind it; the three tools are marked Planned in
  the Orbyn group when an engine reports them.
- **The switch write.** `PUT /api/engines/{id}/code-mode` changes what an engine may call, so it is a Person-class write
  (`web/src/api/personKey.ts`, already listed with the route). One click is one write.
- **Model-chosen text.** Run ids, tool names and statuses come from a model: records are kept in maps, an unknown status is shown as
  unknown, the tool in "Always allow covers X" comes from the approval's own tool field and never from its summary, and arguments
  are summarised (lists, objects, cut text with an ellipsis, "N more fields") so a card never looks complete when it is not.
  Every call of a run keeps its Allow and Deny; only the first shows the script.
