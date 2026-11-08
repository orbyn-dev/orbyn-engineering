import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { UsageReport } from "../../api/types";
import { useUsageReport } from "./useUsageReport";

const report = (scannedAt: string) => ({ host_id: "h", scanned_at: scannedAt, engines: [] }) as unknown as UsageReport;

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it("shows the last report for a period at once on revisit, then the fresh one", async () => {
  const read = vi.spyOn(api, "usage");
  read.mockImplementationOnce(() => new Promise((resolve) => setTimeout(() => resolve(report("first")), 5000)));
  const first = renderHook(() => useUsageReport(7));
  expect(first.result.current.data).toBeNull();
  await act(() => vi.advanceTimersByTimeAsync(5000));
  expect(first.result.current.data?.scanned_at).toBe("first");
  first.unmount();

  read.mockImplementationOnce(() => new Promise((resolve) => setTimeout(() => resolve(report("second")), 5000)));
  const revisit = renderHook(() => useUsageReport(7));
  expect(revisit.result.current).toMatchObject({ pending: true });
  expect(revisit.result.current.data?.scanned_at).toBe("first");
  await act(() => vi.advanceTimersByTimeAsync(5000));
  expect(revisit.result.current.data?.scanned_at).toBe("second");
  expect(revisit.result.current.pending).toBe(false);

  // Another period has nothing kept, so it starts empty rather than showing this one.
  read.mockImplementationOnce(() => new Promise(() => {}));
  const other = renderHook(() => useUsageReport(30));
  expect(other.result.current.data).toBeNull();
});
