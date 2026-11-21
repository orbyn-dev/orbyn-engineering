import { looksSecret } from "./classify";

/** A shell command the engine put forward, to be shown as text and run only when the person presses Run. */
export interface Proposal {
  command: string;
  /** The engine's one line on what the command does, when it gave one. */
  note: string | null;
  /** The command looks like it holds a secret: it is shown masked and never filled in or run from here. */
  secret: boolean;
}

const COMMAND = /<proposed_command>([\s\S]*?)<\/proposed_command>/;
const NOTE = /<proposed_note>([\s\S]*?)<\/proposed_note>/;
const MAX_COMMAND = 1000;
const MAX_NOTE = 280;
// Anything that could move the cursor, end a line early or type something else: all C0 controls and DEL.
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;

/**
 * The command inside an engine message, if it proposed one. One plain line only: a second line, a control character
 * or an overlong command is not offered, so a proposal can never type more than the line the person sees.
 */
export function proposedCommandIn(text: string): Proposal | null {
  const raw = COMMAND.exec(text)?.[1];
  if (raw === undefined) return null;
  const fenced = /^\s*```[^\n]*\n([\s\S]*?)\n?```\s*$/.exec(raw);
  const command = (fenced?.[1] ?? raw).trim();
  if (command === "" || command.length > MAX_COMMAND || CONTROL.test(command)) return null;
  const note = NOTE.exec(text)?.[1]?.replace(/<[^>]*>/g, "").replace(/\s+/g, " ").trim().slice(0, MAX_NOTE) || null;
  return { command, note, secret: looksSecret(command) };
}
