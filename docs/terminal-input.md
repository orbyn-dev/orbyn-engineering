# Terminal input: typed command proposals

Status: proposed for Eng 4 (owner of the engines and the service). The web side is built; the service does not send the field yet.

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

- Absent field, or `proposalSource` of `none`: the card is hidden. Nothing else changes.
- Present: a card shows the command as escaped text (masked when it looks like a secret, with no Accept) and the note.
- Accept puts the command in the input line once per click and removes the card. It writes nothing to the shell.
- Dismiss removes the card and changes nothing.
- A proposal is tied to the run the editor started; one for another run is ignored.

## Security

- The card never executes. The person runs the line from the input with Enter, like any typed command.
- Treat `command` as untrusted text from a model. Limits and the control-character rule keep one proposal to one visible line.
- Only if the service itself ever executes a proposal is that Person-class (a person's explicit approval, never a model or agent). That is out of scope here and would need its own design.
