import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api, request } from "../../../api/client";
import { GeneralSection } from "./GeneralSection";
import { scopedFixture } from "../rows/scopedPreferenceFixture.test-support";
vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

beforeEach(() => vi.mocked(request).mockReset());
afterEach(() => vi.restoreAllMocks());

/** The service on `platform`, with the saved quit mode the row would show. */
function serve(platform: string) {
  vi.spyOn(api, "info").mockResolvedValue({ version: "1", remote: false, platform, environment_label: "Test computer" });
  vi.mocked(request).mockResolvedValue(scopedFixture([
    { key: "quit_confirmation", value: "hold", default: "direct", source: "global", project_scoped: false },
  ]));
}

it("offers the quit confirmation on macOS, where ⌘Q quits the desktop app", async () => {
  serve("macos");
  render(<GeneralSection />);
  expect(await screen.findByRole("radiogroup", { name: "Quit confirmation" })).toBeVisible();
});

for (const platform of ["windows", "linux"]) {
  it(`leaves the macOS-only quit confirmation out on ${platform}`, async () => {
    serve(platform);
    render(<GeneralSection />);
    // The other General rows draw once the preferences arrive.
    expect(await screen.findByRole("switch", { name: "Working section" })).toBeVisible();
    expect(screen.queryByRole("radiogroup", { name: "Quit confirmation" })).toBeNull();
    expect(screen.queryByText(/How ⌘Q quits the desktop app on macOS/)).toBeNull();
  });
}
