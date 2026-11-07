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
const watchers = new Map<string, Set<(controls: TerminalControls) => void | (() => void)>>();

export const terminalKey = (threadId: string, terminalId: string | undefined) => `${threadId}:${terminalId ?? "main"}`;

/** Registers an open terminal; the returned function removes it if it is still the one registered. */
export function registerTerminal(key: string, controls: TerminalControls): () => void {
  open.set(key, controls);
  for (const watcher of watchers.get(key) ?? []) watcher(controls);
  return () => {
    if (open.get(key) === controls) open.delete(key);
  };
}

export const controlsOf = (key: string): TerminalControls | undefined => open.get(key);

/**
 * Runs `listener` with the terminal's controls once it has registered, and again if it registers anew,
 * so something that opens before xterm has loaded still hears it. What `listener` returns is called
 * when it is replaced or when the watch ends.
 */
export function watchTerminal(key: string, listener: (controls: TerminalControls) => void | (() => void)): () => void {
  let cleanup: void | (() => void);
  const run = (controls: TerminalControls) => {
    cleanup?.();
    cleanup = listener(controls);
  };
  const set = watchers.get(key) ?? new Set();
  watchers.set(key, set);
  set.add(run);
  const there = open.get(key);
  if (there) run(there);
  return () => {
    set.delete(run);
    if (set.size === 0) watchers.delete(key);
    cleanup?.();
  };
}
