import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ExplorerRow } from "./ExplorerRow";
import type { Row } from "./model";

afterEach(() => vi.useRealTimers());

const loading: Row = { key: "loading:x", kind: "loading", depth: 3, projectId: "p", label: "Loading files", level: { worktreeId: "w", path: "" } };
const handlers = { onActivate: () => undefined, onMenu: () => undefined, onFocus: () => undefined };

it("shows nothing for a read that finishes within 400 ms, then quiet placeholder rows with no motion", () => {
  vi.useFakeTimers();
  const { container } = render(<ExplorerRow row={loading} selected={false} focusable={false} menuOpen={false} draggable={false} handlers={handlers} />);
  // The label is read at once; nothing is drawn yet.
  expect(screen.getByRole("status", { name: "Loading files" })).toBeInTheDocument();
  expect(container.querySelector(".x2sk")).toBeNull();
  act(() => void vi.advanceTimersByTime(399));
  expect(container.querySelector(".x2sk")).toBeNull();
  act(() => void vi.advanceTimersByTime(1));
  expect(container.querySelectorAll(".x2sk")).toHaveLength(2);
});
