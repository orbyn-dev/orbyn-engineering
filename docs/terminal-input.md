# Terminal input: typed command proposals

Status: the service sends the field (built, not verified on a real engine). The web side is built.

## Field

A run event may carry one `proposal` object (engine event field, `M20`):

| Field | Type | Rules |
| --- | --- | --- |
| `command` | string | Required. One line, at most 4096 characters. No NUL or other control character except tab; no newline. Trimmed. |
| `note` | string | Optional. At most 1024 characters. Shown as plain text; control characters are dropped and whitespace collapsed. |

TypeScript: `TerminalProposal { command: string; note?: string }` in `web/src/features/panels/terminal/input/proposal.ts`. A value that breaks a rule is dropped whole; the web does not repair a command.

## Source of truth

The service reports the field. The web never reads the model's answer text for a command: nothing scrapes `<proposed_command>` or `<proposed_note>`. The client flag `proposalSource` is `none` by default, so the production client draws no card. A test client sets it to `service` and offers typed proposals. No mock ships in the production bundle.

## Behavior

The card is the Designer's `english-proposed` frame; the field supplies only the command and the note.

- Absent field, or `proposalSource` of `none`: the card is hidden. Nothing else changes.
- Present: a card shows the command as escaped text and the note, with Run, Edit and Cancel.
- Run writes the command to the shell on the person's click, once per click (busy guard), then removes the card. Nothing runs without that click.
- Edit puts the command in the input line and runs nothing. Cancel removes the card.
- A command that looks like a secret is shown masked with no Run or Edit, only Cancel.
- A proposal is tied to the run the editor started; one for another run is ignored.

## Security

- The card never runs anything on its own; Run is the person's explicit click.
- Treat `command` as untrusted text from a model. The limits and the control-character rule keep one proposal to one visible line.
- Run is the person's click writing to their own terminal through the existing terminal input route, which is already
  Person-class. The service records no extra approval and executes nothing itself; the `proposal` event carries text only.

## Service

A run the terminal editor starts sends `purpose: "terminal_proposal"` on the send body (not with `result_schema`). The core supplies
the schema `{command: string, note?: string}`, asks the engine for it through the typed turn result (docs/architecture.md, "Typed
turn result") and, when the value passes the rules above, records one `orbyn.proposal` run event (`CUSTOM`, value
`{command, note?}`) after the `orbyn.turn_result` event. A value that breaks a rule records no proposal; the result event still says what
came back. Nothing scrapes tags from the answer text.
