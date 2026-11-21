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
caller may call (see "Who may call what"), so search never leaks a name the caller cannot use.

### `orbyn_code_describe`

`{ paths: string[] }` (up to 16). Returns TypeScript declarations for those tools, generated from the tool
definitions (see "Typed tool API"). An unknown or not-allowed path is listed under `unknown`, not an error.

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
  "error": { "kind": "syntax | runtime | limit | denied | lost", "message": "...", "line": 12 },
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
  stopped on a refusal it did not catch; the message is the gate's reason) or `lost` (the run could not be
  resumed). `logs` still holds what the script printed.
- `runId` is always present, so the person and the audit trail can name the run.

`logs` is what the script wrote with `console.log`, `console.warn` and `console.error`, one string per call.

## What a script sees

One global, `tools`, with one async function per allowed tool, named exactly as on the MCP server:

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
    /// Parses `source` (TypeScript already stripped) and prepares a run. Runs nothing.
    fn prepare(&self, source: &str, limits: &Limits, seed: &RunSeed) -> Result<Box<dyn Run>, SyntaxError>;
}

#[async_trait]
pub trait Run: Send {
    /// Drives the script until it finishes, fails, or `until` passes. A host call is awaited through `host`.
    async fn drive(&mut self, host: &dyn ToolHost, until: Instant) -> Step;
}

#[async_trait]
pub trait ToolHost: Send + Sync {
    async fn call(&self, name: &str, args: Value) -> Result<Value, ToolFailure>;
}

pub enum Step { Done { result: Value, logs: Vec<String> }, Failed { error: RunError, logs: Vec<String> }, Yielded }
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
screen), `skill_load`, `approve`, and the three Code Mode tools themselves (no nesting). A later change that
adds one routes it through its own gate first and updates this list.

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
The web UI's run log reads it. Records are kept 7 days, then deleted. The record is what makes resume work
across a restart, and the only place a script is kept.

Read access (shape settled with the UI in the PR that adds it): `code_run.get(runId)` and a per-thread list,
both limited to the caller's own project.

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
- The outer call's budget is `TOOL_BUDGET`. It is never `GATED_BUDGET`, because an engine that gives up at
  about 60 s would cut a 240 s call and lose the run's reply.

Only the run's own thread may resume it (the run record names the thread). A thread has at most 4 live
runs and the core 16; a start over the limit is refused with the limit named.

### Restart and replay

A live run exists only in the memory of one core process. If the core restarts, the run is gone. A resume of
such a run returns `error: lost`, unless the engine calls `orbyn_code_run({ code, run_id })` with the same
code: the core then replays the script from the start with the recorded results. Replay rules:

- Calls are matched by index, tool name and argument digest. A mismatch ends the replay as `error: lost`
  (the script is not deterministic, or it changed).
- A call recorded as finished returns its recorded result or error and **runs nothing again**, so a side
  effect is never repeated. A recorded call without a stored full result (over 256 KiB) ends the replay
  as `lost`.
- A call that was in flight at the crash and has an effect ends the replay as `lost`: the core cannot know
  whether it happened. A read in flight is run again.
- Past the last recorded call the script runs live, through the gate as usual.

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
  tools, and its own attachments. `orbyn_code_search` and `orbyn_code_describe` list only those, so a worker
  never reads the name of a tool it cannot call. The three Code Mode tools are added to the worker allow
  list on purpose, one by one in the same change that adds them, with a test per tool. Every inner call is
  checked again by `refuse_for_crew_worker`, so a script cannot reach past the list by calling a name it
  was not shown.
- **Not in a script, ever:** `approve`, the three Code Mode tools, and anything that reads or answers the
  approval queue as the person.

A tool added later is refused to a worker by default, as today, and reaches a script only through the same
table `tools/list` reads. There is no second list to forget.

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

A test checks that every tool in `tools/list` has a declaration and that a declaration's required arguments
match its `inputSchema`, so the two cannot drift.

## Switches and what is not here yet

- One `code_mode` setting per engine profile (default off) controls whether the three tools are listed. A
  simple task bypasses nothing in the core; the engine decides when to call Code Mode, so the setting is
  the only switch. Codex also has its own code mode that sees this server as `mcp__orbyn_engineering__*`; both work
  and neither depends on the other.
- External MCP servers (so a script can call them too) are a later phase: they will be proxied through this
  server, so their calls pass the same gate. Until then a script reaches only Orbyn's tools.
- Saving a script as a skill needs the person's approval each time and is not part of the first version.

## Invariants (each has a test)

1. A script holds no token, path, environment variable or handle; there is no network or file access.
2. Every inner call passes `refuse_for_crew_worker`, the effect gate and `note_tool`, in that order, as a
   direct call does. A Crew worker's script cannot call a tool the worker cannot call directly.
3. A refusal is never turned into an allow: a denied inner call throws, and nothing in the run retries it
   behind the person's back.
4. An approval created by an inner call is answered by the person for that tool and arguments, and its
   decision does not change the rules for the script as a whole.
5. A limit ends the run as `error: limit`; it never hangs the core, and a dropped outer call never expires a
   held approval.
6. Replay never repeats a recorded effect.
7. A run is readable and resumable only from its own thread.

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
