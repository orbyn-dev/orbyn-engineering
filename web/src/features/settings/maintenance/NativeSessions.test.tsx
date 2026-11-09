import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ApiFailure, request } from "../../../api/client";
import type { EngineStatus } from "../../../api/types";
import { NativeSessions } from "./NativeSessions";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

const engine = { id: "agent", tier: "acp", installed: true } as unknown as EngineStatus;
const caps = { can_list: true, can_load: true, can_resume: false, can_delete: true };

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(request).mockReset();
});
afterEach(() => vi.useRealTimers());

it("asks again when the agent turns the capability read away as busy, and then offers the sessions", async () => {
  vi.mocked(request)
    .mockRejectedValueOnce(new ApiFailure("engine_busy", "The agent is busy.", 409))
    .mockResolvedValue(caps as never);
  render(<NativeSessions engine={engine} />);
  expect(screen.queryByText("Native sessions")).toBeNull();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(1000);
  });
  expect(vi.mocked(request)).toHaveBeenCalledTimes(2);
  expect(screen.getByText("Native sessions")).toBeInTheDocument();
});

it("says so, with a Retry, when the agent is still busy after the last try, and Retry asks again", async () => {
  vi.mocked(request).mockRejectedValue(new ApiFailure("engine_busy", "The agent is busy.", 409));
  render(<NativeSessions engine={engine} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(vi.mocked(request)).toHaveBeenCalledTimes(7);
  expect(screen.getByRole("alert")).toHaveTextContent("Couldn’t check saved sessions");
  expect(screen.getByRole("alert")).toHaveTextContent("The agent is busy. Try again in a moment.");
  vi.mocked(request).mockResolvedValue(caps as never);
  await act(async () => {
    screen.getByRole("button", { name: "Retry" }).click();
    await vi.advanceTimersByTimeAsync(0);
  });
  expect(screen.getByText("Native sessions")).toBeInTheDocument();
  expect(screen.queryByRole("alert")).toBeNull();
});

it.each([
  ["engine_unavailable", 503],
  ["unsupported", 400],
  ["unauthorized", 401],
])("does not ask again for a final answer (%s)", async (code, status) => {
  vi.mocked(request).mockRejectedValue(new ApiFailure(code, "No.", status));
  render(<NativeSessions engine={engine} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(vi.mocked(request)).toHaveBeenCalledTimes(1);
  expect(screen.queryByText("Native sessions")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("does not ask again after a network failure", async () => {
  vi.mocked(request).mockRejectedValue(new TypeError("Failed to fetch"));
  render(<NativeSessions engine={engine} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(vi.mocked(request)).toHaveBeenCalledTimes(1);
});
