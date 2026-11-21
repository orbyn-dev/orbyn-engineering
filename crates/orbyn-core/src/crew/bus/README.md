# Crew bus

Messages between members, the shared board and the guards of
[SPEC.md](../../../../../docs/crew/protocol/SPEC.md) (H4 to H7, H12). Behind the Crew Preview flag with
the rest of [crew](../README.md): off, the four tools do not exist.

## What it owns

| Module | Owns |
| --- | --- |
| `mod` | The four tools (`crew_post`, `crew_ask`, `crew_board_read`, `crew_board_write`), their specs, the limits and the in-memory bus state (one writer at a time, the `crew_ask` wake-up, the questions waited on) |
| `post` | `crew_post`: the guards in the spec's order, stamp, store, deliver |
| `wait` | `crew_ask`: post a question, wait up to 40 seconds, return the answer or `pending` |
| `board` | `crew_board_read` and `crew_board_write`: one standing revision per key, stale writes refused (H12) |
| `deliver` | The data wrapper, the secret and leak checks on the way out, steering or queueing, and the delivery record |
| `notify` | Telling the lead, once, when a guard refuses a member |
| `rules` | What the guards count: exchanges between two members, who a message is for, the answer to a question. Pure |
| `config` | The lead repository's committed `bus` limits, trusted caller/project binding and read-only default-branch pin check |

## Rules to keep

- The core stamps author, id, run and revision (H9). A draft that names one is refused. A message
  is stored under the run of its sender; the lead's message to a member under that member's run, and
  its message to `all` under the newest run that is live.
- Order of refusals is the spec's: the run (H7), the post limit (H5), the shape, the secret scan (H2), then
  the answer-key leak (H1), the reply check (H6) and the hop limit (H4). A message to `lead` is not checked
  against the key; everything else is, and so is every fact.
- Member tools refuse once the run ends, and so does a message to a member whose run has ended. The core's
  own writes (`add_crew_object`) do not: Evidence and Gates can still be added. A member's object is
  stored by `add_crew_member_object`, which checks the run inside the same transaction.
- **Hop limit.** Three messages between two members, counted both ways. To `lead` or `all` does not count.
  A lead message to a member starts the count again for every pair that member is in; to `all` or to a third
  member it resets nothing. This is H4 as written; narrowing it to the pairs the lead was told about
  is a protocol decision (#446). Counted from the stored messages in order, so a restart loses nothing.
- **Delivery.** The bus itself never starts a turn and never writes a thread's queue (the queue and a plain steer
  show text as the person's own). It hands text only to a turn that is running and takes input
  (`runs.steer_live`), and the timeline shows that under the role `crew`. Otherwise the message is `held`. It
  reaches the recipient in one of two ways:
  - in the `inbox` of the recipient's next bus tool result (`tool_result`), or
  - in guarded engine context at every worker turn launch, including a `needs_person` → `running` resume.
    The person's queued prompt and `user_prompt` remain unchanged; the bus starts no turn. Pending message IDs
    are excluded from the initial brief so the same launch does not include them twice. Delivered history stays.
  - inside the lead's existing result notice. Its held text stays inside the `<crew-message trust="data">` wrapper.
  At most 10 messages and 20,000 characters go in one prompt or inbox, with a line saying how many more are held. A
  worker message is recorded as sent only after the driver accepts the turn, the run becomes running, and current
  cancellation/task-state checks pass. Queue acceptance alone acknowledges nothing. Failed starts release their
  claims and leave messages held. Pending snapshots and claims are selected atomically; a launch and early MCP
  inbox call cannot collect the same pending IDs. No claim mutex guard crosses the driver await.
  A worker whose guard keys cannot be read receives no held body. When a task ends, the lead's base result notice
  lists at most ten of that worker's undelivered message IDs, its recipient and total/remaining count. It forwards
  no bodies and writes no worker-delivery receipt; the metadata survives fallback without lead-held message bodies.
  `live` means handed to the running turn's input; a real engine may read it after the tool call it is in (#444).
- A held or live text is checked for secrets and the answer key again when it leaves, against the keys as they are
  then; so is a board read by a worker and an answer returned to a waiting `crew_ask`. A text a key now covers is
  replaced by a plain notice. A worker's first prompt leaves out board facts and messages a key covers.
- **Posts.** A board write counts as a post (H5), and a key has at most 16 revisions, so rewriting a key is no way
  round the limits.
- **Repository limits.** Protected `.orbyn/crew.json` may contain `"bus":{"exchanges":1,"posts":10}`.
  Both fields are optional nonnegative integers; zero refuses the first counted action. They can only
  tighten the current host ceilings (normally 3 exchanges and 30 posts). Unknown fields and higher
  limits are ignored with fixed diagnostics that quote no repository values. A present null, wrong
  type, repeated recognized field or malformed JSON is refused. An absent file or section adds no limit.
  Start refuses a malformed bus section before admitting a task. Existing whole-document, class and
  default-branch failures keep the router's `needs_person` refusal and audit; no worker is launched.
  Each post, ask's post and board write then captures
  the lead checkout's current committed HEAD and accepted default-branch rules, using the stricter
  values. Git's bounded plain-blob reader rejects links, submodules, oversized files and unreadable
  policy; the remembered default-branch ancestry check never falls back past a damaged or missing pin.
  Bus reads do not advance that pin. No worktree file or caller-supplied project/path is read as policy.
  Worker and lead project bindings are checked again after Git reads.
  A committed change affects a later operation, including an active task; uncommitted changes do not.
  Removing a tighter rule may restore the host ceiling. Counts never reset on refresh or restart,
  and lowering below the stored count refuses the next counted action. Restart rereads the same rules;
  there is no migration or frozen policy record. Git reads precede the bus writer lock; the host ceiling
  is applied again under that lock. Malformed policy refuses writes, while inbox/board reads and
  cancellation remain available. These per-operation local Git reads add work to each bus mutation.
- The lead is told once per distinct refusal (hop, post limit, board conflict; a conflict by key and writer, not by
  the revisions picked), and of at most 10 things a run, by a message from the core
  stored and queued like any other. The worker that keeps knocking does not fill the lead's queue.
- `all` means every other member whose run is live. The lead reads its worker's messages to `lead`.
- A worker sees the brief, messages addressed to it or to `all` by somebody else, and the board. It never
  sees a message to the lead or to another member, and nothing of the lead's thread.
- An answer returned to a waiting `crew_ask` is not also sent to the asker's turn.

## The ledger

A lead message to one member restarts the exchange count of every pair that member is in (H4, kept as written). A routine message can
therefore re-arm a looping pair the lead never looked at (option A). Every such reset is a `hop_reset` row in `crew_bus_events` (migration 36,
append-only, written by the core only, `ledger.rs`): the member, the message id, and each pair with its count before the reset. It holds no
message text, and passes the secret scan and the answer-key check before it is stored. Real runs show whether resets re-arm loops; a notice
id that the lead must name (option B) is revisited then (#446).

## Tested

`tests/bus.rs` runs two fake engines, one that takes input mid-turn and one that does not: a board fact
crosses between running turns, the hop limit trips and resets, an ask goes pending or returns the answer,
and nothing is taken after the run ends. `tests/bus_guards.rs` covers stamped fields, secrets, the answer key,
sizes, the post limit, replies, the board's rules, held messages and the allow list; the count is also tested
as a pure rule.

## Limits of Preview

- `Bus::lower` is a process-wide host ceiling only. Repository limits apply to their own caller's crew and never mutate it.
- Fake engines only. No real Claude or Codex run has shown when a steered message is read (#444).
