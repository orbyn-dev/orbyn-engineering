import { afterEach, expect, it, vi } from "vitest";
import { insertIntoTerminal, registerTerminal, terminalKey, type TerminalControls } from "./terminalControls";

const controls = (write = vi.fn()): TerminalControls => ({ clear: vi.fn(), write, interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined });
let off: (() => void) | null = null;
afterEach(() => { off?.(); off = null; });

it("types one line into the thread's main terminal with no Enter", () => {
  const write = vi.fn();
  off = registerTerminal(terminalKey("t1", undefined), controls(write));
  expect(insertIntoTerminal("t1", "npm test -- 'auth'")).toBe(true);
  expect(write).toHaveBeenCalledExactlyOnceWith("npm test -- 'auth'");
});

it("refuses a line break, an escape or any control character, and writes nothing", () => {
  const write = vi.fn();
  off = registerTerminal(terminalKey("t1", undefined), controls(write));
  for (const text of ["a\nb", "a\rb", "a\u001b[A", "a\u0003", "a\u007f", ""]) expect(insertIntoTerminal("t1", text), JSON.stringify(text)).toBe(false);
  expect(write).not.toHaveBeenCalled();
});

it("answers false when no terminal is open for the thread", () => {
  off = registerTerminal(terminalKey("t2", undefined), controls());
  expect(insertIntoTerminal("t1", "ls")).toBe(false);
});
