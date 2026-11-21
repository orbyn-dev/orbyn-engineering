import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUi } from "../../../product/state/ui";
import { HOSTILE, taskDetail } from "./fixture";
import { RunPage } from "./RunPage";
import { REVOKE_AFTER_MS } from "./download";

const created: Blob[] = [];
let clicks: HTMLAnchorElement[] = [];
beforeEach(() => {
  created.length = 0;
  clicks = [];
  vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("no network in this test"))));
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
    expect(fetch).not.toHaveBeenCalled();
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
    expect(fetch).not.toHaveBeenCalled();
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
});
