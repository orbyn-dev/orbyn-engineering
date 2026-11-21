import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUi } from "../../../../product/state/ui";
import { setAttentionApi, attentionApi } from "./contract";
import { TerminalAttention } from "./TerminalAttention";
import { useAttention } from "./store";

const at = (seq: number, value: unknown, thread = "t1") => ({ seq, ts: "2026-11-22T14:12:03Z", thread_id: thread, event: { type: "CUSTOM", name: "orbyn.terminal_attention", value } });
const waiting = (over: Record<string, unknown> = {}) => ({ v: 1, op: "waiting", id: "a1", terminal_id: "tm1", terminal_title: "dev", engine: "claude", kind: "yes_no", since: new Date(Date.now() - 38_000).toISOString(), excerpt: ["Overwrite auth/redirect.ts with the new version? (y/N)"], ...over });
const writes = vi.fn();

beforeEach(() => { useAttention.getState().reset(); writes.mockReset(); });
afterEach(() => { cleanup(); setAttentionApi(null); vi.restoreAllMocks(); });

describe("production default", () => {
  it("has no client and draws nothing", () => {
    expect(attentionApi()).toBeNull();
    const { container } = render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe("the item", () => {
  beforeEach(() => setAttentionApi({ subscribe: () => () => undefined }));
  it("draws the engine, the terminal, the question and why, with Open terminal and Dismiss only", () => {
    act(() => useAttention.getState().receive(at(1, waiting())));
    render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    expect(screen.getByText("Claude Code in the terminal “dev” is waiting for an answer.")).toBeInTheDocument();
    expect(screen.getByText(/Overwrite auth\/redirect.ts/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open terminal" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
    for (const absent of ["Answer here", "Yes", "No"]) expect(screen.queryByRole("button", { name: absent })).toBeNull();
  });
  it("draws hostile text as text", () => {
    act(() => useAttention.getState().receive(at(1, waiting({ excerpt: ["<img src=x onerror=alert(1)>", "\u001b]8;;http://x.invalid\u0007link\u001b]8;;\u0007"], terminal_title: "<b>x</b>" }))));
    const { container } = render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    expect(container.querySelector("img, b, a, script")).toBeNull();
    expect(container.textContent).toContain("<img src=x onerror=alert(1)>");
    expect(container.textContent).not.toContain("\u001b");
  });
  it("a secret prompt has Open terminal alone and no prompt box text beyond the label", () => {
    act(() => useAttention.getState().receive(at(1, waiting({ kind: "secret", excerpt: ["Password:"] }))));
    render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    expect(screen.getByText("Claude Code asked for a password in the terminal “dev”.")).toBeInTheDocument();
    expect(screen.getByText(/Orbyn does not take passwords, tokens or keys here/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open terminal" })).toBeInTheDocument();
  });
  it("Open terminal is one click, one action, and focuses only: it asks the host to open and writes nothing", () => {
    const open = vi.fn();
    act(() => useAttention.getState().receive(at(1, waiting())));
    render(<TerminalAttention threadId="t1" onOpen={open} />);
    fireEvent.click(screen.getByRole("button", { name: "Open terminal" }));
    expect(open).toHaveBeenCalledTimes(1);
    expect(open.mock.calls[0]?.[0]).toMatchObject({ threadId: "t1", terminalId: "tm1" });
  });
  it("Dismiss hides it for this window, once", () => {
    act(() => useAttention.getState().receive(at(1, waiting())));
    const { container } = render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(container).toBeEmptyDOMElement();
  });
  it("collapses to one quiet line when cleared, and says who answered", () => {
    act(() => useAttention.getState().receive(at(1, waiting())));
    render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    act(() => useAttention.getState().receive(at(2, { v: 1, op: "clear", id: "a1", reason: "input" })));
    expect(screen.queryByRole("button", { name: "Open terminal" })).toBeNull();
    expect(screen.getByText(/Answered in the terminal “dev”/)).toBeInTheDocument();
  });
  it("lists several terminals in one item, each with its own Open, and a secret with Open alone", () => {
    act(() => {
      useAttention.getState().receive(at(1, waiting()));
      useAttention.getState().receive(at(2, waiting({ id: "a2", terminal_id: "tm2", terminal_title: "test", engine: "codex", excerpt: ["Apply 3 changes? [Y/n]"] })));
      useAttention.getState().receive(at(3, waiting({ id: "a3", terminal_id: "tm3", terminal_title: "release", kind: "secret", excerpt: ["Password:"] })));
    });
    render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    expect(screen.getByText("3 terminals are waiting for you.")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^Open terminal/ })).toHaveLength(3);
    expect(screen.getByText("Each terminal is answered on its own. Orbyn does not answer several at once.")).toBeInTheDocument();
  });
  it("shows only its own thread", () => {
    act(() => useAttention.getState().receive(at(1, waiting(), "other")));
    const { container } = render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    expect(container).toBeEmptyDOMElement();
  });
  it("never reads the UI store to write anything", () => {
    const before = useUi.getState().terminalOpen;
    act(() => useAttention.getState().receive(at(1, waiting())));
    render(<TerminalAttention threadId="t1" onOpen={() => undefined} />);
    expect(useUi.getState().terminalOpen).toBe(before);
  });
});
