# Command blocks (W1)

What the terminal drawer needs from the service to show one block per command. **Service built, web hidden**: the service keeps,
redacts and serves blocks through the two routes below; the web side is built hidden and in a production build has no client, so the
terminal is unchanged. A dev-only preview (`#/preview/command-blocks`) installs the mock in `web/mock/commandBlocks.ts`. Names here
change only if the service pull request says so. Row: G17 in [parity](parity.md). Frames: `block-*` in Engineer Designs.

## What the service keeps

Per terminal (a thread's `main` shell or a named terminal), an ordered list of blocks, oldest first. A block starts at an OSC 133 `C`
mark (output starts) and ends at the `D` mark (or at the next `A` or `C` mark when no `D` arrives, with no exit code). The marks are
the ones the web already reads in `web/src/features/panels/terminal/shellMarks.ts`; Orbyn installs no shell integration, so a shell
that emits none yields no blocks. A command Orbyn itself runs (a project script, a Crew check, an agent command) would be one block
with the same fields only when a single marked-terminal write path already exists; the service adds none today (see Decisions).

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
  can change without touching the routes. If the rules cannot be read, that terminal keeps nothing and the list answers empty.
- **Marks are not verified.** A block's boundaries, `exit_code` and `command` come from marks (OSC 133, OSC 633;E, OSC 7) in the
  terminal's own output, so any program or `cat file` can forge them: a failing build can show exit 0 and a Run again line can be
  chosen by a file's content. The web shows the command before Run again and never sends Enter; treat `exit_code` as advisory until
  the shell integration passes a nonce (not built). A flood of forged `C` marks can fill the 500-block cap and evict real history.
- **Only a partial last line is held back.** A complete line of a running block is readable as soon as its newline arrives, and
  `output_bytes` counts it, so a command that prints "Compiling..." and then waits shows that line before it ends. A last line with no
  newline yet (a prompt, a progress line) stays pending until its newline arrives or the command ends.
- The working directory (`cwd`) is redacted like the command.
- **A terminal that carries a workflow run's env values keeps no blocks.** Those values are masked on the output stream after the block
  parser reads it, so the parser never records such a terminal (tested: `a_terminal_with_run_secrets_keeps_no_command_blocks`).
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

## Decisions

The service side of #974 is built in `crates/orbyn-core/src/host/blocks/` (parser, store, text) and
`crates/orbynd/src/routes/blocks.rs`, with module tests and a service test in `crates/orbynd/tests/command_blocks.rs`. The web stays
hidden.

1. **Routes stay as written.** `GET /api/threads/{id}/terminals/{terminal}/blocks?before=&limit=` and
   `GET .../blocks/{block_id}/output?after=&limit=` are both Local reads, like the neighbouring terminal reads. Neither writes. There
   is no service-side run, rerun or delete route.
2. **Limits stay as written.** 500 blocks per terminal, 1 MiB of text per block, 8 MiB per terminal; the oldest is dropped whole and a
   running block is never dropped. The list returns `limits`. Blocks are in memory only: an `orbynd` restart clears them and closing,
   settling or archiving a terminal drops them with it. A running block ends there with no exit code.
3. **The service parses the pty stream.** A small streaming state machine reads OSC 133 and 633 across read boundaries (`A`, `B`, `C`,
   `D;<code>`), plus `633;E` for `command`, OSC 7 and `633;P;Cwd=` for `cwd`, and the alternate-screen switch (`CSI ? 1049 h/l`, also
   `?1047` and `?47`) for `fullscreen`. It emits no block for a shell that emits no marks; that is an empty list, never an error. A
   block ends at `D`, at the next `A` or `C` when no `D` arrived, or when the terminal closes. A block that used the alternate screen
   keeps no output: `output_bytes` is 0 and the output route answers an empty page.
4. **Text is stored plain.** Bytes are decoded as UTF-8 lossily, then CSI, OSC, DCS and every other ESC sequence and every control
   character except newline and tab are removed; a carriage return that redraws a line keeps the last text of that line. `command` is
   one line, cut (not wrapped) at 4096 characters, taken from `633;E` when the shell sent it and null otherwise.
5. **Redaction runs before storage.** Every command and every chunk of output goes through the `Redactor` for `Surface::Terminal`
   through one `redact(text) -> text` in the block store before it is stored, and again before it is answered. If no redactor can be
   obtained the store keeps nothing and the list is empty. Nothing logs command text or output; a log line names the terminal id, block
   id and a byte count.
6. **Cursors are opaque and validated.** A cursor into text the service dropped is `410 cursor_expired`; a malformed cursor is
   `422 bad_cursor`; a gone thread, terminal or block is `404 not_found`, and a thread the person cannot open answers the same `404`.
   Output pages never cut a UTF-8 character and prefer a line boundary.
7. **A command Orbyn itself runs is not a block yet.** Project scripts, Crew checks and agent commands either run as a separate
   `shell -lc` process that emits no marks or are typed into an engine's pty; there is no single place that writes a shell command into
   a marked terminal. The parser would read the marks if such a terminal emitted them, but the service adds no synthetic block. This
   stays listed in G17.
