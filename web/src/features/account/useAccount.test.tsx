import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { dataChanged } from "../../api/shell";
import { startDataRefresh } from "../../lib/dataRefresh";
import { installMockFetch } from "../../test-support";
import { accountApi } from "./api";
import { announceAccountChange } from "./changes";
import { useAccount } from "./useAccount";

afterEach(() => vi.restoreAllMocks());

/**
 * The account changed, so the read must not join one sent before the change: the service event
 * and this page's own change both ask the client for a fresh read.
 */
describe("account-change reads pass fresh through", () => {
  it("a data_changed account event asks for a fresh read", async () => {
    installMockFetch();
    // The app's shell subscribes its screens to the service's change stream once, at boot.
    const stop = startDataRefresh();
    try {
      const view = vi.spyOn(accountApi, "view");
      renderHook(() => useAccount());
      await waitFor(() => expect(view).toHaveBeenCalled());
      expect(view.mock.calls.some((call) => call[1] === false)).toBe(true);
      view.mockClear();
      dataChanged("account");
      // The burst is coalesced for a moment before the read runs.
      await waitFor(() => expect(view.mock.calls.some((call) => call[1] === true)).toBe(true), { timeout: 2000 });
    } finally {
      stop();
    }
  });

  it("this page's own account change asks for a fresh read", async () => {
    installMockFetch();
    const view = vi.spyOn(accountApi, "view");
    renderHook(() => useAccount());
    await waitFor(() => expect(view).toHaveBeenCalled());
    view.mockClear();
    announceAccountChange();
    await waitFor(() => expect(view.mock.calls.some((call) => call[1] === true)).toBe(true));
  });
});
