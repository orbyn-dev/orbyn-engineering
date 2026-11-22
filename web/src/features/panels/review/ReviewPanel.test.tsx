import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PullComment, PullConversation, PullRequest, PullThread, Run, Thread } from "../../../api/types";
import { ApiFailure, api } from "../../../api/client";
import { pullsApi } from "../../../api/pulls";
import type { ComposerChip, ReviewChip } from "../../../product/composer/composerContext";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { ReviewPanel } from "./ReviewPanel";

const thread = { id: "t1", project_id: "p-app", cwd: "/r", engine: "codex", worktree: { path: "/r", branch: "fix/login", base: "main" }, active_run: null } as unknown as Thread;
const run = { id: "run1", thread_id: "t1", state: "running" } as unknown as Run;
const draft = (id: string, path: string, line: number, comment: string, over: Partial<ReviewChip> = {}): ReviewChip =>
  ({ id, kind: "review", path, line, comment, excerpt: `lines of ${id}`, sourceRevision: "r1", sourceCwd: "/r", ...over });
const put = (chips: ComposerChip[], sent: ComposerChip[] = []) => useComposerChips.setState({ chips: { t1: chips }, sent: { t1: sent }, reviewDrafts: {} });

const forgePull = (number = 42, state = "open"): PullRequest => ({
  number, title: `Pull ${number}`, url: `https://example.com/pull/${number}`, state, head: `pull-${number}`, base: "main",
  author: "riley", draft: false, updated_at: null, labels: [], review_state: null, checks_state: null,
});
const forgeComment = (body: string, author: string | null = "sam"): PullComment =>
  ({ id: `c-${body}`, kind: "review", author, body, created_at: null, path: null, line: null });
const forgeThread = (id: string, over: Partial<PullThread> = {}): PullThread =>
  ({ id, resolved: false, path: "auth/redirect.ts", line: 15, comments: [forgeComment(`body ${id}`)], ...over });
const forgeConversation = (threads: PullThread[], comments: PullComment[] = []): PullConversation => ({ comments, threads });
const forge = (threads: PullThread[], comments: PullComment[] = []) => {
  vi.mocked(pullsApi.linked).mockResolvedValue([forgePull()]);
  return vi.mocked(pullsApi.conversation).mockResolvedValue(forgeConversation(threads, comments));
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => {
  useComposerChips.setState({ chips: {}, sent: {}, reviewDrafts: {} });
  useUi.setState({ drafts: {} });
  useWorkspace.setState({ threads: { t1: thread } });
  vi.spyOn(api, "thread").mockResolvedValue(thread);
  vi.spyOn(pullsApi, "linked").mockResolvedValue([]);
  vi.spyOn(pullsApi, "conversation").mockResolvedValue({ comments: [], threads: [] });
});
afterEach(() => vi.restoreAllMocks());

const open = async () => {
  fireEvent.click(screen.getByRole("button", { name: "Send all to engine" }));
  return await screen.findByRole("dialog");
};

describe("Review and comments tab", () => {
  it("says how to start when there are no comments", async () => {
    render(<ReviewPanel threadId="t1" />);
    expect(await screen.findByText("No comments yet")).toBeInTheDocument();
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

  it("shows the linked pull request's unresolved and resolved threads, the resolved ones closed until asked", async () => {
    forge([
      forgeThread("open", { comments: [forgeComment("Is inside() enough?", "sam")] }),
      forgeThread("done", { resolved: true, path: "auth/session.ts", line: 4, comments: [forgeComment("Fixed the redirect.", "riley")] }),
    ]);
    render(<ReviewPanel threadId="t1" />);
    expect(await screen.findByText("On the pull request · 1")).toBeInTheDocument();
    expect(screen.getByText("Resolved · 1")).toBeInTheDocument();
    expect(screen.getByText("Is inside() enough?")).toBeInTheDocument();
    expect(screen.queryByText("Fixed the redirect.")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Resolved · 1" }));
    expect(screen.getByText("Fixed the redirect.")).toBeInTheDocument();
  });

  it("jumps to the file and line of a forge comment", async () => {
    forge([forgeThread("open")]);
    const reveal = vi.spyOn(useUi.getState(), "revealFile");
    render(<ReviewPanel threadId="t1" />);
    fireEvent.click(await screen.findByRole("button", { name: "Open auth/redirect.ts:15" }));
    expect(reveal).toHaveBeenCalledWith("auth/redirect.ts", 15, { threadId: "t1", cwd: "/r" });
  });

  it("shows a forge comment body as text, never as markup", async () => {
    forge([forgeThread("open", { comments: [forgeComment("<img src=x onerror=alert(1)>", "sam")] })]);
    const { container } = render(<ReviewPanel threadId="t1" />);
    expect(await screen.findByText("<img src=x onerror=alert(1)>")).toBeInTheDocument();
    expect(container.querySelector("img")).toBeNull();
  });

  it("cuts a long forge comment at 400 characters and shows the rest behind Show more", async () => {
    const body = "a".repeat(400) + "TAIL";
    forge([forgeThread("open", { comments: [forgeComment(body)] })]);
    render(<ReviewPanel threadId="t1" />);
    expect(await screen.findByText("a".repeat(400))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show more" }));
    expect(screen.getByText(body)).toBeInTheDocument();
  });

  it("says quietly when the pull request comments could not be read, keeps the drafts and reads again", async () => {
    put([draft("a", "a.ts", 1, "a draft")]);
    const linked = vi.mocked(pullsApi.linked).mockRejectedValueOnce(new ApiFailure("offline", "The service is offline.", 503)).mockResolvedValue([forgePull()]);
    vi.mocked(pullsApi.conversation).mockResolvedValue(forgeConversation([forgeThread("open")]));
    render(<ReviewPanel threadId="t1" />);
    expect(await screen.findByText("Pull request comments could not be read.")).toBeInTheDocument();
    expect(screen.getByText("a draft")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("On the pull request · 1")).toBeInTheDocument();
    expect(linked).toHaveBeenCalledTimes(2);
    expect(screen.queryByText("Pull request comments could not be read.")).toBeNull();
  });

  it("filters the forge comments by the file filter", async () => {
    forge([
      forgeThread("a", { path: "a.ts", line: 1, comments: [forgeComment("Comment on a")] }),
      forgeThread("b", { path: "b.ts", line: 2, comments: [forgeComment("Comment on b")] }),
    ]);
    render(<ReviewPanel threadId="t1" />);
    expect(await screen.findByText("On the pull request · 2")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("combobox", { name: "Filter by file" }), { target: { value: "b.ts" } });
    expect(screen.queryByText("Comment on a")).toBeNull();
    expect(screen.getByText("Comment on b")).toBeInTheDocument();
  });

  it("ignores a slow older forge read after the thread changes", async () => {
    const other = { ...thread, id: "t2" } as Thread;
    useWorkspace.setState({ threads: { t1: thread, t2: other } });
    const older = deferred<PullRequest[]>();
    vi.mocked(pullsApi.linked).mockImplementation((id) => id === "t1" ? older.promise : Promise.resolve([forgePull(43)]));
    vi.mocked(pullsApi.conversation).mockImplementation(async (_projectId, number) => number === 43
      ? forgeConversation([forgeThread("new", { path: "new.ts", line: 2, comments: [forgeComment("new body")] })])
      : forgeConversation([forgeThread("old", { path: "old.ts", line: 3, comments: [forgeComment("old body")] })]));
    const view = render(<ReviewPanel threadId="t1" />);
    view.rerender(<ReviewPanel threadId="t2" />);
    expect(await screen.findByText("new body")).toBeInTheDocument();
    await act(async () => { older.resolve([forgePull(42)]); });
    expect(screen.queryByText("old body")).toBeNull();
    expect(screen.getByText("new body")).toBeInTheDocument();
  });

  it("draws no forge groups and asks for no comments when no pull request is linked", async () => {
    render(<ReviewPanel threadId="t1" />);
    expect(await screen.findByText("No comments yet")).toBeInTheDocument();
    expect(pullsApi.linked).toHaveBeenCalledTimes(1);
    expect(pullsApi.conversation).not.toHaveBeenCalled();
    expect(screen.queryByText(/On the pull request/)).toBeNull();
    expect(screen.queryByText(/Resolved/)).toBeNull();
  });

  it("waits for the forge read before saying there are no comments", async () => {
    const older = deferred<PullRequest[]>();
    vi.mocked(pullsApi.linked).mockReturnValue(older.promise);
    render(<ReviewPanel threadId="t1" />);
    expect(screen.queryByText("No comments yet")).toBeNull();
    await act(async () => { older.resolve([]); });
    expect(await screen.findByText("No comments yet")).toBeInTheDocument();
  });

  it("reads at most five linked pull requests, the open ones first", async () => {
    vi.mocked(pullsApi.linked).mockResolvedValue([forgePull(1, "closed"), forgePull(2), forgePull(3, "closed"), forgePull(4), forgePull(5), forgePull(6)]);
    render(<ReviewPanel threadId="t1" />);
    await waitFor(() => expect(pullsApi.conversation).toHaveBeenCalledTimes(5));
    expect(vi.mocked(pullsApi.conversation).mock.calls.map((call) => call[1])).toEqual([2, 4, 5, 6, 1]);
  });
});
