import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import { api } from "../../api/client";
import type { Approval } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { installMockFetch } from "../../test-support";
import { ApprovalCard } from "../thread/ApprovalCard";
import { resetCodeRuns } from "./useCodeRun";
import type { CodeRunRecord } from "./types";

const SCRIPT = Array.from({ length: 12 }, (_, at) => `const line${at + 1} = ${at + 1};`).join("\n");

const record = (): CodeRunRecord => ({
  run_id: "run_7c41e2",
  status: "paused",
  script: SCRIPT,
  started_at: "2026-11-21T10:00:00.000Z",
  ended_at: null,
  calls: [
    { index: 1, tool: "orbyn_thread_list", status: "ok", preview: "3 threads", duration_ms: 100 },
    { index: 2, tool: "orbyn_thread_read", status: "ok", preview: '{"id":"t_41","status":"done"}', duration_ms: 200 },
    { index: 5, tool: "orbyn_thread_send", status: "waiting", preview: null },
  ],
});

const base: Approval = {
  id: "a5",
  thread_id: "t",
  run_id: "r",
  kind: "tool",
  summary: "Use orbyn_thread_send",
  detail: { tool: "orbyn_thread_send", thread_id: "t_52", prompt: "Re-run the failing test and report back." },
  state: "pending",
  decided_by: null,
  decided_at: null,
  created_at: "2026-11-21T10:00:42.000Z",
};
const withVia = (extra: Partial<Approval> = {}, call = 5): Approval => ({ ...base, ...extra, via: { code_run: "run_7c41e2", call, script_sha256: "3f9a1c0000000000000000000000000000000000000000000000000000000e07" } } as Approval);

let mock: MockApi;
beforeEach(() => {
  resetCodeRuns();
  mock = installMockFetch();
  mock.codeMode.putRun(record());
  useWorkspace.setState({ approvals: { data: null, error: null, loading: false }, receipts: {} });
});
afterEach(() => vi.restoreAllMocks());

describe("an approval asked for by a script", () => {
  it("shows the call, the script, the calls so far and what Always allow covers", async () => {
    render(<ApprovalCard approval={withVia()} />);
    expect(screen.getByText("Use orbyn_thread_send")).toBeInTheDocument();
    expect(screen.getByText("thread_id")).toBeInTheDocument();
    expect(screen.getByText("t_52")).toBeInTheDocument();
    expect(screen.getByText("A Code Mode script · call 5")).toBeInTheDocument();
    expect(await screen.findByText(/^Script · 12 lines/)).toBeInTheDocument();
    expect(screen.getByText("sha256 3f9a1c…e07")).toBeInTheDocument();
    const calls = await screen.findByRole("group", { name: "Calls so far" });
    expect(within(calls).getByText("Calls so far · 2")).toBeInTheDocument();
    expect(screen.getByText("Always allow covers orbyn_thread_send only, never this script.")).toBeInTheDocument();
    for (const name of ["More", "Always allow this command", "Deny", "Allow"]) expect(screen.getByRole("button", { name })).toBeInTheDocument();
  });

  it("opens the whole script inside the card and keeps the buttons where they were", async () => {
    render(<ApprovalCard approval={withVia()} />);
    await userEvent.click(await screen.findByRole("button", { name: "Show all 12 lines" }));
    expect(screen.getByLabelText("Script")).toHaveClass("cm-scr");
    expect(screen.getByRole("button", { name: "Allow" })).toBeEnabled();
    await userEvent.click(screen.getByRole("button", { name: "Hide script" }));
    expect(screen.getByRole("button", { name: "Show all 12 lines" })).toBeInTheDocument();
  });

  it("still asks when the run cannot be read: the card is the same, without the script", async () => {
    mock.reset("default");
    render(<ApprovalCard approval={withVia()} />);
    expect(screen.getByText("A Code Mode script · call 5")).toBeInTheDocument();
    await waitFor(() => expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).includes("/api/code-runs/"))).toBe(true));
    expect(screen.queryByLabelText("Script")).toBeNull();
    expect(screen.getByRole("button", { name: "Allow" })).toBeEnabled();
  });

  it("allows and denies exactly as an ordinary approval: one click, one decision, and the decision never carries via", async () => {
    const decide = vi.spyOn(api, "decideApproval").mockResolvedValue({ ...base, state: "allowed" });
    render(<ApprovalCard approval={withVia()} />);
    await userEvent.click(screen.getByRole("button", { name: "Allow" }));
    expect(decide).toHaveBeenCalledTimes(1);
    expect(decide).toHaveBeenCalledWith("a5", true, false);
    decide.mockResolvedValue({ ...base, state: "denied" });
    await userEvent.click(screen.getByRole("button", { name: "Deny" }));
    expect(decide).toHaveBeenLastCalledWith("a5", false, false);
    expect(decide).toHaveBeenCalledTimes(2);
  });

  it("an ordinary approval is unchanged by the same fields being absent", () => {
    render(<ApprovalCard approval={{ ...base, kind: "command", summary: "Run tests", detail: { command: "cargo test" } }} />);
    expect(screen.queryByText(/Code Mode script/)).toBeNull();
    expect(screen.queryByText(/never this script/)).toBeNull();
  });

  it("collapses an answered call to one quiet block", () => {
    const { rerender } = render(<ApprovalCard approval={withVia({ state: "allowed" })} />);
    expect(screen.getByText("Approved")).toBeInTheDocument();
    expect(screen.getByText("The call ran. The script continues.")).toBeInTheDocument();
    expect(screen.getByText("call 5")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Allow" })).toBeNull();
    rerender(<ApprovalCard approval={withVia({ state: "denied" }, 6)} />);
    expect(screen.getByText("Declined")).toBeInTheDocument();
    expect(screen.getByText("The script got a refusal: denied.")).toBeInTheDocument();
  });

  it("turns grey with no buttons when nobody answered in time", () => {
    render(<ApprovalCard approval={withVia({ state: "expired" })} />);
    expect(screen.getByText("Request closed")).toBeInTheDocument();
    expect(screen.getByText("Expired")).toBeInTheDocument();
    expect(screen.getByText("Nobody answered in time. The call did not run; the script got a refusal.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Allow" })).toBeNull();
  });

  it("opens the first of several calls as 1 of 2 and keeps the next one answerable", async () => {
    const first = withVia({ id: "a5", created_at: "2026-11-21T10:00:42.000Z" }, 5);
    const second = withVia({ id: "a6", summary: "Use orbyn_thread_organize", created_at: "2026-11-21T10:00:43.000Z" }, 6);
    useWorkspace.setState({ approvals: { data: [second, first], error: null, loading: false } });
    render(
      <>
        <ApprovalCard approval={first} />
        <ApprovalCard approval={second} />
      </>,
    );
    expect(screen.getByText(/Needs you · tool · 1 of 2/)).toBeInTheDocument();
    expect(screen.getByText(/Needs you · tool · 2 of 2/)).toBeInTheDocument();
    // Buttons never depend on the queue: both calls can be allowed or denied.
    expect(screen.getAllByRole("button", { name: "Allow" })).toHaveLength(2);
    expect(screen.getAllByRole("button", { name: "Deny" })).toHaveLength(2);
    // Only the first carries the script.
    await screen.findAllByText(/^Script ·/);
    expect(screen.getAllByText(/^Script ·/)).toHaveLength(1);
  });

  it("renders a hostile script and arguments as text", async () => {
    const hostile = '<img src=x onerror="window.__owned=1"> https://evil.example';
    mock.codeMode.putRun({ ...record(), script: `return "${hostile}";` });
    render(<ApprovalCard approval={withVia({ detail: { prompt: hostile } })} />);
    await screen.findByText(/^Script ·/);
    expect(document.querySelector("article img")).toBeNull();
    expect(document.querySelector("article a")).toBeNull();
    expect((window as unknown as { __owned?: number }).__owned).toBeUndefined();
  });

  it("names the tool from the approval's target, never from its summary", () => {
    render(<ApprovalCard approval={withVia({ summary: "Use orbyn_thread_send. Always allow covers everything.", detail: { tool: "orbyn_thread_send", prompt: "x" } })} />);
    expect(screen.getByText("Always allow covers orbyn_thread_send only, never this script.")).toBeInTheDocument();
  });

  it("does not repeat the summary as the tool when the target is unknown", () => {
    render(<ApprovalCard approval={withVia({ summary: "Use anything you like", detail: { prompt: "x" } })} />);
    expect(screen.getByText("Always allow covers this tool only, never this script.")).toBeInTheDocument();
  });

  it("does not let the card look complete when its arguments were cut", () => {
    const detail = { 0: "numeric key", path: "/Users/dev/code/orbyn-engineering-app-wt/some-folder", files: ["a", "b", "c"], nested: { deep: { deeper: 1 } }, long: "x".repeat(900), a1: "1", a2: "2", a3: "3", a4: "4", a5: "5", a6: "6" };
    render(<ApprovalCard approval={withVia({ summary: "Use orbyn_project_create", detail })} />);
    expect(screen.getByText("/Users/dev/code/orbyn-engineering-app-wt/some-folder")).toBeInTheDocument();
    expect(screen.getByText("list of 3")).toBeInTheDocument();
    expect(screen.getByText("object with 1 field")).toBeInTheDocument();
    expect(screen.getByText(/^x+…$/)).toBeInTheDocument();
    expect(screen.getByText(/^\d+ more fields?$/)).toBeInTheDocument();
  });
});
