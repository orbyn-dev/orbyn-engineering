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
- **Delivery.** A running worker is steered when its engine takes input mid-turn (`runs.steer`), else the
  message is queued for its next turn. The record says `live` only when the text was handed to the running
  turn's input; a real engine may read it after the tool call it is in (#444). A worker that is not in a turn
  (verifying, waiting for a person) has the message `held`, so it cannot start a turn on files the verifier
  is reading; it goes out with the next retry prompt (#447). The lead is always queued, never steered.
- The lead is told once per distinct refusal (hop, post limit, board conflict), by a message from the core
  stored and queued like any other. The worker that keeps knocking does not fill the lead's queue.
- `all` means every other member whose run is live. The lead reads its worker's messages to `lead`.
- A worker sees the brief, messages addressed to it or to `all` by somebody else, and the board. It never
  sees a message to the lead or to another member, and nothing of the lead's thread.
- An answer returned to a waiting `crew_ask` is not also sent to the asker's turn.

## Tested

`tests/bus.rs` runs two fake engines, one that takes input mid-turn and one that does not: a board fact
crosses between running turns, the hop limit trips and resets, an ask goes pending or returns the answer,
and nothing is taken after the run ends. `tests/bus_guards.rs` covers stamped fields, secrets, the answer key,
sizes, the post limit, replies, the board's rules, held messages and the allow list; the count is also tested
as a pure rule.

## Limits of Preview

- Limits come from `Bus::lower`; no rule file is read yet (#445).
- Fake engines only. No real Claude or Codex run has shown when a steered message is read (#444).
