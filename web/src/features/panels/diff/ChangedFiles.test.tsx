import { render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import type { FileDiff } from "../../../api/types";
import { ChangedFiles } from "./ChangedFiles";

const file = (path: string, additions = 1): FileDiff => ({ path, status: "modified", additions, deletions: 0, patch: "" });
const repo = (id: string, files?: string[]) => ({ repo_id: id, name: id, branch: "fix/login", added: 4, deleted: 3, trusted: true, files });
const draw = (files: FileDiff[], repos?: ReturnType<typeof repo>[] | null) =>
  render(<ChangedFiles files={files} more={0} selected={null} filter="" onFilter={() => {}} onPick={() => {}} onMore={() => {}} repos={repos} />);

it("is one flat list for a thread with no repositories, as before", () => {
  draw([file("a.ts"), file("b.ts")]);
  expect(screen.queryByRole("heading")).toBeNull();
  expect(screen.getAllByRole("button", { name: /Modified/ })).toHaveLength(2);
});

it("is one flat list when the service does not say which repository a file belongs to", () => {
  draw([file("a.ts")], [repo("engineering"), repo("planner")]);
  expect(screen.queryByRole("heading")).toBeNull();
});

it("groups the files by repository, each group with its branch and counts", () => {
  draw([file("auth/redirect.ts", 2), file("calendar/feed.ts", 3), file("README.md")], [repo("engineering", ["auth/redirect.ts"]), repo("planner", ["calendar/feed.ts"])]);
  const groups = screen.getAllByRole("group").filter((group) => group.classList.contains("fl-repo"));
  expect(groups.map((group) => group.getAttribute("aria-label"))).toEqual(["engineering changes", "planner changes", "Other changed files"]);
  const first = groups[0]!;
  expect(within(first).getByRole("heading", { name: /engineering/ })).toBeVisible();
  expect(within(first).getByText("fix/login")).toBeVisible();
  expect(within(first).getByText("+4 −3")).toBeVisible();
  expect(within(first).getAllByRole("button")).toHaveLength(1);
  expect(within(groups[1]!).getByRole("button", { name: /calendar\/feed.ts/ })).toBeVisible();
});

it("shows hostile repository names and branches as text", () => {
  draw([file("a.ts")], [{ ...repo("<img src=x onerror=alert(1)>", ["a.ts"]), branch: "<b>x</b>" }]);
  expect(screen.getByText("<b>x</b>")).toBeVisible();
  expect(document.querySelector("img")).toBeNull();
  expect(document.querySelector("b")).toBeNull();
});
