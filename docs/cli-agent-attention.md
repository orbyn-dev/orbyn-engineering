# CLI-agent attention (W10)

What the service must tell the web so that an engine running in a terminal (an engine console) shows a Needs you item and a
notification when it stops and waits for the person. **Built in the service**: detection, redaction, the event on the thread's
log and the open-items read. **The web side is built and observed against a real service** (`web/e2e/real/terminal-attention.spec.ts`):
the window follows a thread's log after the shell stream's `thread_event` notice and raises the toast, reads the thread's current open
items once when it starts for threads with an open terminal and once before a notice's follow, and draws the item, the silence strip,
the tab and row dots and the two Settings rows; a dev-only preview
(`#/preview/attention`) installs the mock in `web/mock/attention.ts`. Names here change only if the service pull request says so.
It extends G14 (engine console, row G14 in [parity](parity.md)); the product row is W10 in [crew](crew.md). Frames: `attention-*` in
Engineer Designs. Owner of the detection and the event: Eng 2.

## The one channel

No new channel. The event rides the thread's event log that already exists (`GET /api/threads/{id}/events`, `ThreadEvent
{ seq, thread_id, run_id, ts, event }`) as an AG-UI `CUSTOM` event, so ordering, replay and `seq` come for free:

```json
{ "seq": 412, "thread_id": "t_9", "run_id": null, "ts": "2026-11-22T14:12:03Z",
  "event": { "type": "CUSTOM", "name": "orbyn.terminal_attention", "value": { "v": 1, "op": "waiting", "…": "…" } } }
```

`run_id` is null: a terminal is not a run. The web ignores a `CUSTOM` event with another name, and ignores `value.v` it does not know.

No new stream carries the wait. When the service appends one of these events it publishes a small
`ShellEvent::ThreadEvent { thread_id, seq }` on the existing `GET /api/shell/events` stream, with no event payload. A window that
cares starts following that thread's log from `seq`; threads that never announced anything are not followed. The event itself is
only in the thread's log.

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

## Open items read

`GET /api/threads/{id}/terminal-attention` (Local) answers `{ "items": [...] }`: the items waiting now in that thread, each shaped
exactly as the `waiting` value above, oldest first, at most 8 (`TerminalAttentionOpen`). A window that opens after an item began reads
it once, then follows the log as before. It reads the service's in-memory registry only: the stored, already redacted excerpt (a
`secret` item carries only its label), never the terminal's text, and it writes nothing. Cleared and closed items are gone; an
unknown thread is a 404.

The web reads it once when the attention client starts, for the threads that already have a terminal open in this window (and once
more for a thread whose terminal opens later), and once before it begins following a thread from a `thread_event` notice. The wait is
bound: it reads threads the window knows have an open terminal plus threads that announce a notice, at most 32 threads
(`MAX_FOLLOWED`), in small batches, and never polls. Each answer feeds the same store path as a log event: latest per `id`, the excerpt
cleaned, capped, ANSI-stripped and drawn as text; a `secret` keeps only its label. A read item was already waiting before the window
opened, so it raises no toast and no system notification, while the amber terminal-tab and sidebar dots and the thread's Needs you item
still show it. A read that fails or answers 404 draws nothing and says nothing; the log follow still starts. Items beyond 8 and unknown
shapes are ignored.

## Detection rules (built)

A terminal is waiting when all hold: it is an engine console (G14; a plain shell is never reported), it printed nothing for the
quiet time (10 to 120 seconds, 20 by default, a setting), and its last line reads like a question or a choice, or the terminal
is in no-echo mode. A long build that is quiet is not waiting. A full-screen engine screen is not split into blocks; the excerpt
is the last lines of the screen's text.

- The heuristic reads the last non-empty line after ANSI and control characters are stripped. `[y/N]`, `(y/n)`, `[yes/no]` and
  `(yes/no)`, case-insensitively, are `yes_no`; a trailing `?`, a trailing `:` after prompt-like words (`Password:`, `Enter … :`,
  `Choose …`), or a numbered or arrow-marked choice list under such a header is `question`.
- The no-echo check reads the pty master's termios `ECHO` flag with `tcgetattr` on unix. Windows ConPTY has no such mode, so
  `secret` is never reported there. `kind: "secret"` is decided by the mode, never by the words.
- `input` comes from `PtySession::write` alone, a count with no bytes kept. `output` is any new pty output. `closed` is the output
  ending or the terminal being closed.

## Limits, rate and dedupe

- One active item per terminal. A second `waiting` for a terminal with an active item updates it (same `id`) at most once every 5 s.
- A new episode on the same terminal needs a `clear` first and at least 10 s since the last `waiting`.
- At most 8 active items per thread, 32 per service; past that the service drops the oldest and sends its `clear` with `closed`.
- IDs are service-made ULIDs: opaque, at most 64 characters, and never reused. A terminal that opens again starts a new episode with
  a new id.
- The web also keeps at most 64 entries, one notification per terminal per 30 s, and latest `seq` wins per `id`: a `waiting` older than
  the clear of the same `id` is ignored.
- Event size: at most 4 KiB of `value`; the service cuts the oldest excerpt lines until the serialized value is under it.

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
  frames) over the excerpt before it appends it, through `Core::redactor(Surface::Terminal)`. The hook exists now, so the detector
  runs only while `terminal_attention_enabled` is true and a redactor was obtained; an event whose redactor cannot be obtained is
  not sent. Redact first, then cut: a cut line can never leave the front of a hidden value behind.
- For a `secret` prompt the item has Open terminal and Dismiss (Dismiss only hides it on this device) and no answer field, and the notification and the toast say "is waiting for a password" and
  never carry the prompt's text. Orbyn takes no password, token or key here.
- A notification and a toast never carry an answer. Excerpts are not logged; a log line names the terminal id and a byte count.
  Nothing in this feature writes to a terminal, and no service-side answer route is added.
- A fake prompt cannot make the web type: Open terminal focuses the terminal, nothing more.

## Settings

The frames show one device setting, Settings › General: **Terminal agents waiting for you** (on by default) and **Quiet time before it
counts as waiting** (10 to 120 s, 20 by default). They follow Thread notifications (system notification) and In-app notifications
(toast). The service reads two stored device settings: `terminal_attention_enabled` (bool, default `true`) and
`terminal_attention_quiet_seconds` (whole seconds, 10 to 120, default `20`). Settings › General has both rows in the web (the switch, and a number field kept to 10 to 120 and saved when it loses focus); per-project switches and quiet hours are not in the frames.

## Web side

`web/src/features/panels/terminal/attention/`: `contract.ts` (the types and `attentionApi()`, the service client unless a test or the preview installed another), `client.ts` (the service client: a `thread_event` notice from `GET /api/shell/events` starts following that thread's log from `seq - 1`, at most 32 threads, oldest let go first; before that follow, and once when the client starts for each thread with a terminal open in this window, it reads the thread's open items through `api.terminalAttention` (also once for a terminal that opens later) and feeds them through the same store path without a toast; reads only; `since` is the RFC 3339 string, parsed in `store.ts`), `text.ts`
(cleaning, caps, copy), `store.ts` (the pure reducer and the store), `TerminalAttention.tsx` (the item), `useAttention.ts` (the
subscription, the toast and the notification), the silence strip under the terminal drawer's tab row (`attention/SilenceStrip.tsx`: the
frame's amber dot and `Waiting for you · <engine> has been silent for <n> s` for the waiting terminal on screen, no action and no
excerpt, gone when the event clears or the item is dismissed), the amber dot on the terminal tab (`TerminalDrawerBody.tsx`) and on the
thread's sidebar row (`ThreadRow.tsx`, with the word Terminal in place of its age or run status; the dot alone in the Files view) while
a terminal of the thread waits and is not dismissed here, `AttentionPreview.tsx` (dev-only). Notifications use the paths that exist
(`pushToast`, and `new Notification` only when permission is already granted); no new OS permission. The mock is never in a production
bundle: `scripts/assert-no-previews.mjs` fails the build if its marker or the route appears. Observed against a real `orbynd` in
`web/e2e/real/terminal-attention.spec.ts`: a fake engine console prints a question and goes quiet, the notice makes the window follow
the thread's log and raise the toast for a thread that is not on screen, and a later window's open-items read is seen on the wire,
drawn without a notice, and the silence strip draws it for the console on screen.

## Decisions

1. **The event is in the thread's event log.** It is persisted, replayable and kept under the same retention as the thread's other
   events. The excerpt is redacted before the append, through `Core::redactor(Surface::Terminal)`; an event whose redactor cannot be
   obtained is not sent. The serialized `value` stays under 4 KiB.
2. **The log stays per thread; no new stream.** `GET /api/shell/events` carries a small `thread_event` notice (thread id and `seq`, no
   payload) whenever one of these events is appended. The web follows the log only for threads that announced something.
3. **The numbers are these.** Quiet time is 20 s by default, settable from 10 to 120 whole seconds
   (`terminal_attention_quiet_seconds`); detection is on by default (`terminal_attention_enabled`). A second waiting for an active
   terminal updates the same id at most once every 5 s. A new episode needs a clear first and 10 s since the last waiting. The caps
   are 8 active items per thread and 32 per service, dropping the oldest with `closed`. IDs are service-made, at most 64 characters
   and never reused. The heuristic and the no-echo check are in the service; unix reads termios `ECHO` with `tcgetattr`, and Windows
   ConPTY has no such mode, so `secret` is never reported there.
