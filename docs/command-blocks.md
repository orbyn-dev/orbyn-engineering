# Command blocks (W1)

What the terminal drawer needs from the service to show one block per command. **Planned**: the web side is built hidden, the
service routes are not built. In a production build the web has no client and sends nothing, so the terminal is unchanged; a dev-only
preview (`#/preview/command-blocks`) installs the mock in `web/mock/commandBlocks.ts`. Names here change only if the service pull
request says so. Row: G17 in [parity](parity.md). Frames: `block-*` in Engineer Designs. Owner of the service side: Eng 2.

## What the service keeps

Per terminal (a thread's `main` shell or a named terminal), an ordered list of blocks, oldest first. A block starts at an OSC 133 `C`
mark (output starts) and ends at the `D` mark (or at the next `A` or `C` mark when no `D` arrives, with no exit code). The marks are
the ones the web already reads in `web/src/features/panels/terminal/shellMarks.ts`; Orbyn installs no shell integration, so a shell
that emits none yields no blocks. A command Orbyn itself runs (a project script, a Crew check, an agent command) is one block with
the same fields.

| Field | Type | Rules |
| --- | --- | --- |
| `id` | string | Service-made, unique within the terminal, never reused. Opaque to the web, which keeps it as a map key, never as an object key or a path. |
| `command` | string or null | The command line, one line, redacted by the service's secret rules, at most 4096 characters (cut, not wrapped). Null when the shell sent no line. |
| `started_at` | string | RFC 3339, UTC. When output started (the `C` mark). |
| `ended_at` | string or null | When the `D` mark arrived. Null while the command runs. |
| `exit_code` | integer or null | From OSC 133 `D;<code>`. Null while running, or when the shell sent no code. |
| `cwd` | string or null | The working directory at the `C` mark when the shell reports it (OSC 7 or 633 `P;Cwd=`). Shown as a label only; the web never opens it. |
| `output_bytes` | integer | Bytes of output kept for the block (after redaction and the caps below). |
| `truncated` | boolean | True when bytes were dropped from the top of this block's output because of a cap. |
| `dropped_lines` | integer | Lines dropped from the top; 0 unless `truncated`. |
| `fullscreen` | boolean | True when the program used the alternate screen. Its output is not kept: `output_bytes` is 0 and the output route answers an empty page. The block shows "Full-screen program, output not kept". |

The service keeps the output as text, not as the raw byte stream: the bytes are decoded as UTF-8 (invalid sequences replaced), and the
service removes ANSI escape sequences (CSI, OSC, DCS and other ESC sequences) and every control character except newline and tab
before storing. A carriage return that redraws a line (a progress bar) keeps the last text of that line. The web strips again
before it draws or copies, so a service that forgets still cannot put an escape into a clipboard.

### Limits

| Limit | Value | Behavior past it |
| --- | --- | --- |
| Blocks per terminal | 500 (the web's `ShellMarks` keeps 500) | The oldest block is dropped whole. |
| Bytes per block | 1 MiB of text | The top of the output is dropped; `truncated` is true and `dropped_lines` counts the lines lost. |
| Bytes per terminal | 8 MiB of block text, in step with the 8 MiB scrollback that exists today | The oldest blocks are dropped whole until the total fits. A running block is never dropped. |

Eng 2 may propose other numbers with a measurement; the web reads `limits` from the list answer and shows what the service says, it does
not hard-code them.

### Retention

- Blocks live as long as the terminal does. Closing a terminal, or settling or archiving the thread, drops its blocks with it.
- Blocks are held in memory by the service for the first version, like the scrollback; a restart of `orbynd` clears them. Persisting
  them is a separate decision that needs the redaction rules in place first (output may hold secrets).
- A block still running when the terminal closes ends there with no exit code.

## Routes

Both are under the thread, like the terminal routes that exist (`GET, POST /api/threads/:id/terminals`). `main` names the thread's
main shell. The id in the path is a thread the person can open and a terminal that belongs to it; anything else is `404`.

| Route | Class | Answer |
| --- | --- | --- |
| `GET /api/threads/{id}/terminals/{terminal}/blocks?before=<cursor>&limit=<n>` | Local read | `{ blocks, next_before, limits }` |
| `GET /api/threads/{id}/terminals/{terminal}/blocks/{block_id}/output?after=<cursor>&limit=<bytes>` | Local read | `{ text, next, total_lines, dropped_lines }` |

Neither is Person-class: each reads output the person's own shell already showed in the drawer, over the local connection, the way the
terminal output stream does. Neither writes. There is **no** service-side run, rerun or delete route in this design.

- **List.** Newest page first when `before` is absent: the last `limit` blocks (default 50, at most 200), returned oldest first.
  `next_before` is an opaque cursor for the page before, or null at the start. `limits` carries `max_blocks`, `max_block_bytes` and
  `max_total_bytes` so the web can say what is kept. A running block is always in the newest page, with `ended_at` null.
- **Output.** The block's text, oldest first. `after` is the opaque `next` of the previous page; `limit` is in bytes (default 64 KiB,
  at most 256 KiB), and a page never cuts a UTF-8 character or a line in two when a newline is near. `next` is null on the last page.
  `total_lines` is the line count of all kept text, or null for a running block whose end is not known. `dropped_lines` repeats the
  block's count so one answer is enough to say "Showing the last 6 of 1,248 lines".
- A running block's output grows. The web asks again about every 2 s while a block runs, and the service may answer from its memory
  without waiting. A cursor from before the growth stays valid: it is an offset into the kept text, and dropping the top of a block
  moves the offset, so the service answers `410 cursor_expired` for a cursor into dropped text and the web starts again from the top.
- Errors use the usual `{ code, message }` body: `404 not_found` for a thread, terminal or block that is gone (a dropped block is
  gone), `410 cursor_expired`, `422 bad_cursor`. A thread whose terminal has no marks answers an empty list, not an error.

## Actions

| Action | Where it runs | Rule |
| --- | --- | --- |
| Copy output | The web | Reads every page of the block's output (up to the web's cap), strips ANSI again, puts text on the clipboard. Never sends it anywhere. |
| Copy command | The web | The block's `command`, as text. Disabled when it is null. |
| Run again | The web, over the terminal socket | **A click writes the command text into the terminal's input and nothing more.** No newline is added, so the shell shows the line and waits; the person presses Enter in the terminal. It is the same write path the terminal already has (`/ws/terminal/{thread}/{terminal}`, the Person-class connection that carries typing). It never calls a service route, and the service never runs a block's command on its own. A command that is empty, longer than 4096 characters or holds a newline or control character is not written. |
| Stop | The web, over the terminal socket | Writes Ctrl-C, as the drawer's Stop does today. |

The service cannot tell a rerun from typing, which is the point: the line is the person's to run.

Not in this version (listed in the [parity row](parity.md)): Send to agent, Ask Crew to fix, Filter, Find, Run again in a new tab, Clear
all blocks, and the full-screen frame while the program runs. Clear all would need a delete route; that is a decision for Eng 2 and
Products Research, not something the web invents.

## Security

- **Output is untrusted terminal text.** A program can print anything: markup, escape sequences, a fake prompt, a link. The web draws
  it as text in a `pre`, never as HTML, never as a link, never evaluates it. It strips escape sequences and control characters before
  drawing and before copying, and caps what it keeps in memory (see `web/src/features/panels/terminal/blocks/blockText.ts`).
- **The service redacts, the web does not trust that it did.** The service runs its secret rules (the Redactor, per the `redact-*`
  frames) over both `command` and the output text **before** it stores them and again before it answers, so a secret printed by a
  command is never in the list or the output. The hook is a single function in the block store, `redact(text) -> text`, so the rule set
  can change without touching the routes. Until the Redactor exists the hook is the identity and the routes stay behind the hidden
  flag: **do not turn blocks on in a build before redaction lands**.
- Redaction applies to what the web copies too: the web copies what the service sent. A copy therefore never holds more than the drawer
  showed.
- A block's `command` and `cwd` are labels. The web never opens a path from them, never builds a URL from them and never puts them in a
  log line. Nothing here is logged with its text; a log line names the terminal id, the block id and a byte count.
- The cursors are opaque and signed or bounded by the service; the web never builds one.
- Rerun is a write the person triggers by a click and finishes with the Enter key; nothing in this design can run a command without both.

## Web side

`web/src/features/panels/terminal/blocks/`: `blocksContract.ts` (the typed client interface, `blocksApi()` is null in production),
`blockText.ts` (cleaning, caps, durations), `CommandBlocks.tsx` and `BlockCard.tsx` (the drawer's blocks), `CommandBlocksPreview.tsx`
(the dev-only screen). The mock, `web/mock/commandBlocks.ts`, is never in a production bundle: `scripts/assert-no-previews.mjs` fails the
build if its marker or the route appears.

## Questions for Eng 2

1. Route names and paths above (`/api/threads/{id}/terminals/{terminal}/blocks…`): confirm or rename.
2. Are `max_blocks`, `max_block_bytes` and `max_total_bytes` right, and is "in memory, cleared by a restart" acceptable for the first version?
3. Does `orbynd` see OSC 133 marks, or only the web's xterm? Today only the web reads them (`shellMarks.ts`), so the service needs its own
   parser on the pty stream; the OSC 133 `D` exit code is the one source for `exit_code`.
4. Full-screen detection (`fullscreen`) needs the alternate-screen switch (`CSI ? 1049 h/l`) parsed on the same stream.
