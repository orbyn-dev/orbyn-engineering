import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUi } from "../../../product/state/ui";
import { HOSTILE, runPage, runningRunPage, taskDetail } from "./fixture";
import { RunPage } from "./RunPage";
import { REVOKE_AFTER_MS } from "./download";
import { WITHHELD } from "./runExport";

const created: Blob[] = [];
let clicks: HTMLAnchorElement[] = [];

const answer = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });
/** The run read is absent by default: an older service answers 404, so the page keeps its Flow-based Planned marks. */
const noRunRead = () => Promise.resolve(answer({ code: "not_found", message: "task not found" }, 404));

beforeEach(() => {
  created.length = 0;
  clicks = [];
  vi.stubGlobal("fetch", vi.fn(noRunRead));
  URL.createObjectURL = vi.fn((blob: Blob) => { created.push(blob); return "blob:run-page"; });
  URL.revokeObjectURL = vi.fn();
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) { clicks.push(this); });
  useUi.setState({ toasts: [] });
});
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

const toasts = () => useUi.getState().toasts.map((toast) => toast.text);

describe("the Run page", () => {
  it("draws the task's own records and says what is Planned", () => {
    render(<RunPage detail={taskDetail()} pull={142} />);
    expect(screen.getByRole("heading", { name: "Use next in the auth redirect" })).toBeInTheDocument();
    expect(screen.getByText("3 of 3 passed")).toBeInTheDocument();
    expect(screen.getByText("next accepts //host as a path")).toBeInTheDocument();
    expect(screen.getAllByText(/Planned/).length).toBeGreaterThan(2);
    expect(screen.queryByText(/7F3A|crew-result/)).toBeNull();
    expect(screen.getByRole("button", { name: /Verify/ })).toBeDisabled();
  });

  it("puts untrusted text on the page as text", () => {
    const { container } = render(<RunPage detail={taskDetail({ row: { ...taskDetail().row, name: HOSTILE } })} pull={null} />);
    expect(container.querySelector("img, script")).toBeNull();
  });

  it("Save as file is one click and one download: a blob and an anchor, then the URL is revoked, nothing sent", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    render(<RunPage detail={taskDetail()} pull={142} />);
    await user.click(screen.getByRole("button", { name: "Save as file" }));
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
    expect(clicks).toHaveLength(1);
    expect(clicks[0]!.download).toBe("use-next-in-the-auth-redirect-run.html");
    expect(clicks[0]!.getAttribute("href")).toBe("blob:run-page");
    const text = await created[0]!.text();
    expect(text).toContain("<!doctype html>");
    expect(text).not.toMatch(/<script/i);
    expect(toasts()).toContain("Download started");
    expect(URL.revokeObjectURL).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(REVOKE_AFTER_MS); });
    expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:run-page");
    // Saving is local: the only request is the read that drew the page, never a write.
    for (const [url, init] of vi.mocked(fetch).mock.calls) {
      expect(String(url)).toMatch(/^\/api\/crew\/flow\/tasks\/[^/]+\/run$/);
      expect(init?.method ?? "GET").toBe("GET");
    }
  });

  it("Copy PR comment copies the Markdown once and says to paste it yourself", async () => {
    const write = vi.fn((_text: string) => Promise.resolve());
    const user = userEvent.setup();
    Object.defineProperty(navigator, "clipboard", { value: { writeText: write }, configurable: true });
    render(<RunPage detail={taskDetail({ row: { ...taskDetail().row, name: HOSTILE } })} pull={142} />);
    await user.click(screen.getByRole("button", { name: "Copy PR comment" }));
    expect(write).toHaveBeenCalledTimes(1);
    const text = write.mock.calls[0]![0];
    expect(text).toContain("#142");
    expect(text).not.toMatch(/@octocat|\]\(/);
    expect(await screen.findByText(/Copied to the clipboard/)).toHaveTextContent("Paste it into a comment on pull request #142 yourself");
    // Copying is local: the only request is the read that drew the page, never a write.
    for (const [, init] of vi.mocked(fetch).mock.calls) expect(init?.method ?? "GET").toBe("GET");
  });

  it("shows a clipboard failure instead of claiming a copy", async () => {
    const user = userEvent.setup();
    Object.defineProperty(navigator, "clipboard", { value: { writeText: vi.fn(() => Promise.reject(new Error("denied"))) }, configurable: true });
    render(<RunPage detail={taskDetail()} pull={null} />);
    await user.click(screen.getByRole("button", { name: "Copy PR comment" }));
    expect(await screen.findByText(/Couldn.t copy/)).toBeInTheDocument();
    expect(screen.queryByText(/Copied to the clipboard/)).toBeNull();
  });

  it("a failed run says so first", () => {
    render(<RunPage detail={taskDetail({ row: { ...taskDetail().row, state: "failed", cells: [{ stage: "checks", state: "failed", passed: 2, of: 3 }] } })} pull={null} />);
    expect(screen.getByRole("alert")).toHaveTextContent("did not pass");
    expect(screen.getByText("2 of 3 passed")).toBeInTheDocument();
  });

  it("uses the run read for the prompt, checks, not-verified lines, commit and pull request", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(runPage()))));
    render(<RunPage detail={taskDetail()} pull={142} />);
    expect(await screen.findByText("Return people to the page they came from. Add tests.")).toBeInTheDocument();
    expect(screen.getByText("pnpm lint")).toBeInTheDocument();
    expect(screen.getByText("pnpm test auth")).toBeInTheDocument();
    expect(screen.getByText("pass")).toBeInTheDocument();
    expect(screen.getByText("fail")).toBeInTheDocument();
    expect(screen.getAllByText("exit 0")).not.toHaveLength(0);
    expect(screen.getByText("4 s")).toBeInTheDocument();
    expect(screen.getByText("The changed files follow the style rules.")).toBeInTheDocument();
    expect(screen.getByText("The mobile app’s redirect: no check covers it.")).toBeInTheDocument();
    expect(screen.getAllByText("a41c9e2").length).toBeGreaterThan(0);
    expect(screen.getByText("#142")).toBeInTheDocument();
    expect(screen.getByText("crew/task-redirect")).toBeInTheDocument();
    expect(screen.queryByTitle("The Flow read carries the brief's purpose only")).toBeNull();
    expect(screen.queryByTitle("The Flow read carries counts, never a check")).toBeNull();
    // The read still lacks the plan, the pull request URL and head, the signed proof and the other device: those stay Planned.
    expect(screen.getByTitle("Nothing stores a task's plan steps")).toBeInTheDocument();
    expect(screen.getByTitle("The read has no pull request URL or head")).toBeInTheDocument();
    expect(screen.getByTitle("Signing the result is not in the beta")).toBeInTheDocument();
    expect(screen.getByTitle("The page is built from this device's records")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Verify/ })).toBeDisabled();
  });

  it("says a value was hidden and the brief was cut beside the prompt", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(runPage({ prompt: { text: "A short brief.", hidden: true, truncated: true } })))));
    render(<RunPage detail={taskDetail()} pull={null} />);
    expect(await screen.findByText("A short brief.")).toBeInTheDocument();
    expect(screen.getByText(/A value was hidden because it looked like a key/)).toHaveTextContent("cut at this page’s length");
  });

  it("keeps the Flow-based page when the run read fails, with an error and Retry", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer({ code: "internal", message: "the run could not be read" }, 500))));
    render(<RunPage detail={taskDetail()} pull={142} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Can’t read this run’s details");
    expect(screen.getByRole("alert")).toHaveTextContent("the run could not be read");
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Use next in the auth redirect" })).toBeInTheDocument();
    expect(screen.getByText("3 of 3 passed")).toBeInTheDocument();
  });

  it("treats a 404 as no run read and keeps today's Planned marks exactly", async () => {
    render(<RunPage detail={taskDetail()} pull={142} />);
    await waitFor(() => expect(fetch).toHaveBeenCalled());
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByTitle("The Flow read carries the brief's purpose only")).toBeInTheDocument();
    expect(screen.getByTitle("The Flow read carries counts, never a check")).toBeInTheDocument();
    expect(screen.queryByText("Return people to the page they came from. Add tests.")).toBeNull();
  });

  it("shows a withheld check's outcome, exit code and duration while its command waits for the run to finish", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(answer(runningRunPage()))));
    render(<RunPage detail={taskDetail({ row: { ...taskDetail().row, state: "verifying" } })} pull={null} />);
    expect(await screen.findAllByText(WITHHELD)).toHaveLength(4);
    expect(screen.getByText("pass")).toBeInTheDocument();
    expect(screen.getByText("fail")).toBeInTheDocument();
    expect(screen.getAllByText("4 s").length).toBeGreaterThan(0);
    expect(screen.getAllByText("22 s").length).toBeGreaterThan(0);
  });
});
