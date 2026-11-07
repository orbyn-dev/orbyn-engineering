/** What the drawer asks of a terminal that is open: clear it, interrupt it, read what it printed. */
export interface TerminalControls {
  clear(): void;
  /** Sends Ctrl-C. */
  interrupt(): void;
  /** Everything in the scrollback and on screen, as text. */
  output(): string;
  /** Marks every match of `text` and moves to the next or previous one; an empty text clears the marks. */
  find(text: string, direction: "next" | "previous"): void;
  /** Hears the current match's place: `index` is -1 when there is none. */
  onResults(listener: (results: { index: number; count: number }) => void): () => void;
}

const open = new Map<string, TerminalControls>();

export const terminalKey = (threadId: string, terminalId: string | undefined) => `${threadId}:${terminalId ?? "main"}`;

/** Registers an open terminal; the returned function removes it if it is still the one registered. */
export function registerTerminal(key: string, controls: TerminalControls): () => void {
  open.set(key, controls);
  return () => {
    if (open.get(key) === controls) open.delete(key);
  };
}

export const controlsOf = (key: string): TerminalControls | undefined => open.get(key);
