import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createSnapshotMock } from "../../../mock/snapshot";
import { useUi } from "../../product/state/ui";
import { installSnapshotTransport } from "./shell";
import { resetSnapshotRuntimeForTests, useSnapshot } from "./runtime";
import { SnapshotRuntime } from "./SnapshotRuntime";
import { SnapshotSettingsSection } from "./SnapshotSettingsSection";
import { macState } from "./snapshotFixture.test-support";

// The setup entry and dialog are hidden until the shell's allow actions prompt and read the grants back
// (docs/snapshots.md). These tests exercise the kept dialog behind that one flag.
vi.mock("./types", async (original) => {
  const actual = await original<typeof import("./types")>();
  return { ...actual, SNAPSHOT_SETUP_AVAILABLE: true };
});

let mock: ReturnType<typeof createSnapshotMock>;

const view = () => render(<><SnapshotRuntime /><SnapshotSettingsSection /></>);

beforeEach(() => {
  useUi.setState(useUi.getInitialState(), true);
  mock = createSnapshotMock({ manual: true });
  installSnapshotTransport(mock);
});

afterEach(() => {
  resetSnapshotRuntimeForTests();
  installSnapshotTransport(null);
});

describe("the setup dialog behind the flag", () => {
  it("calls snapshot_setup and only shows what the shell reads back", async () => {
    mock.setState(macState({ supported: true, permissions: { screen_recording: false, accessibility: false } }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    const dialog = await screen.findByRole("dialog", { name: "Set up Snap Shot" });
    expect(within(dialog).getByText("Not granted.")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Allow Screen Recording" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_setup", args: { action: "allow_screen_recording" } }));
    expect(within(dialog).queryByText("Granted")).toBeNull();
    expect(within(dialog).getByText("Waiting")).toBeVisible();

    act(() => {
      mock.setState({ permissions: { screen_recording: true, accessibility: false }, needs_setup: null });
      window.dispatchEvent(new Event("focus"));
    });
    await waitFor(() => expect(within(dialog).getByText("Granted")).toBeVisible());
    expect(within(dialog).queryByText("Waiting")).toBeNull();
  });

  it("asks for Accessibility through snapshot_setup and keeps the row Not granted", async () => {
    mock.setState(macState({ supported: true, permissions: { screen_recording: true, accessibility: false }, needs_setup: "accessibility" }));
    useUi.getState().setPrefs({ snapshotIncludeAccessibility: true, snapshotEnabled: true });
    const user = userEvent.setup();
    view();
    const dialog = await screen.findByRole("dialog", { name: "Set up Snap Shot" });
    await user.click(within(dialog).getByRole("button", { name: "Allow Accessibility" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_setup", args: { action: "allow_accessibility" } }));
    expect(within(dialog).getByText("Waiting")).toBeVisible();
    expect(within(dialog).getAllByText("Granted by macOS")).toHaveLength(1);
  });

  it("has the mac rows, the test capture, and closes with Done", async () => {
    mock.setState(macState({ supported: true, permissions: { screen_recording: true, accessibility: false }, needs_setup: null }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    const dialog = await screen.findByRole("dialog", { name: "Set up Snap Shot" });
    expect(within(dialog).getByText("Screen Recording")).toBeVisible();
    expect(within(dialog).getByText("Accessibility")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Test capture" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_setup", args: { action: "test_capture" } }));
    await user.click(within(dialog).getByRole("button", { name: "Done" }));
    expect(screen.queryByRole("dialog", { name: "Set up Snap Shot" })).toBeNull();
  });

  it("keeps a test capture as a dismissible draft chip when no composer is under the dialog", async () => {
    mock.setState(macState({ supported: true, permissions: { screen_recording: true, accessibility: true }, needs_setup: null }));
    useUi.getState().setPrefs({ snapshotEnabled: true });
    const user = userEvent.setup();
    view();
    await waitFor(() => expect(useSnapshot.getState().status).toBe("ready"));
    useSnapshot.getState().openSetup();
    const dialog = await screen.findByRole("dialog", { name: "Set up Snap Shot" });
    await user.click(within(dialog).getByRole("button", { name: "Test capture" }));
    mock.emit("snapshot://started", { id: "test-4" });
    mock.emit("snapshot://ready", { id: "test-4" });
    expect(await screen.findByText("Snap Shot · Safari")).toBeVisible();
    expect(screen.getByText(/Not sent yet/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Remove Snap Shot · Safari" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_discard", args: { id: "test-4" } }));
  });

  it("on Windows has no permission step, marks the capture border Planned and keeps accessibility Planned", async () => {
    mock.setState(macState({ supported: true, platform: "windows", permissions: { screen_recording: false, accessibility: false } }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    const dialog = await screen.findByRole("dialog", { name: "Set up Snap Shot" });
    expect(within(dialog).getByText("Windows asks for no permission for Snap Shot.")).toBeVisible();
    expect(within(dialog).getByText("Capture border")).toBeVisible();
    expect(within(dialog).getAllByText("Planned").length).toBeGreaterThan(0);
    expect(screen.getByRole("switch", { name: "Include accessibility text" })).toBeDisabled();
  });
});
