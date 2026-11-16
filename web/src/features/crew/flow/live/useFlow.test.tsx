import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { MOCK_CREW } from "../../../../../mock/flow";
import { installMockFetch } from "../../../../test-support";
import { useFlowTasks } from "./useFlow";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

const nextPages = () => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes("offset=50"));

describe("useFlowTasks, the next page", () => {
  it("sends one read when showMore is called twice before the first answer, whatever a button does", async () => {
    mock.reset("crew-flow-many");
    const real = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (String(input).includes("offset=50")) await new Promise((resolve) => setTimeout(resolve, 60));
      return real(input, init);
    });
    const { result } = renderHook(() => useFlowTasks({ crew: MOCK_CREW, filter: "all", q: "", engine: null }));
    await waitFor(() => expect(result.current.rows).toHaveLength(50));
    // Two calls in one turn: no render separates them, so only the hook's own guard can stop the second.
    await act(async () => {
      void result.current.showMore();
      void result.current.showMore();
    });
    await waitFor(() => expect(result.current.rows).toHaveLength(60));
    expect(nextPages()).toHaveLength(1);
  });
});
