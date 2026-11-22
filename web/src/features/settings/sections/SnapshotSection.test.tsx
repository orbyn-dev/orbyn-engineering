import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createSnapshotMock } from "../../../../mock/snapshot";
import { useUi } from "../../../product/state/ui";
import { installSnapshotTransport } from "../../snapshot/shell";
import { resetSnapshotRuntimeForTests, useSnapshot } from "../../snapshot/runtime";
import { SnapshotRuntime, SnapshotSettingsSection } from "../../snapshot";
import { DEFAULT_SETTINGS, macState } from "../../snapshot/snapshotFixture.test-support";

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

describe("when the shell is not there", () => {
  it("draws no Snap Shot section, no Planned label and no disabled row", async () => {
    installSnapshotTransport(null);
    view();
    await waitFor(() => expect(useSnapshot.getState().status).toBe("hidden"));
    expect(screen.queryByText("Snap Shot")).toBeNull();
    expect(screen.queryByText("Planned")).toBeNull();
  });

  it("draws nothing when the shell answers supported:false", async () => {
    mock.setState({ supported: false });
    view();
    await waitFor(() => expect(useSnapshot.getState().status).toBe("hidden"));
    expect(screen.queryByText("Snap Shot")).toBeNull();
  });

  it("draws nothing when snapshot_state throws", async () => {
    installSnapshotTransport({
      async invoke() {
        throw new Error("no");
      },
      async listen() {
        return () => undefined;
      },
    });
    view();
    await waitFor(() => expect(useSnapshot.getState().status).toBe("hidden"));
    expect(screen.queryByText("Snap Shot")).toBeNull();
  });
});

describe("the settings, off and on", () => {
  it("starts off with one switch, no shortcut and the contract default chord stored", async () => {
    view();
    const snap = await screen.findByRole("switch", { name: "Snap Shot" });
    expect(snap).not.toBeChecked();
    expect(screen.queryByText("Shortcut")).toBeNull();
    expect(useUi.getState().snapshotShortcut).toEqual(DEFAULT_SETTINGS.snapshot_shortcut);
    expect(useUi.getState().snapshotEnabled).toBe(false);
    expect(screen.getByText(/Orbyn registers no shortcut and asks macOS for nothing/)).toBeVisible();
  });

  it("turns on, sends the full settings and opens the setup dialog on the first enable", async () => {
    const user = userEvent.setup();
    view();
    const snap = await screen.findByRole("switch", { name: "Snap Shot" });
    await user.click(snap);
    await waitFor(() => expect(mock.configured.at(-1)?.snapshot_enabled).toBe(true));
    expect(Object.keys(mock.configured.at(-1)!).sort()).toEqual(Object.keys(DEFAULT_SETTINGS).sort());
    expect(await screen.findByRole("dialog", { name: "Set up Snap Shot" })).toBeVisible();
    expect(mock.calls.some((call) => call.command === "snapshot_setup")).toBe(false);
  });

  it("shows the accessibility warning in plain words, off by default", async () => {
    mock.setState(macState({ supported: true, enabled: false, permissions: { screen_recording: true, accessibility: false } }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    const ax = await screen.findByRole("switch", { name: "Include accessibility text" });
    expect(ax).not.toBeChecked();
    expect(screen.getByText(/Off by default: it sends what is in another app's window/)).toBeVisible();
  });

  it("sends every change with the full eight settings", async () => {
    mock.setState(macState({ supported: true }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    await waitFor(() => expect(mock.configured.at(-1)?.snapshot_enabled).toBe(true));
    const count = mock.configured.length;

    await user.click(screen.getByRole("switch", { name: "Flash" }));
    await waitFor(() => expect(mock.configured.length).toBeGreaterThan(count));
    expect(mock.configured.at(-1)).toMatchObject({ snapshot_flash: false });
    expect(Object.keys(mock.configured.at(-1)!).sort()).toEqual(Object.keys(DEFAULT_SETTINGS).sort());

    await user.click(screen.getByRole("switch", { name: "Animations" }));
    await waitFor(() => expect(mock.configured.at(-1)).toMatchObject({ snapshot_animations: false }));

    await user.click(screen.getByRole("switch", { name: "Play a sound" }));
    await waitFor(() => expect(mock.configured.at(-1)).toMatchObject({ snapshot_play_sound: false }));

    await user.selectOptions(screen.getByRole("combobox", { name: "Sound" }), "camera-shutter");
    await waitFor(() => expect(mock.configured.at(-1)).toMatchObject({ snapshot_sound: "camera-shutter" }));

    await user.click(screen.getByRole("switch", { name: "Include accessibility text" }));
    await waitFor(() => expect(mock.configured.at(-1)).toMatchObject({ snapshot_include_accessibility: true }));
  });
});

describe("the shortcut recorder", () => {
  it("refuses a chord with no modifier and keeps recording", async () => {
    mock.setState(macState({ supported: true }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    await user.click(await screen.findByRole("button", { name: "Change" }));
    const recorder = screen.getByRole("group", { name: "Shortcut recorder" });
    fireEvent.keyDown(recorder, { key: "9" });
    expect(screen.getByRole("alert")).toHaveTextContent("Press the keys you want, with at least one modifier. Escape cancels.");
    expect(screen.getByRole("group", { name: "Shortcut recorder" })).toBeVisible();
    expect(useUi.getState().snapshotShortcut).toEqual(DEFAULT_SETTINGS.snapshot_shortcut);
  });

  it("takes a chord with a modifier, saves it and sends it", async () => {
    mock.setState(macState({ supported: true }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    await user.click(await screen.findByRole("button", { name: "Change" }));
    const recorder = screen.getByRole("group", { name: "Shortcut recorder" });
    // Shift turns the 9 key into "(" in the browser; the physical key is what the chord records.
    fireEvent.keyDown(recorder, { key: "(", code: "Digit9", metaKey: true, shiftKey: true });
    await waitFor(() =>
      expect(useUi.getState().snapshotShortcut).toEqual({ kind: "chord", key: "9", mod: true, shift: true, alt: false, ctrl: false }),
    );
    await waitFor(() => expect(mock.configured.at(-1)?.snapshot_shortcut.key).toBe("9"));
    expect(screen.queryByRole("group", { name: "Shortcut recorder" })).toBeNull();
  });

  it("cancels with Escape and changes nothing", async () => {
    mock.setState(macState({ supported: true }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    await user.click(await screen.findByRole("button", { name: "Change" }));
    fireEvent.keyDown(screen.getByRole("group", { name: "Shortcut recorder" }), { key: "Escape" });
    expect(screen.queryByRole("group", { name: "Shortcut recorder" })).toBeNull();
    expect(useUi.getState().snapshotShortcut).toEqual(DEFAULT_SETTINGS.snapshot_shortcut);
  });

  it("shows both Shift keys and the modifier pair as disabled choices marked Planned, with the reason", async () => {
    mock.setState(macState({ supported: true }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    await user.click(await screen.findByRole("button", { name: "Change" }));
    expect(screen.getByRole("button", { name: "Both Shift keys" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Modifier pair" })).toBeDisabled();
    expect(screen.getAllByText("Planned").length).toBeGreaterThan(0);
    expect(screen.getByText("Both Shift keys, or one modifier pair. Planned.")).toBeVisible();
  });
});

describe("the shell's state", () => {
  it("shows a shortcut the shell could not register, its reason, and asks it to try again", async () => {
    mock.setState(macState({ supported: true, shortcut_registered: false, shortcut_message: "Another app uses Cmd+Shift+9.", permissions: { screen_recording: true, accessibility: true }, needs_setup: null }));
    useUi.getState().setPrefs({ snapshotEnabled: true });
    const user = userEvent.setup();
    view();
    expect(await screen.findByText("Another app uses Cmd+Shift+9.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_setup", args: { action: "retry_shortcut" } }));
  });
});

describe("the exclusion list", () => {
  it("shows the built-in apps as read-only and adds and removes a person's app", async () => {
    mock.setState(macState({ supported: true }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("switch", { name: "Snap Shot" }));
    await user.click(await screen.findByRole("button", { name: "Edit list…" }));
    const dialog = await screen.findByRole("dialog", { name: "Apps to skip" });
    for (const name of ["Orbyn (its own windows)", "1Password", "Keychain Access", "Passwords"]) {
      expect(within(dialog).getByText(name)).toBeVisible();
    }
    expect(within(dialog).queryByRole("button", { name: /Remove 1Password/ })).toBeNull();
    expect(within(dialog).getAllByText("Built in. Cannot be removed.")).toHaveLength(8);

    await user.type(within(dialog).getByLabelText("App id"), "com.example.vault");
    await user.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(within(dialog).getByText("com.example.vault")).toBeVisible();
    expect(useUi.getState().snapshotExcludedApps).toEqual(["com.example.vault"]);
    await waitFor(() => expect(mock.configured.at(-1)?.snapshot_excluded_apps).toEqual(["com.example.vault"]));

    await user.click(within(dialog).getByRole("button", { name: "Remove com.example.vault" }));
    expect(useUi.getState().snapshotExcludedApps).toEqual([]);
  });

  it("refuses a 65th entry and one over 255 characters", async () => {
    useUi.getState().setPrefs({ snapshotEnabled: true, snapshotExcludedApps: Array.from({ length: 64 }, (_, i) => `com.example.app${i}`) });
    mock.setState(macState({ supported: true, enabled: true }));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Edit list…" }));
    const dialog = await screen.findByRole("dialog", { name: "Apps to skip" });
    await user.type(within(dialog).getByLabelText("App id"), "com.example.one-too-many");
    await user.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("The list holds at most 64 apps.");

    useUi.getState().setPrefs({ snapshotExcludedApps: [] });
    const input = within(dialog).getByLabelText("App id");
    fireEvent.change(input, { target: { value: "a".repeat(256) } });
    await user.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("An app id can be at most 255 characters.");
  });
});

describe("the setup dialog", () => {
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
