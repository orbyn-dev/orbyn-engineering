import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ExplorerRow } from "./ExplorerRow";
import type { Row } from "./model";

afterEach(() => vi.useRealTimers());

const loading: Row = { key: "loading:x", kind: "loading", depth: 3, projectId: "p", label: "Loading files", level: { worktreeId: "w", path: "" } };
const handlers = { onActivate: () => undefined, onMenu: () => undefined, onFocus: () => undefined };

it("shows nothing for a read that finishes within 400 ms, then quiet placeholder rows with no motion", () => {
  vi.useFakeTimers();
  const { container } = render(<ExplorerRow row={loading} selected={false} focusable={false} menuOpen={false} dragPath={null} handlers={handlers} />);
  // The label is read at once; nothing is drawn yet.
  expect(screen.getByRole("treeitem", { name: "Loading files" })).toBeInTheDocument();
  expect(container.querySelector(".x2sk")).toBeNull();
  act(() => void vi.advanceTimersByTime(399));
  expect(container.querySelector(".x2sk")).toBeNull();
  act(() => void vi.advanceTimersByTime(1));
  expect(container.querySelectorAll(".x2sk")).toHaveLength(2);
});

const file = (change: Row["change"]): Row => ({ key: "f", kind: "file", depth: 3, projectId: "p", label: "a.ts", path: "a.ts", change });

it("draws M A D R U at the right of a file", () => {
  for (const [change, letter] of [["modified", "M"], ["added", "A"], ["deleted", "D"], ["renamed", "R"], ["untracked", "U"]] as const) {
    const { unmount } = render(<ExplorerRow row={file(change)} selected={false} focusable={false} menuOpen={false} dragPath={null} handlers={handlers} />);
    expect(screen.getByText(letter)).toBeInTheDocument();
    unmount();
  }
});

it("draws a conflict as a bold ! in neutral ink on a tinted row, and a deleted file struck through", () => {
  const { unmount } = render(<ExplorerRow row={file("conflicted")} selected={false} focusable={false} menuOpen={false} dragPath={null} handlers={handlers} />);
  expect(screen.getByText("!")).toHaveClass("xg-bang");
  expect(screen.getByRole("treeitem")).toHaveClass("xg-c");
  expect(screen.getByRole("treeitem")).toHaveAccessibleName("a.ts, Conflicted");
  unmount();
  render(<ExplorerRow row={file("deleted")} selected={false} focusable={false} menuOpen={false} dragPath={null} handlers={handlers} />);
  expect(screen.getByRole("treeitem")).toHaveClass("xg-d");
  expect(screen.getByText("D")).toBeInTheDocument();
});
