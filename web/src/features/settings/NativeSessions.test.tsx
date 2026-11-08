import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ApiFailure, request } from "../../api/client";
import type { EngineStatus } from "../../api/types";
import { NativeSessions } from "./NativeSessions";

vi.mock("../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

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

it("stops asking after a few tries", async () => {
  vi.mocked(request).mockRejectedValue(new ApiFailure("engine_busy", "The agent is busy.", 409));
  render(<NativeSessions engine={engine} />);
  await act(async () => {
    await vi.advanceTimersByTimeAsync(60_000);
  });
  expect(vi.mocked(request)).toHaveBeenCalledTimes(7);
  expect(screen.queryByText("Native sessions")).toBeNull();
});
