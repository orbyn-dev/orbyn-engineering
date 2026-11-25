import { render, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { installMockFetch } from "../test-support";
import { useDesktopApp } from "../features/crew/autopilot/approvals/presence";
import { DesktopApp } from "../features/settings/sections/DesktopApp";
import { api } from "./client";
import { shellChanged } from "./shell";

afterEach(() => vi.restoreAllMocks());

/**
 * The fresh flag reaches the client from the call sites that need it: a shell-changed event is
 * not allowed to join a shell read sent before the change. The unit tests above call the helper
 * directly; these cover the wiring a refactor could silently drop.
 */
describe("shell-changed reads pass fresh through", () => {
  it("presence's shell-changed read asks the client for a fresh one", async () => {
    installMockFetch();
    const shell = vi.spyOn(api, "shell");
    renderHook(() => useDesktopApp());
    await waitFor(() => expect(shell).toHaveBeenCalled());
    expect(shell.mock.calls.some((call) => call[1] === false)).toBe(true);
    shell.mockClear();
    shellChanged();
    await waitFor(() => expect(shell.mock.calls.some((call) => call[1] === true)).toBe(true));
  });

  it("the Settings desktop page's shell-changed reload asks the client for a fresh one", async () => {
    installMockFetch();
    const shell = vi.spyOn(api, "shell");
    render(<DesktopApp />);
    await waitFor(() => expect(shell).toHaveBeenCalled());
    expect(shell.mock.calls.some((call) => call[1] === false)).toBe(true);
    shell.mockClear();
    shellChanged();
    await waitFor(() => expect(shell.mock.calls.some((call) => call[1] === true)).toBe(true));
  });
});
