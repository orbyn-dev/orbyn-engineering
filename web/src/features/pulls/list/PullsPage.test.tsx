import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { pullsApi } from "../../../api/pulls";
import type { PullList } from "../../../api/types";
import { dataChanged } from "../../../api/shell";
import { COALESCE_MS, startDataRefresh } from "../../../lib/dataRefresh";
import { forgetKept } from "../../../lib/keptData";
import { MOTION } from "../../../lib/motion";
import { setViewportWidth } from "../../../test-support";
import { PullsPage } from "./PullsPage";

let finish!: (list: PullList) => void;

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(pullsApi, "list").mockReturnValue(new Promise<PullList>((resolve) => { finish = resolve; }));
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const skeletonRows = () => document.querySelectorAll(".pull-skel > li");

it("says Loading in the header, with the table header already there", () => {
  render(<PullsPage />);
  expect(screen.getByRole("status")).toHaveTextContent("Loading");
  expect(screen.getByText("Title and state")).toBeInTheDocument();
});

it("shows nothing under the header before 400 ms and 6 skeleton rows after", () => {
  render(<PullsPage />);
  act(() => void vi.advanceTimersByTime(MOTION.wait - 1));
  expect(skeletonRows()).toHaveLength(0);
  act(() => void vi.advanceTimersByTime(1));
  expect(skeletonRows()).toHaveLength(6);
});

it("removes the skeleton when the data arrives", async () => {
  render(<PullsPage />);
  act(() => void vi.advanceTimersByTime(MOTION.wait));
  expect(skeletonRows()).toHaveLength(6);
  await act(async () => finish({ pulls: [], failed: [] }));
  expect(skeletonRows()).toHaveLength(0);
});

it("keeps the desktop Filters popover at 700 px; only a phone gets the sheet", () => {
  setViewportWidth(700);
  render(<PullsPage />);
  fireEvent.click(screen.getByRole("button", { name: "Filters" }));
  expect(screen.getByRole("dialog", { name: "Filters" })).toBeInTheDocument();
  expect(document.querySelector(".pulls-sheet")).toBeNull();
  expect(screen.getByRole("combobox", { name: "Search people" })).toBeInTheDocument();
});

const entry = (number: number) => ({
  project_id: "p",
  project_name: "p",
  pull: { number, title: `Pull ${number}`, url: `https://github.com/o/r/pull/${number}`, state: "open", draft: false, labels: [] },
}) as unknown as PullList["pulls"][number];

it("reads again in the background when the service says pull requests changed, with no loading state", async () => {
  forgetKept();
  const stop = startDataRefresh();
  const list = vi.spyOn(pullsApi, "list").mockResolvedValue({ pulls: [entry(1)], failed: [] });
  render(<PullsPage />);
  await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
  expect(screen.getByText("Pull 1")).toBeInTheDocument();
  const before = list.mock.calls.length;
  list.mockResolvedValue({ pulls: [entry(1), entry(2)], failed: [] });
  dataChanged("pulls");
  await act(async () => void (await vi.advanceTimersByTimeAsync(COALESCE_MS)));
  expect(screen.getByText("Pull 2")).toBeInTheDocument();
  expect(screen.queryByText("Loading")).toBeNull();
  expect(screen.queryByText("Refreshing")).toBeNull();
  // A background read takes the slow lane: no foreground priority, and no forced read.
  const call = list.mock.calls[before] as unknown[];
  expect(call[3]).toBeUndefined();
  expect(call[4]).toBe(false);
  stop();
});

it("sends a real read when the person presses Refresh", async () => {
  forgetKept();
  const list = vi.spyOn(pullsApi, "list").mockResolvedValue({ pulls: [entry(1)], failed: [] });
  render(<PullsPage />);
  await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
  const before = list.mock.calls.length;
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await act(async () => void (await vi.advanceTimersByTimeAsync(0)));
  const call = list.mock.calls[before] as unknown[];
  expect(call[3]).toEqual({ priority: "foreground" });
  expect(call[4]).toBe(true);
});
