import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { codeRun } from "../../../mock/codemode";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { CodeRunsList } from "./RunsList";
import { resetCodeRuns } from "./useCodeRun";

let mock: MockApi;
beforeEach(() => {
  resetCodeRuns();
  mock = installMockFetch();
});
afterEach(() => vi.restoreAllMocks());

const started = "2026-11-21T10:00:00.000Z";

describe("the runs of a thread", () => {
  it("draws a CodeRunSummary row: id, status, times, call count and the head of the script", async () => {
    mock.codeMode.putRun(codeRun({ run_id: "run_7c41e2", status: "completed", started_at: started, ended_at: "2026-11-21T10:00:03.100Z", script: "const a = await tools.orbyn_thread_list({});\nreturn a;", calls: [{ index: 1, tool: "orbyn_thread_list", status: "ok" }, { index: 2, tool: "orbyn_thread_read", status: "ok" }] }));
    render(<CodeRunsList threadId="t-1" />);
    const row = await screen.findByRole("button", { name: /run_7c41e2/ });
    expect(row).toHaveTextContent("Completed");
    expect(row).toHaveTextContent("2 calls");
    expect(row).toHaveTextContent("const a = await tools.orbyn_thread_list({}); return a;");
    expect(screen.getByText("Open a run to see its calls. Runs are kept for 7 days.")).toBeInTheDocument();
  });

  it("opens a run as the same block the thread draws, from its own record", async () => {
    mock.codeMode.putRun(codeRun({ run_id: "run_7c41e2", status: "error", started_at: started, ended_at: "2026-11-21T10:00:01.000Z", error: { kind: "limit", message: "A script may make 200 calls." } }));
    render(<CodeRunsList threadId="t-1" />);
    await userEvent.click(await screen.findByRole("button", { name: /run_7c41e2/ }));
    expect(await screen.findByText(/A script may make 200 calls\./)).toBeInTheDocument();
  });

  it.each([
    ["cancelled", "Cancelled"],
    ["lost", "Lost"],
    ["syntax", "Error"],
    ["runtime", "Error"],
    ["limit", "Error"],
    ["denied", "Error"],
    [null, "Error"],
  ] as const)("draws an errored run of kind %s as %s", async (kind, word) => {
    vi.mocked(fetch).mockResolvedValueOnce(new Response(JSON.stringify({ runs: [{ run_id: "run_k", status: "error", started_at: started, ended_at: started, calls: 1, error_kind: kind }] }), { status: 200 }));
    render(<CodeRunsList threadId="t-1" />);
    const row = await screen.findByRole("button", { name: /run_k/ });
    expect(row).toHaveTextContent(word);
    if (kind === "cancelled") expect(row).not.toHaveTextContent("Error");
  });

  it("shows a status it does not know as Unknown and does not open it", async () => {
    vi.mocked(fetch).mockResolvedValueOnce(new Response(JSON.stringify({ runs: [{ run_id: "run_x", status: "sideways", started_at: started, ended_at: null, calls: 0 }] }), { status: 200 }));
    render(<CodeRunsList threadId="t-1" />);
    const row = await screen.findByRole("button", { name: /run_x/ });
    expect(row).toHaveTextContent("Unknown");
    expect(row).toBeDisabled();
  });

  it("draws nothing, and no error, when the service does not offer Code Mode (404)", async () => {
    mock.codeMode.setOffered(false);
    const { container } = render(<CodeRunsList threadId="t-1" />);
    await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalled());
    // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
  });

  it("draws nothing for a thread with no runs, and nothing when the read fails", async () => {
    const empty = render(<CodeRunsList threadId="t-1" />);
    // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(empty.container).toBeEmptyDOMElement();
    empty.unmount();
    mock.fail("/api/threads/t-2/code-runs", 1, 500);
    const failed = render(<CodeRunsList threadId="t-2" />);
    // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(failed.container).toBeEmptyDOMElement();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
