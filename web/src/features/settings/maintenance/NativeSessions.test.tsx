import { act, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { holdFetch, wire } from "../../../wire.test-support";
import { NativeSessions } from "./NativeSessions";

let mock: MockApi;
/** The mock service's ACP agent: it lists one saved conversation and says it can load and delete it. */
let engine: EngineStatus;
const capabilities = () => `/api/engines/${engine.id}/native-sessions/capabilities`;
const asked = () => wire().filter((call) => call.path === capabilities());

beforeEach(() => {
  vi.useFakeTimers();
  mock = installMockFetch();
  engine = (mock.handle("GET", "/api/engines").body as EngineStatus[]).find((entry) => entry.id === "gemini-acp")!;
});
afterEach(() => vi.useRealTimers());

/** The panel with a project to list sessions in. */
async function panel() {
  await useWorkspace.getState().refreshProjects();
  const view = render(<NativeSessions engine={engine} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
  return view;
}

it("asks again when the agent turns the capability read away as busy, and then offers the sessions", async () => {
  mock.fail(capabilities(), 1, 409, "engine_busy", "The agent is busy.");
  render(<NativeSessions engine={engine} />);
  expect(screen.queryByText("Native sessions")).toBeNull();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
  });
  expect(asked()).toHaveLength(2);
  expect(screen.getByText("Native sessions")).toBeInTheDocument();
});

it("says so, with a Retry, when the agent is still busy after the last try, and Retry asks again", async () => {
  mock.fail(capabilities(), 7, 409, "engine_busy", "The agent is busy.");
  render(<NativeSessions engine={engine} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(asked()).toHaveLength(7);
  expect(screen.getByRole("alert")).toHaveTextContent("Couldn’t check saved sessions");
  expect(screen.getByRole("alert")).toHaveTextContent("The agent is busy. Try again in a moment.");
  await act(async () => {
    screen.getByRole("button", { name: "Retry" }).click();
    await vi.advanceTimersByTimeAsync(0);
  });
  expect(screen.getByText("Native sessions")).toBeInTheDocument();
  expect(screen.queryByRole("alert")).toBeNull();
});

it.each([
  ["engine_unavailable", 502],
  ["unsupported", 422],
  ["unauthorized", 401],
])("does not ask again for a final answer (%s)", async (code, status) => {
  mock.fail(capabilities(), 99, status, code, "No.");
  render(<NativeSessions engine={engine} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(asked()).toHaveLength(1);
  expect(screen.queryByText("Native sessions")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("does not ask again after a network failure", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("Failed to fetch"); }));
  render(<NativeSessions engine={engine} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(asked()).toHaveLength(1);
});

it("cancels a list the person walks away from, so the next list is not refused as busy", async () => {
  // The agent takes its time: the list is still open when the panel goes away.
  const slow = holdFetch("GET", /\/native-sessions\?/);
  const view = await panel();
  await act(async () => {
    screen.getByRole("button", { name: "List sessions" }).click();
    await vi.advanceTimersByTimeAsync(0);
  });
  expect(slow.signals.map((signal) => signal.aborted)).toEqual([false]);
  view.unmount();
  expect(slow.signals.map((signal) => signal.aborted)).toEqual([true]);
});

async function showedSessions(held: { method: string; path: RegExp }) {
  // Import and delete take their time: the agent is still working when the panel goes away.
  const slow = holdFetch(held.method, held.path);
  const view = await panel();
  await act(async () => {
    screen.getByRole("button", { name: "List sessions" }).click();
    await vi.advanceTimersByTimeAsync(0);
  });
  expect(screen.getByText("First chat")).toBeInTheDocument();
  return { view, slow };
}

it("cancels an import the person walks away from, so the agent is freed", async () => {
  const { view, slow } = await showedSessions({ method: "POST", path: /\/native-sessions\/import$/ });
  await act(async () => {
    screen.getByRole("button", { name: "Import" }).click();
    await vi.advanceTimersByTimeAsync(0);
  });
  expect(slow.signals.map((signal) => signal.aborted)).toEqual([false]);
  view.unmount();
  expect(slow.signals.map((signal) => signal.aborted)).toEqual([true]);
});

it("cancels a delete the person walks away from, so the agent is freed", async () => {
  const { view, slow } = await showedSessions({ method: "POST", path: /\/native-sessions\/delete$/ });
  await act(async () => {
    screen.getByRole("button", { name: "Delete" }).click();
    await vi.advanceTimersByTimeAsync(0);
  });
  await act(async () => {
    within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }).click();
    await vi.advanceTimersByTimeAsync(0);
  });
  expect(slow.signals.map((signal) => signal.aborted)).toEqual([false]);
  view.unmount();
  expect(slow.signals.map((signal) => signal.aborted)).toEqual([true]);
});
