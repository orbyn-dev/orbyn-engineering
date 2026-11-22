import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Thread } from "../../../api/types";
import { useAttention } from "../../panels/terminal/attention/store";
import { ThreadRow, type RowActions } from "./ThreadRow";

const thread = { id: "t1", project_id: "p1", title: "Fix login", engine: "claude", model: null, scratch: false, active_run: null, last_run_state: "completed", lifecycle: null, worktree: null, updated_at: "2020-01-01T00:00:00Z" } as unknown as Thread;
const actions = new Proxy({}, { get: () => vi.fn() }) as RowActions;
const row = (id = "t1") => render(
  <ThreadRow thread={{ ...thread, id }} section="active" selected={false} compact={false} multiline={false} multiSelected={false}
    focusable dragging={false} highlighted={false} verb={null} actions={actions} />,
);
const at = (seq: number, value: unknown, thread_id = "t1") => ({ seq, ts: "2026-11-22T14:12:03Z", thread_id, event: { type: "CUSTOM", name: "orbyn.terminal_attention", value } });
const waiting = { v: 1, op: "waiting", id: "a1", terminal_id: "tm1", terminal_title: "dev", engine: "claude", kind: "yes_no", since: "2026-11-22T14:12:00Z", excerpt: ["Overwrite? (y/N)"] };

beforeEach(() => useAttention.getState().reset());
afterEach(cleanup);

describe("the thread row of a thread with a waiting terminal", () => {
  it("has no dot while nothing waits", () => {
    const { container } = row();
    expect(container.querySelector(".status-dot")).toBeNull();
  });
  it("shows the amber dot and says Terminal, for its own thread only", () => {
    act(() => { useAttention.getState().receive(at(1, waiting)); });
    const mine = row();
    expect(mine.container.querySelector(".status-dot.tone-warn")).not.toBeNull();
    expect(screen.getByText("Terminal")).toBeInTheDocument();
    mine.unmount();
    expect(row("t2").container.querySelector(".status-dot")).toBeNull();
  });
  it("loses the dot when the terminal is answered or the item is dismissed", () => {
    act(() => { useAttention.getState().receive(at(1, waiting)); });
    const { container } = row();
    act(() => { useAttention.getState().dismiss("a1"); });
    expect(container.querySelector(".status-dot")).toBeNull();
    act(() => { useAttention.getState().reset(); useAttention.getState().receive(at(2, { ...waiting, id: "a2" })); });
    expect(container.querySelector(".status-dot.tone-warn")).not.toBeNull();
    act(() => { useAttention.getState().receive(at(3, { v: 1, op: "clear", id: "a2", reason: "closed" })); });
    expect(container.querySelector(".status-dot")).toBeNull();
  });
});
