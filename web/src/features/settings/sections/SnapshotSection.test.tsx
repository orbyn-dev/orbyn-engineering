import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

  it("turns on, sends the full settings and keeps the Planned setup entry hidden", async () => {
    const user = userEvent.setup();
    view();
    const snap = await screen.findByRole("switch", { name: "Snap Shot" });
    await user.click(snap);
    await waitFor(() => expect(mock.configured.at(-1)?.snapshot_enabled).toBe(true));
    expect(Object.keys(mock.configured.at(-1)!).sort()).toEqual(Object.keys(DEFAULT_SETTINGS).sort());
    expect(screen.queryByRole("dialog", { name: "Set up Snap Shot" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Set up Snap Shot…" })).toBeNull();
    expect(useSnapshot.getState().setupOpen).toBe(false);
    expect(mock.calls.some((call) => call.command === "snapshot_setup")).toBe(false);
  });

  it("does not offer an Allow Accessibility action while setup is Planned", async () => {
    mock.setState(macState({ supported: true, enabled: true, permissions: { screen_recording: true, accessibility: false }, needs_setup: "accessibility" }));
    useUi.getState().setPrefs({ snapshotEnabled: true, snapshotIncludeAccessibility: true });
    view();
    expect(await screen.findByText(/Accessibility is not allowed for Orbyn/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Allow Accessibility" })).toBeNull();
    expect(screen.queryByRole("dialog", { name: "Set up Snap Shot" })).toBeNull();
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
