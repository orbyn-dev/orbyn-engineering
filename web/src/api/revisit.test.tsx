import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useUsageReport } from "../features/usage/useUsageReport";
import type { UsageReport } from "./types";

const usage = vi.fn();
vi.mock("../features/usage/api", () => ({ api: { usage: (...args: unknown[]) => usage(...args), refreshUsage: vi.fn() } }));
import { forgetKept } from "../lib/keptData";
import { useAsync } from "../lib/useAsync";

/** Pull requests and Usage read through `useAsync(..., { keep: true })` and `useUsageReport`. */
describe("a revisited screen", () => {
  it("shows its last data at once, with no loading state, while it reads again", async () => {
    forgetKept();
    let answer: (value: string[]) => void = () => undefined;
    const load = () => new Promise<string[]>((resolve) => (answer = resolve));
    const first = renderHook(() => useAsync("pulls:open::", load, { keep: true }));
    expect(first.result.current.status).toBe("loading");
    await act(async () => answer(["#1"]));
    expect(first.result.current.status).toBe("ready");
    first.unmount();

    const again = renderHook(() => useAsync("pulls:open::", load, { keep: true }));
    expect(again.result.current.status).toBe("ready");
    expect(again.result.current.data).toEqual(["#1"]);
    expect(again.result.current.refreshing).toBe(true);
    await act(async () => answer(["#1", "#2"]));
    expect(again.result.current.data).toEqual(["#1", "#2"]);
  });

  it("shows Usage's last report at once on a revisit", async () => {
    forgetKept();
    let answer: (value: UsageReport) => void = () => undefined;
    usage.mockImplementation(() => new Promise<UsageReport>((resolve) => (answer = resolve)));
    const report = { marker: 1 } as unknown as UsageReport;
    const first = renderHook(() => useUsageReport(30));
    expect(first.result.current.data).toBeNull();
    await act(async () => answer(report));
    first.unmount();

    const again = renderHook(() => useUsageReport(30));
    expect(again.result.current.data).toBe(report);
  });
});
