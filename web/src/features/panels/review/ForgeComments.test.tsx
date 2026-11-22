import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../api/client";
import type { PullComment, PullRequest, PullThread } from "../../../api/types";
import { useUi } from "../../../product/state/ui";
import { ForgeComments } from "./ForgeComments";
import type { ForgeRead } from "./useForgeComments";

const pull: PullRequest = {
  number: 42,
  title: "Add approval timeouts",
  url: "https://example.com/pull/42",
  state: "open",
  head: "approval-timeout",
  base: "main",
  author: "dev",
  draft: false,
  updated_at: null,
  labels: [],
  review_state: null,
  checks_state: null,
};

const comment = (body: string, author: string | null = "sam"): PullComment => ({
  id: `c-${body}`,
  kind: "review",
  author,
  body,
  created_at: null,
  path: null,
  line: null,
});

const thread = (id: string, over: Partial<PullThread> = {}): PullThread => ({
  id,
  resolved: false,
  path: "auth/redirect.ts",
  line: 15,
  comments: [comment(`body ${id}`)],
  ...over,
});

const ready = (threads: PullThread[], comments: PullComment[] = []): ForgeRead => ({
  status: "ready",
  data: [{ pull, conversation: { comments, threads } }],
  error: null,
  refreshing: false,
  reload: () => undefined,
});

const show = (read: ForgeRead, file = "") =>
  render(<ForgeComments threadId="t1" cwd="/r" file={file} read={read} onRetry={() => undefined} />);

afterEach(() => vi.restoreAllMocks());

describe("Forge comments", () => {
  it("lists unresolved threads on the pull request and keeps resolved ones behind a disclosure", () => {
    show(ready([
      thread("open", { comments: [comment("first", "sam"), comment("second", "riley"), comment("third", "sam")] }),
      thread("done", { resolved: true, path: "auth/session.ts", line: 4, comments: [comment("Fixed the redirect.", "riley")] }),
    ]));
    expect(screen.getByText("On the pull request · 1")).toBeInTheDocument();
    expect(screen.getByText("Resolved · 1")).toBeInTheDocument();
    expect(screen.getByText("3 comments")).toBeInTheDocument();
    expect(screen.getByText("sam")).toBeInTheDocument();
    const disclosure = screen.getByRole("button", { name: "Resolved · 1" });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Fixed the redirect.")).toBeNull();
  });

  it("opens the resolved threads when the disclosure is pressed", () => {
    show(ready([thread("done", { resolved: true, path: "auth/session.ts", line: 4, comments: [comment("Fixed the redirect.")] })]));
    fireEvent.click(screen.getByRole("button", { name: "Resolved · 1" }));
    expect(screen.getByRole("button", { name: "Resolved · 1" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Fixed the redirect.")).toBeInTheDocument();
  });

  it("jumps to the file and line of a forge comment", () => {
    const reveal = vi.spyOn(useUi.getState(), "revealFile");
    show(ready([thread("open")]));
    fireEvent.click(screen.getByRole("button", { name: "Open auth/redirect.ts:15" }));
    expect(reveal).toHaveBeenCalledWith("auth/redirect.ts", 15, { threadId: "t1", cwd: "/r" });
  });

  it("shows a comment body as text, never as markup", () => {
    const body = "<img src=x onerror=alert(1)> <b>bold</b>";
    const { container } = render(<ForgeComments threadId="t1" cwd="/r" file="" read={ready([thread("open", { comments: [comment(body)] })])} onRetry={() => undefined} />);
    expect(container.querySelector("img, b")).toBeNull();
    expect(screen.getByText(body)).toBeInTheDocument();
  });

  it("cuts a long body at 400 characters and shows the rest behind Show more", () => {
    const body = "a".repeat(400) + "TAIL";
    show(ready([thread("long", { comments: [comment(body)] })]));
    expect(screen.getByText("a".repeat(400))).toBeInTheDocument();
    expect(screen.queryByText(body)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show more" }));
    expect(screen.getByText(body)).toBeInTheDocument();
  });

  it("lists a thread without a path or line under On the pull request with no jump button", () => {
    show(ready([thread("general", { path: null, line: null, comments: [comment("A general note", "riley")] })]));
    expect(screen.getByText("A general note")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Open / })).toBeNull();
  });

  it("draws nothing from issue-kind general comments", () => {
    show(ready([], [comment("general chatter")]));
    expect(screen.queryByText("general chatter")).toBeNull();
    expect(screen.queryByText(/On the pull request/)).toBeNull();
    expect(screen.queryByText(/Resolved/)).toBeNull();
  });

  it("filters the forge comments by file", () => {
    show(ready([
      thread("a", { path: "a.ts", line: 1, comments: [comment("Comment on a")] }),
      thread("b", { path: "b.ts", line: 2, comments: [comment("Comment on b")] }),
    ]), "b.ts");
    expect(screen.queryByText("Comment on a")).toBeNull();
    expect(screen.getByText("Comment on b")).toBeInTheDocument();
  });

  it("says quietly that the pull request comments could not be read and offers Try again", () => {
    const retry = vi.fn();
    render(<ForgeComments threadId="t1" cwd="/r" file="" read={{ status: "error", data: null, error: new ApiFailure("offline", "The service is offline.", 503), refreshing: false, reload: () => undefined }} onRetry={retry} />);
    expect(screen.getByText("Pull request comments could not be read.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(retry).toHaveBeenCalledTimes(1);
  });
});
