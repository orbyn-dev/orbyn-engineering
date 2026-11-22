import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createSnapshotMock } from "../../../mock/snapshot";
import { installSnapshotTransport } from "./shell";
import { resetSnapshotRuntimeForTests } from "./runtime";
import SnapshotPreview from "./SnapshotPreview";
import { macState } from "./snapshotFixture.test-support";

let mock: ReturnType<typeof createSnapshotMock>;

beforeEach(() => {
  mock = createSnapshotMock({ state: macState({ supported: true }) });
  installSnapshotTransport(mock);
});

afterEach(() => {
  resetSnapshotRuntimeForTests();
  installSnapshotTransport(null);
  window.location.hash = "";
});

describe("the Snap Shot preview", () => {
  it("draws a settings frame from the mock", async () => {
    window.location.hash = "#/preview/snapshot?frame=snapshot-settings-off";
    render(<SnapshotPreview />);
    expect(await screen.findByRole("switch", { name: "Snap Shot" })).not.toBeChecked();
    expect(screen.getByText("Settings")).toBeVisible();
  });

  it("draws the settings accessibility frame without the setup dialog", async () => {
    window.location.hash = "#/preview/snapshot?frame=snapshot-settings-accessibility";
    render(<SnapshotPreview />);
    expect(await screen.findByText(/Accessibility is not allowed for Orbyn/)).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "Set up Snap Shot" })).toBeNull();
  });

  it("draws a captured draft chip from the mock's ready event", async () => {
    window.location.hash = "#/preview/snapshot?frame=snapshot-draft";
    render(<SnapshotPreview />);
    await waitFor(() => expect(screen.getByText("Snap Shot · Safari")).toBeVisible(), { timeout: 2000 });
    expect(screen.getByText(/Not sent yet · Acme · Sign in/)).toBeVisible();
  });
});
