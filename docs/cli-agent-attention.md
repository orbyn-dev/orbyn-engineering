# CLI-agent attention (W10)

What the service must tell the web so that an engine running in a terminal (an engine console) shows a Needs you item and a
notification when it stops and waits for the person. **Planned**: the web side is built hidden, the detection is not built. In a
production build the web has no client and draws nothing; a dev-only preview (`#/preview/attention`) installs the mock in
`web/mock/attention.ts`. Names here change only if the service pull request says so. It extends G14 (engine console, row G14 in
[parity](parity.md)); the product row is W10 in [crew](crew.md). Frames: `attention-*` in Engineer Designs. Owner of the detection
and the event: Eng 2.

## The one channel

No new channel. The event rides the thread's event log that already exists (`GET /api/threads/{id}/events`, `ThreadEvent
{ seq, thread_id, run_id, ts, event }`) as an AG-UI `CUSTOM` event, so ordering, replay and `seq` come for free:

```json
{ "seq": 412, "thread_id": "t_9", "run_id": null, "ts": "2026-11-22T14:12:03Z",
  "event": { "type": "CUSTOM", "name": "orbyn.terminal_attention", "value": { "v": 1, "op": "waiting", "…": "…" } } }
```

`run_id` is null: a terminal is not a run. The web ignores a `CUSTOM` event with another name, and ignores `value.v` it does not know.

## Fields

`op: "waiting"` (the engine stopped and waits):

| Field | Type | Rules |
| --- | --- | --- |
| `v` | 1 | Version. |
| `op` | `"waiting"` | |
| `id` | string | Service-made, stable for one waiting episode, never reused, at most 64 characters. Opaque: the web keeps it as a `Map` key, never as an object key, a path or a class. Re-sending `waiting` with the same `id` updates the item (a changed excerpt); it is not a new item. |
| `terminal_id` | string | The terminal in this thread (`GET /api/threads/{id}/terminals`). The thread is `thread_id` of the envelope. |
| `terminal_title` | string | The terminal's title, one line, at most 80 characters. A label only. |
| `engine` | string | Engine family: `claude`, `codex`, or another family id. Unknown ids are drawn as the id (capped at 40 characters), never as markup. |
| `kind` | `"question"` \| `"yes_no"` \| `"secret"` | `yes_no`: the prompt reads like `[y/N]`, `(y/n)`, `[Y/n]`. `secret`: the terminal is in no-echo mode (a password, passphrase or token), decided by the terminal mode and not by the words. Anything else that stopped and reads like a question or a choice is `question`. |
| `since` | string | RFC 3339, UTC. When the terminal last printed. The web shows "It stopped printing 38 s ago" from it. |
| `excerpt` | string[] | The last lines of the terminal, redacted by the service, ANSI-free, newest last. At most 6 lines, 200 characters per line, 1,000 in all (cut, not wrapped). For `secret` only the prompt's label (one line, at most 80 characters), never anything typed. |

`op: "clear"` (the item no longer applies):

| Field | Type | Rules |
| --- | --- | --- |
| `v`, `op` | 1, `"clear"` | |
| `id` | string | The `waiting` it clears. A clear for an unknown `id` is ignored. |
| `reason` | `"input"` \| `"output"` \| `"closed"` | `input`: the person typed in the terminal (the service sees a write, not the text). `output`: the terminal printed again. `closed`: the terminal ended or was closed. After a clear the `id` is not reused. |

The service never records what was typed. The cleared line says "Answered in the terminal" for `input`, "Continued" for `output` and
"The terminal closed" for `closed`.

## Detection rules (Eng 2)

A terminal is waiting when all hold: it is an engine console (G14; a plain shell is never reported), it printed nothing for the
quiet time (frames: 10 to 120 seconds, proposed 20, a setting), and its last line reads like a question or a choice, or the terminal
is in no-echo mode. A long build that is quiet is not waiting. A full-screen engine screen is not split into blocks; the excerpt
is the last lines of the screen's text.

## Limits, rate and dedupe

- One active item per terminal. A second `waiting` for a terminal with an active item updates it (same `id`) at most once every 5 s.
- A new episode on the same terminal needs a `clear` first and at least 10 s since the last `waiting`.
- At most 8 active items per thread, 32 per service; past that the service drops the oldest and sends its `clear` with `closed`.
- The web also keeps at most 64 entries, one notification per terminal per 30 s, and latest `seq` wins per `id`: a `waiting` older than
  the clear of the same `id` is ignored.
- Event size: at most 4 KiB of `value`.

## Local or Person-class

| Thing | Class |
| --- | --- |
| The event, the Needs you item, the notification, Open terminal, Dismiss | **Local**: reads of output the person's own terminal already showed, over the local connection. Clicking only focuses the terminal. |
| Answering (typing into the terminal) | **Person-class**, and only over the terminal socket that exists (`/ws/terminal/{thread}/{terminal}`, the connection that carries typing). **There is no service-side answer route.** |

The frames also show Answer here, Yes and No. They are not built: each would write to the terminal socket, so they stay
**Planned** until the answer field and its rule (what is typed is sent exactly as typed, then Enter, kept nowhere) are built and
reviewed. Nothing in this design types into a terminal on its own, and the web never answers for the person.

## Security

- **The excerpt is untrusted terminal text.** A program can print markup, escape sequences, a fake prompt or a link. The web strips
  ANSI and control characters (`stripTerminalText`), caps the lines and characters again, and draws it as text in a `pre`, never as
  HTML, never as a link.
- **The service redacts and the web does not trust that it did.** The service runs its secret rules (the Redactor, `redact-*`
  frames) over the excerpt before it sends it, through one function `redact(text) -> text`. Until it exists that hook is the identity
  and **the detection must stay off in a build**.
- For a `secret` prompt the item has Open terminal alone, and the notification and the toast say "is waiting for a password" and
  never carry the prompt's text. Orbyn takes no password, token or key here.
- A notification and a toast never carry an answer. Excerpts are not logged; a log line names the terminal id and a byte count.
- A fake prompt cannot make the web type: Open terminal focuses the terminal, nothing more.

## Settings

The frames show one device setting, Settings › General: **Terminal agents waiting for you** (on by default) and **Quiet time before it
counts as waiting** (10 to 120 s, proposed 20). They follow Thread notifications (system notification) and In-app notifications
(toast). Not built in the web yet (Planned); per-project switches and quiet hours are not in the frames.

## Web side

`web/src/features/panels/terminal/attention/`: `contract.ts` (the typed client; `attentionApi()` is null in production), `text.ts`
(cleaning, caps, copy), `store.ts` (the pure reducer and the store), `TerminalAttention.tsx` (the item), `useAttention.ts` (the
subscription, the toast and the notification), `AttentionPreview.tsx` (dev-only). Notifications use the paths that exist
(`pushToast`, and `new Notification` only when permission is already granted); no new OS permission. The mock is never in a production
bundle: `scripts/assert-no-previews.mjs` fails the build if its marker or the route appears.

## Questions for Eng 2

1. Is the event in the thread's event log (replayable, persisted) or live-only? The excerpt is redacted text; if the log persists it,
   say so and apply the retention rules. The web handles both.
2. The log is per thread. The web needs these events for every thread, to notify about one the person is not looking at. Does the
   service fan them out on one stream, or does the web follow each thread's log?
3. Quiet time, the question heuristic and the no-echo check (terminal mode) belong to the service: confirm the numbers.
