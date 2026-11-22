import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Approval } from "../../api/types";
import type { ToolItem } from "../../product/timeline/types";
import { codeRun } from "../../../mock/codemode";
import { installMockFetch } from "../../test-support";
import { useWorkspace } from "../../product/state/workspace";
import { ApprovalCard } from "../thread/ApprovalCard";
import { isRunId, runAnchorId, runIdOf } from "./model";
import { resetCodeRuns } from "./useCodeRun";

const base: Approval = {
  id: "a5",
  thread_id: "t_9",
  run_id: "r",
  kind: "tool",
  summary: "Use orbyn_thread_send",
  detail: { tool: "orbyn_thread_send", thread_id: "t_52" },
  state: "pending",
  decided_by: null,
  decided_at: null,
  via: null,
  created_at: "2026-11-21T10:00:42.000Z",
};
const withVia = (run: unknown): Approval => ({ ...base, via: { code_run: run, call: 5, script_sha256: null } } as unknown as Approval);

let frames: FrameRequestCallback[] = [];
const flush = async () => {
  // The lookup retries across animation frames; run them all.
  for (let at = 0; at < 40 && frames.length > 0; at += 1) {
    const now = frames;
    frames = [];
    await act(async () => { now.forEach((frame) => frame(0)); });
  }
};

beforeEach(() => {
  resetCodeRuns();
  const mock = installMockFetch();
  mock.codeMode.putRun(codeRun({ run_id: "run_7c41e2", status: "paused", script: "1", started_at: "2026-11-21T10:00:00.000Z", ended_at: null, calls: [], pending: [] }));
  useWorkspace.setState({ approvals: { data: null, error: null, loading: false }, receipts: {} });
  window.location.hash = "#/";
  frames = [];
  vi.spyOn(window, "requestAnimationFrame").mockImplementation((frame) => { frames.push(frame); return frames.length; });
  vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => undefined);
});
afterEach(() => {
  vi.restoreAllMocks();
  document.body.querySelectorAll("[data-test-block]").forEach((one) => one.remove());
});

const block = (id: string) => {
  const element = document.createElement("div");
  element.id = id;
  element.tabIndex = -1;
  element.setAttribute("data-test-block", "");
  element.scrollIntoView = vi.fn();
  document.body.append(element);
  return element;
};

describe("run anchors", () => {
  it("only accepts the ids the service makes", () => {
    expect(isRunId("run_7c41e2")).toBe(true);
    expect(isRunId("a".repeat(64))).toBe(true);
    expect(isRunId("a".repeat(65))).toBe(false);
    for (const bad of ["", "a b", "a/b", "a\"b", "a<b>", "../x", "é"]) expect(isRunId(bad)).toBe(false);
    expect(runAnchorId("run_7c41e2")).toBe("cm-run-run_7c41e2");
    expect(runAnchorId("a b")).toBeNull();
  });

  it("finds a run call's own run id from its reply or its resume argument", () => {
    const item = (args: unknown, result: unknown, name = "mcp__orbyn_engineering__orbyn_code_run"): ToolItem => ({
      kind: "tool", key: "t", seq: 0, ts: "", runId: "r", callId: "c", name,
      args: JSON.stringify(args), output: "", result: result === null ? null : JSON.stringify(result), done: true, exitCode: 0,
    });
    expect(runIdOf(item({ code: "return 1" }, { status: "paused", runId: "run_reply" }))).toBe("run_reply");
    expect(runIdOf(item({ run_id: "run_args" }, { status: "paused" }))).toBe("run_args");
    expect(runIdOf(item({ code: "x" }, { status: "completed", runId: "bad id" }))).toBeNull();
    expect(runIdOf(item({ code: "x" }, null))).toBeNull();
    expect(runIdOf(item({ run_id: "run_x" }, null, "mcp__orbyn_engineering__orbyn_code_search"))).toBeNull();
  });
});

describe("Open the run", () => {
  it("is drawn for a valid run id", () => {
    render(<ApprovalCard approval={withVia("run_7c41e2")} />);
    expect(screen.getByRole("button", { name: "Open the run" })).toBeInTheDocument();
  });

  it("is not drawn for an approval without via, or an unusable run id", () => {
    const { unmount } = render(<ApprovalCard approval={base} />);
    expect(screen.queryByRole("button", { name: "Open the run" })).toBeNull();
    unmount();
    render(<ApprovalCard approval={withVia("run 1<script>")} />);
    expect(screen.queryByRole("button", { name: "Open the run" })).toBeNull();
  });

  it("goes to the approval's thread, then scrolls to the run block and focuses it", async () => {
    const element = block("cm-run-run_7c41e2");
    render(<ApprovalCard approval={withVia("run_7c41e2")} />);
    await userEvent.click(screen.getByRole("button", { name: "Open the run" }));
    expect(window.location.hash).toBe("#/threads/t_9");
    await flush();
    expect(element.scrollIntoView).toHaveBeenCalledWith({ block: "center", behavior: "smooth" });
    expect(element).toHaveFocus();
    expect(screen.queryByText("That run is not in this thread's transcript.")).toBeNull();
  });

  it("does not animate under reduced motion", async () => {
    vi.spyOn(window, "matchMedia").mockImplementation((query) => ({ matches: query.includes("reduce"), media: query, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {}, dispatchEvent: () => false, onchange: null }) as MediaQueryList);
    const element = block("cm-run-run_7c41e2");
    render(<ApprovalCard approval={withVia("run_7c41e2")} />);
    await userEvent.click(screen.getByRole("button", { name: "Open the run" }));
    await flush();
    expect(element.scrollIntoView).toHaveBeenCalledWith({ block: "center", behavior: "auto" });
  });

  it("leaves the address alone when already in that thread", async () => {
    window.location.hash = "#/threads/t_9";
    const element = block("cm-run-run_7c41e2");
    const before = window.history.length;
    render(<ApprovalCard approval={withVia("run_7c41e2")} />);
    await userEvent.click(screen.getByRole("button", { name: "Open the run" }));
    await flush();
    expect(window.history.length).toBe(before);
    expect(element.scrollIntoView).toHaveBeenCalled();
  });

  it("says so quietly when the run is not in the transcript, and does nothing else", async () => {
    render(<ApprovalCard approval={withVia("run_7c41e2")} />);
    await userEvent.click(screen.getByRole("button", { name: "Open the run" }));
    await flush();
    expect(await screen.findByText("That run is not in this thread's transcript.")).toBeInTheDocument();
  });
});
