import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { installMockFetch } from "../../../test-support";
import { GeneralSection } from "./GeneralSection";

it("offers the quit confirmation on macOS, where ⌘Q quits the desktop app", async () => {
  installMockFetch();
  render(<GeneralSection />);
  expect(await screen.findByRole("radiogroup", { name: "Quit confirmation" })).toBeVisible();
});

for (const platform of ["windows", "linux"] as const) {
  it(`leaves the macOS-only quit confirmation out on ${platform}`, async () => {
    installMockFetch().reset(`platform-${platform}`);
    render(<GeneralSection />);
    // The other General rows draw once the preferences arrive.
    expect(await screen.findByRole("switch", { name: "Working section" })).toBeVisible();
    expect(screen.queryByRole("radiogroup", { name: "Quit confirmation" })).toBeNull();
    expect(screen.queryByText(/How ⌘Q quits the desktop app on macOS/)).toBeNull();
  });
}
