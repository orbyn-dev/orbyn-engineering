# Terminal input: typed command proposals

Status: the service records the field and the web is wired to it: the terminal editor sends `purpose: "terminal_proposal"` when it starts a run as a prompt and reads the `orbyn.proposal` event for that run, so the card can show in the real app. Not observed against a real engine; a queued prompt shows no card, and there is no packaged-app observation yet.

## Field

A run event may carry one `proposal` object (engine event field, `M20`):

| Field | Type | Rules |
| --- | --- | --- |
| `command` | string | Required. One line, at most 4096 characters. No NUL or other control character except tab; no newline. Trimmed. |
| `note` | string | Optional. At most 1024 characters. Shown as plain text; control characters are dropped and whitespace collapsed. |

TypeScript: `TerminalProposal { command: string; note?: string }` in `web/src/features/panels/terminal/input/proposal.ts`. A value that breaks a rule is dropped whole; the web does not repair a command.

## Source of truth

The service reports the field. The web never reads the model's answer text for a command: nothing scrapes `<proposed_command>` or `<proposed_note>`. The client flag `proposalSource` is `service` by default, so the production client draws the card from the `orbyn.proposal` event of the run the editor started. A test can set it to `none`, or offer typed test proposals. No mock ships in the production bundle.

## Behavior

The card is the Designer's `english-proposed` frame; the field supplies only the command and the note.

- Absent field, a field for another run, or `proposalSource` of `none`: the card is hidden. Nothing else changes.
- Present for the run the editor started: a card shows the command as escaped text and the note, with Run, Edit and Cancel.
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

Known limits: not observed against a real engine or in a packaged app; a queued prompt shows no card because the editor does
not know its run id (the queue body carries no `purpose`); a preparation retry (the project script failed and the person
presses Retry) relaunches the run without its typed result request; the raw proposal is stored in the run's events and only
the web card masks a secret-looking command.
