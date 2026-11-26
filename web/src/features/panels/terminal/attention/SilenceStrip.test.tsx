import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { SilenceStrip } from "./SilenceStrip";
import { useAttention } from "./store";

const at = (seq: number, value: unknown, thread = "t1") => ({ seq, ts: "2026-11-22T14:12:03Z", thread_id: thread, event: { type: "CUSTOM", name: "orbyn.terminal_attention", value } });
const waiting = (over: Record<string, unknown> = {}) => ({
  v: 1, op: "waiting", id: "a1", terminal_id: "tm1", terminal_title: "dev", engine: "claude", kind: "yes_no",
  since: new Date(Date.now() - 38_000).toISOString(), excerpt: ["Overwrite auth/redirect.ts with the new version? (y/N)"], ...over,
});

beforeEach(() => useAttention.getState().reset());
afterEach(cleanup);

describe("the silence strip", () => {
  it("draws the frame's copy for the engine console on screen: Waiting for you · <engine> has been silent for <n> s", () => {
    act(() => useAttention.getState().receive(at(1, waiting())));
    const { container } = render(<SilenceStrip threadId="t1" terminalIds={["tm1"]} />);
    expect(container.textContent).toBe("Waiting for you · Claude Code has been silent for 38 s");
    expect(container.querySelector(".status-dot.tone-warn")).not.toBeNull();
    for (const absent of ["Answer here", "Yes", "No", "Dismiss"]) expect(container.textContent).not.toContain(absent);
  });

  it("leaves when the event clears", () => {
    act(() => useAttention.getState().receive(at(1, waiting())));
    const { container } = render(<SilenceStrip threadId="t1" terminalIds={["tm1"]} />);
    expect(container.querySelector(".term-wait")).not.toBeNull();
    act(() => useAttention.getState().receive(at(2, { v: 1, op: "clear", id: "a1", reason: "input" })));
    expect(container).toBeEmptyDOMElement();
  });

  it("leaves when the item is dismissed on this window", () => {
    act(() => useAttention.getState().receive(at(1, waiting())));
    const { container } = render(<SilenceStrip threadId="t1" terminalIds={["tm1"]} />);
    act(() => useAttention.getState().dismiss("a1"));
    expect(container).toBeEmptyDOMElement();
  });

  it("draws only for a terminal on screen, in this thread", () => {
    act(() => useAttention.getState().receive(at(1, waiting())));
    const elsewhere = render(<SilenceStrip threadId="t2" terminalIds={["tm1"]} />);
    expect(elsewhere.container).toBeEmptyDOMElement();
    elsewhere.unmount();
    const otherTerminal = render(<SilenceStrip threadId="t1" terminalIds={["tm2"]} />);
    expect(otherTerminal.container).toBeEmptyDOMElement();
  });

  it("a secret prompt's strip names the engine and nothing it printed", () => {
    act(() => useAttention.getState().receive(at(1, waiting({ kind: "secret", excerpt: ["Password:"] }))));
    const { container } = render(<SilenceStrip threadId="t1" terminalIds={["tm1"]} />);
    expect(container.textContent).toBe("Waiting for you · Claude Code has been silent for 38 s");
    expect(container.textContent).not.toContain("Password");
  });
});
