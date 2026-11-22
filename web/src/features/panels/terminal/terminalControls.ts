/** What the drawer asks of a terminal that is open: clear it, interrupt it, read what it printed. */
export interface TerminalControls {
  clear(): void;
  /** Types text into the shell as if it were typed in the terminal itself. Nothing when the terminal only watches. */
  write(data: string): void;
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
const registryListeners = new Set<() => void>();

export const terminalKey = (threadId: string, terminalId: string | undefined) => `${threadId}:${terminalId ?? "main"}`;

/** Calls back when a terminal opens or closes in this window. Returns a stop function. */
export function onTerminalRegistryChange(listener: () => void): () => void {
  registryListeners.add(listener);
  return () => {
    registryListeners.delete(listener);
  };
}

/**
 * The threads with a terminal open in this window, in the order their first terminal registered. Used to read that thread's
 * waiting items once when the window opens; the terminal key starts with the thread id (`terminalKey`).
 */
export function openTerminalThreads(): string[] {
  const threads = new Set<string>();
  for (const key of open.keys()) {
    const at = key.indexOf(":");
    if (at > 0) threads.add(key.slice(0, at));
  }
  return [...threads];
}

const registryChanged = () => registryListeners.forEach((listener) => listener());

/** Registers an open terminal; the returned function removes it if it is still the one registered. */
export function registerTerminal(key: string, controls: TerminalControls): () => void {
  open.set(key, controls);
  registryChanged();
  for (const watcher of watchers.get(key) ?? []) watcher(controls);
  return () => {
    if (open.get(key) === controls) {
      open.delete(key);
      registryChanged();
    }
  };
}

/**
 * Types `text` into a thread's main terminal over its own socket, without Enter, so nothing runs until the person presses Enter there.
 * Refuses (returns false) when no such terminal is open, when it only watches, or when the text holds a line break or any other
 * control character: a newline in a terminal is Enter, and an escape can move the cursor or answer a prompt.
 */
export function insertIntoTerminal(threadId: string, text: string): boolean {
  const controls = open.get(terminalKey(threadId, undefined));
  if (!controls || text === "" || Array.from(text).some((char) => { const code = char.charCodeAt(0); return code < 32 || code === 127; })) return false;
  controls.write(text);
  return true;
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
