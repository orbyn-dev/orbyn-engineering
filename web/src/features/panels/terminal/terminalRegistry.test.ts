import { describe, expect, it, vi } from "vitest";
import { onTerminalRegistryChange, openTerminalThreads, registerTerminal, terminalKey, type TerminalControls } from "./terminalControls";

const controls = (): TerminalControls => ({
  clear: vi.fn(), write: vi.fn(), interrupt: vi.fn(), output: () => "", find: vi.fn(), onResults: () => () => undefined,
});

describe("the open terminal registry the attention client reads once", () => {
  it("names each thread once, in the order its first terminal opened, and forgets it when the last closes", () => {
    const stopOne = registerTerminal(terminalKey("t1", undefined), controls());
    const stopTwo = registerTerminal(terminalKey("t1", "tm2"), controls());
    const stopThree = registerTerminal(terminalKey("t2", undefined), controls());
    try {
      expect(openTerminalThreads()).toEqual(["t1", "t2"]);
      stopOne();
      expect(openTerminalThreads()).toEqual(["t1", "t2"]);
      stopTwo();
      expect(openTerminalThreads()).toEqual(["t2"]);
      stopThree();
      expect(openTerminalThreads()).toEqual([]);
    } finally {
      stopOne();
      stopTwo();
      stopThree();
    }
  });

  it("calls back when a terminal opens or closes, and stops when asked", () => {
    const listener = vi.fn();
    const stopListening = onTerminalRegistryChange(listener);
    const stop = registerTerminal(terminalKey("t1", undefined), controls());
    expect(listener).toHaveBeenCalledTimes(1);
    stop();
    expect(listener).toHaveBeenCalledTimes(2);
    stopListening();
    const again = registerTerminal(terminalKey("t2", undefined), controls());
    expect(listener).toHaveBeenCalledTimes(2);
    again();
  });
});
