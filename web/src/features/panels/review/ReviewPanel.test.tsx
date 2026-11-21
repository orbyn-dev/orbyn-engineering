import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Run, Thread } from "../../../api/types";
import { ApiFailure, api } from "../../../api/client";
import type { ComposerChip, ReviewChip } from "../../../product/composer/composerContext";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { ReviewPanel } from "./ReviewPanel";

const thread = { id: "t1", cwd: "/r", engine: "codex", worktree: { path: "/r", branch: "fix/login", base: "main" }, active_run: null } as unknown as Thread;
const run = { id: "run1", thread_id: "t1", state: "running" } as unknown as Run;
const draft = (id: string, path: string, line: number, comment: string, over: Partial<ReviewChip> = {}): ReviewChip =>
  ({ id, kind: "review", path, line, comment, excerpt: `lines of ${id}`, sourceRevision: "r1", sourceCwd: "/r", ...over });
const put = (chips: ComposerChip[], sent: ComposerChip[] = []) => useComposerChips.setState({ chips: { t1: chips }, sent: { t1: sent }, reviewDrafts: {} });

beforeEach(() => {
  useComposerChips.setState({ chips: {}, sent: {}, reviewDrafts: {} });
  useUi.setState({ drafts: {} });
  useWorkspace.setState({ threads: { t1: thread } });
  vi.spyOn(api, "thread").mockResolvedValue(thread);
});
afterEach(() => vi.restoreAllMocks());

const open = async () => {
  fireEvent.click(screen.getByRole("button", { name: "Send all to engine" }));
  return await screen.findByRole("dialog");
};

describe("Review and comments tab", () => {
  it("says how to start when there are no comments", () => {
    render(<ReviewPanel threadId="t1" />);
    expect(screen.getByText("No comments yet")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Send all to engine" })).toBeNull();
  });

  it("lists drafts from files and diffs together and marks an out-of-date one", () => {
    put([draft("a", "auth/redirect.ts", 15, "Does inside() reject //evil.com?"), draft("b", "auth/redirect.test.ts", 8, "Add a case.", { stale: true })]);
    render(<ReviewPanel threadId="t1" />);
    expect(screen.getByText("Drafts · 2")).toBeInTheDocument();
    expect(screen.getByText("auth/redirect.ts:15")).toBeInTheDocument();
    expect(screen.getAllByText("Out of date")).toHaveLength(1);
  });

  it("shows comment text as text, never as markup", () => {
    put([draft("a", "a.ts", 1, "<img src=x onerror=alert(1)> <b>bold</b>")]);
    const { container } = render(<ReviewPanel threadId="t1" />);
    expect(container.querySelector("img, b")).toBeNull();
    expect(screen.getByText("<img src=x onerror=alert(1)> <b>bold</b>")).toBeInTheDocument();
  });

  it("edits and deletes a draft", () => {
    put([draft("a", "a.ts", 1, "old"), draft("b", "b.ts", 2, "keep")]);
    render(<ReviewPanel threadId="t1" />);
    fireEvent.click(screen.getAllByRole("button", { name: "Edit comment" })[0]!);
    fireEvent.change(screen.getByRole("textbox", { name: "Comment" }), { target: { value: "new text" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect((useComposerChips.getState().chips.t1![0] as ReviewChip).comment).toBe("new text");
    fireEvent.click(screen.getAllByRole("button", { name: "Delete comment" })[0]!);
    expect(useComposerChips.getState().chips.t1!.map((chip) => chip.id)).toEqual(["b"]);
  });

  it("never sends on open: the confirm shows the one message and Cancel leaves every draft", async () => {
    const start = vi.spyOn(api, "startRun").mockResolvedValue(run);
    put([draft("a", "auth/redirect.ts", 15, "first"), draft("b", "auth/redirect.test.ts", 8, "second")]);
    render(<ReviewPanel threadId="t1" />);
    const dialog = await open();
    expect(within(dialog).getByText("2 comments in 2 files")).toBeInTheDocument();
    expect(within(dialog).getByText(/^Review comments on fix\/login \(2\)/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(start).not.toHaveBeenCalled();
    expect(useComposerChips.getState().chips.t1).toHaveLength(2);
  });

  it("sends all drafts as ONE message in written order, then moves them to Open", async () => {
    const start = vi.spyOn(api, "startRun").mockResolvedValue(run);
    put([draft("a", "z.ts", 15, "first"), draft("b", "a.ts", 8, "second"), draft("c", "z.ts", 2, "third")]);
    render(<ReviewPanel threadId="t1" />);
    const dialog = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Send 3 comments" }));
    await waitFor(() => expect(useComposerChips.getState().chips.t1).toEqual([]));
    expect(start).toHaveBeenCalledTimes(1);
    const prompt = start.mock.calls[0]![1];
    expect(prompt.startsWith("Review comments on fix/login (3)")).toBe(true);
    expect(prompt.indexOf("first")).toBeLessThan(prompt.indexOf("second"));
    expect(prompt.indexOf("second")).toBeLessThan(prompt.indexOf("third"));
    expect(useComposerChips.getState().sent.t1).toHaveLength(3);
    expect(screen.getByText("Open · 3")).toBeInTheDocument();
  });

  it("keeps every draft when the send fails, and says so", async () => {
    vi.spyOn(api, "startRun").mockRejectedValue(new ApiFailure("offline", "The service is offline.", 503));
    put([draft("a", "a.ts", 1, "x"), draft("b", "b.ts", 2, "y")]);
    render(<ReviewPanel threadId="t1" />);
    const dialog = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Send 2 comments" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("The service is offline.");
    expect(useComposerChips.getState().chips.t1).toHaveLength(2);
    expect(useComposerChips.getState().sent.t1 ?? []).toHaveLength(0);
  });

  it("keeps the drafts until the service accepts, and a double click sends once", async () => {
    let accept!: (value: Run) => void;
    const start = vi.spyOn(api, "startRun").mockReturnValue(new Promise<Run>((done) => { accept = done; }));
    put([draft("a", "a.ts", 1, "x"), draft("b", "b.ts", 2, "y")]);
    render(<ReviewPanel threadId="t1" />);
    const dialog = await open();
    const send = within(dialog).getByRole("button", { name: "Send 2 comments" });
    fireEvent.click(send);
    fireEvent.click(send);
    expect(start).toHaveBeenCalledTimes(1);
    expect(useComposerChips.getState().chips.t1).toHaveLength(2);
    await act(async () => { accept(run); });
    await waitFor(() => expect(useComposerChips.getState().chips.t1).toEqual([]));
    expect(start).toHaveBeenCalledTimes(1);
  });

  it("queues the one message while a run is going", async () => {
    const busy = { ...thread, active_run: run } as Thread;
    useWorkspace.setState({ threads: { t1: busy } });
    const start = vi.spyOn(api, "startRun").mockResolvedValue(run);
    const enqueue = vi.spyOn(api, "enqueue").mockResolvedValue({} as never);
    put([draft("a", "a.ts", 1, "x"), draft("b", "b.ts", 2, "y")]);
    render(<ReviewPanel threadId="t1" />);
    const dialog = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Send 2 comments" }));
    await waitFor(() => expect(enqueue).toHaveBeenCalledTimes(1));
    expect(start).not.toHaveBeenCalled();
    expect(enqueue.mock.calls[0]![1]).toContain("(2)");
  });

  it("leaves other chips in the composer when the drafts are sent", async () => {
    vi.spyOn(api, "startRun").mockResolvedValue(run);
    const file: ComposerChip = { id: "f", kind: "file", path: "x.ts", line: null, excerpt: null, language: null };
    put([draft("a", "a.ts", 1, "x"), file]);
    render(<ReviewPanel threadId="t1" />);
    const dialog = await open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Send 1 comment" }));
    await waitFor(() => expect(useComposerChips.getState().chips.t1).toEqual([file]));
  });
});
