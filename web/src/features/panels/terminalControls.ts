/** What the drawer asks of a terminal that is open: clear it, interrupt it, read what it printed. */
export interface TerminalControls {
  clear(): void;
  /** Sends Ctrl-C. */
  interrupt(): void;
  /** Everything in the scrollback and on screen, as text. */
  output(): string;
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
