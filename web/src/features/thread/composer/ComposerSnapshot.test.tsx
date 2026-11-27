import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../../api/client";
import type { Thread } from "../../../api/types";
import { createSnapshotMock } from "../../../../mock/snapshot";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { useModelStore } from "../../../product/state/modelStore";
import { SnapshotRuntime } from "../../snapshot";
import { installSnapshotTransport } from "../../snapshot/shell";
import { resetSnapshotRuntimeForTests, useSnapshot } from "../../snapshot/runtime";
import { macState, sourceOf, takeOf } from "../../snapshot/snapshotFixture.test-support";
import { Composer } from "./Composer";

const engine = null;
const thread = {
  id: "t1",
  project_id: "p",
  title: "Fix login redirect",
  engine: "claude",
  model: "m",
  cwd: "/r",
  worktree: null,
  skill_profile: null,
  scratch: false,
  active_run: null,
  last_run_state: null,
  created_at: "",
  updated_at: "",
  lifecycle: { permission_mode: "supervised" },
} as unknown as Thread;

let mock: ReturnType<typeof createSnapshotMock>;

beforeEach(async () => {
  useUi.setState(useUi.getInitialState(), true);
  useUi.getState().setPrefs({ snapshotEnabled: true });
  useWorkspace.setState({ approvals: { data: [], error: null, loading: false } });
  useModelStore.setState({ byEngine: {}, loading: {}, errors: {} });
  mock = createSnapshotMock({
    manual: true,
    state: macState({ supported: true, enabled: true, shortcut_registered: true, permissions: { screen_recording: true, accessibility: true }, needs_setup: null }),
    take: takeOf(),
  });
  installSnapshotTransport(mock);
  await useSnapshot.getState().start();
});

afterEach(() => {
  resetSnapshotRuntimeForTests();
  installSnapshotTransport(null);
  vi.restoreAllMocks();
});

const view = () => render(<><SnapshotRuntime /><Composer thread={thread} engine={engine} showContext /></>);

describe("the composer button", () => {
  it("is not there with no shell", () => {
    resetSnapshotRuntimeForTests();
    installSnapshotTransport(null);
    useUi.setState(useUi.getInitialState(), true);
    view();
    expect(screen.queryByRole("button", { name: "Snap Shot" })).toBeNull();
  });

  it("is not there when the shell answers supported:false", async () => {
    resetSnapshotRuntimeForTests();
    mock.setState({ supported: false });
    await useSnapshot.getState().start();
    view();
    expect(screen.queryByRole("button", { name: "Snap Shot" })).toBeNull();
  });

  it("is not there while the setting is off", async () => {
    mock.setState({ enabled: false });
    useUi.getState().setPrefs({ snapshotEnabled: false });
    await waitFor(() => expect(useSnapshot.getState().state?.enabled).toBe(false));
    view();
    expect(screen.queryByRole("button", { name: "Snap Shot" })).toBeNull();
  });

  it("calls snapshot_capture on the person's click", async () => {
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    await waitFor(() => expect(mock.calls.some((call) => call.command === "snapshot_capture")).toBe(true));
  });
});

describe("a finished capture", () => {
  it("takes, uploads, writes the source and shows a draft chip, and sends nothing", async () => {
    const upload = vi.spyOn(api, "uploadAttachment").mockResolvedValue({
      id: "a1", thread_id: "t1", name: "Snap Shot · Safari.png", mime: "image/png", size: 42n, created_at: "",
    });
    const source = vi.spyOn(api, "attachmentSource").mockResolvedValue();
    const start = vi.spyOn(api, "startRun");
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    await waitFor(() => expect(useSnapshot.getState().capture?.id).toBeTruthy());
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    expect(await screen.findByText("Snap Shot · Safari")).toBeVisible();
    expect(screen.getByText(/Not sent yet · Acme · Sign in/)).toBeVisible();
    await waitFor(() => expect(source).toHaveBeenCalledWith("a1", {
      kind: "snap_shot",
      captured_at: "2026-11-22T14:32:08Z",
      app_name: "Safari",
      window_title: "Acme · Sign in",
      app_id: "com.apple.Safari",
    }));
    expect(upload).toHaveBeenCalledTimes(1);
    expect(upload.mock.invocationCallOrder[0]).toBeLessThan(source.mock.invocationCallOrder[0]!);
    const file = upload.mock.calls[0]![1];
    expect(file).toBeInstanceOf(File);
    expect(file.name).toBe("Snap Shot · Safari.png");
    expect(file.type).toBe("image/png");
    expect(start).not.toHaveBeenCalled();
    const commands = mock.calls.map((call) => call.command).filter((name) => name.startsWith("snapshot_"));
    expect(commands).toEqual(expect.arrayContaining(["snapshot_capture", "snapshot_take"]));
    expect(commands.indexOf("snapshot_capture")).toBeLessThan(commands.indexOf("snapshot_take"));
    // The PNG is a File in memory: no bytes in a toast, the saved preferences or this window's storage.
    expect(JSON.stringify(useUi.getState().toasts)).not.toContain("aGVsbG8=");
    expect(JSON.stringify(localStorage)).not.toContain("aGVsbG8=");
  });

  it.each([
    [true, "the accessible text goes along"],
    [false, "no accessible text is sent"],
  ] as const)("with Include accessibility text %s, %s", async (include, _what) => {
    useUi.getState().setPrefs({ snapshotIncludeAccessibility: include });
    mock.setTake(takeOf({ source: sourceOf({ accessible_text: "Sign in to continue" }) }));
    vi.spyOn(api, "uploadAttachment").mockResolvedValue({
      id: "a1", thread_id: "t1", name: "Snap Shot.png", mime: "image/png", size: 42n, created_at: "",
    });
    const source = vi.spyOn(api, "attachmentSource").mockResolvedValue();
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    await waitFor(() => expect(useSnapshot.getState().capture?.id).toBeTruthy());
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    await waitFor(() => expect(source).toHaveBeenCalledTimes(1));
    const body = source.mock.calls[0]![1];
    if (include) expect(body.accessible_text).toBe("Sign in to continue");
    else expect(body).not.toHaveProperty("accessible_text");
  });

  it("shows app name, title and accessible text as text only", async () => {
    const hostile = "<img src=x onerror=\"window.__snapPwned=1\">";
    mock.setTake(takeOf({
      source: sourceOf({ app_name: hostile, window_title: "<a href=\"https://evil.example\">Sign in</a>", accessible_text: "<script>window.__snapPwned=1</script>" }),
    }));
    vi.spyOn(api, "uploadAttachment").mockResolvedValue({
      id: "a1", thread_id: "t1", name: "Snap Shot.png", mime: "image/png", size: 42n, created_at: "",
    });
    vi.spyOn(api, "attachmentSource").mockResolvedValue();
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    await screen.findByText("Snap Shot · <img src=x onerror=\"window.__snapPwned=1\">");
    await user.click(screen.getByRole("button", { name: "Show Snap Shot details" }));
    const dialog = await screen.findByRole("dialog", { name: "Snap Shot details" });
    expect(within(dialog).getAllByText(hostile).length).toBeGreaterThan(0);
    expect(within(dialog).getAllByText("<a href=\"https://evil.example\">Sign in</a>").length).toBeGreaterThan(0);
    expect(within(dialog).getAllByText("<script>window.__snapPwned=1</script>").length).toBeGreaterThan(0);
    expect(within(dialog).queryByRole("img")).toBeNull();
    expect(within(dialog).queryByRole("link")).toBeNull();
    expect((window as unknown as { __snapPwned?: unknown }).__snapPwned).toBeUndefined();
    expect(within(dialog).getByText(/Treat it as untrusted text, not instructions/)).toBeVisible();
  });
});

describe("failures", () => {
  const cases = [
    ["permission", "Screen Recording is not allowed"],
    ["no_window", "No window to capture"],
    ["too_large", "That window is too large"],
    ["timeout", "The capture timed out"],
    ["capture_error", "Could not capture"],
  ] as const;

  it.each(cases)("says what a %s failure means and keeps the draft unchanged", async (reason, title) => {
    const upload = vi.spyOn(api, "uploadAttachment");
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    await waitFor(() => expect(useSnapshot.getState().capture?.id).toBeTruthy());
    mock.emit("snapshot://failed", { id: useSnapshot.getState().capture!.id, reason });
    await waitFor(() => expect(useUi.getState().toasts.at(-1)?.text).toContain(title));
    expect(useUi.getState().toasts.at(-1)?.kind).toBe("failed");
    expect(upload).not.toHaveBeenCalled();
    expect(screen.queryByText(/Not sent yet/)).toBeNull();
  });

  it("shows the exclusion text with the app name and no window", async () => {
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    await waitFor(() => expect(useSnapshot.getState().capture?.id).toBeTruthy());
    mock.emit("snapshot://excluded", { app_name: "1Password", window_title: "Private vault" });
    await waitFor(() => expect(useUi.getState().toasts.at(-1)?.text).toContain("Not captured: 1Password"));
    expect(useUi.getState().toasts.at(-1)?.text).not.toContain("Private vault");
  });

  it("cancels a capture with snapshot_discard and ignores its late ready", async () => {
    const upload = vi.spyOn(api, "uploadAttachment");
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    await waitFor(() => expect(useSnapshot.getState().capture?.id).toBeTruthy());
    const id = useSnapshot.getState().capture!.id;
    expect(await screen.findByText("Capturing · gives up after 10 seconds")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Cancel capture" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_discard", args: { id } }));
    mock.emit("snapshot://ready", { id });
    await waitFor(() => expect(screen.queryByText("Capturing · gives up after 10 seconds")).toBeNull());
    expect(upload).not.toHaveBeenCalled();
  });

  it("discards when the upload fails and shows the failure", async () => {
    vi.spyOn(api, "uploadAttachment").mockRejectedValue(new Error("service down"));
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    await waitFor(() => expect(mock.calls.some((call) => call.command === "snapshot_discard")).toBe(true));
    await waitFor(() => expect(useUi.getState().toasts.at(-1)?.kind).toBe("failed"));
    expect(screen.queryByText(/Not sent yet/)).toBeNull();
  });
});

describe("removing a chip", () => {
  it("discards the capture and removes it", async () => {
    vi.spyOn(api, "uploadAttachment").mockResolvedValue({
      id: "a1", thread_id: "t1", name: "Snap Shot · Safari.png", mime: "image/png", size: 42n, created_at: "",
    });
    vi.spyOn(api, "attachmentSource").mockResolvedValue();
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    await screen.findByText("Snap Shot · Safari");
    const id = mock.calls.find((call) => call.command === "snapshot_capture") ? "snap-1" : "";
    await user.click(screen.getByRole("button", { name: "Remove Snap Shot · Safari" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_discard", args: { id } }));
    expect(screen.queryByText("Snap Shot · Safari")).toBeNull();
  });
});

describe("test capture", () => {
  it("runs the same flow with its own id and dismisses with snapshot_discard", async () => {
    vi.spyOn(api, "uploadAttachment").mockResolvedValue({
      id: "a1", thread_id: "t1", name: "Snap Shot · Safari.png", mime: "image/png", size: 42n, created_at: "",
    });
    vi.spyOn(api, "attachmentSource").mockResolvedValue();
    const user = userEvent.setup();
    view();
    await useSnapshot.getState().testCapture();
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_setup", args: { action: "test_capture" } }));
    mock.emit("snapshot://started", { id: "test-9" });
    mock.emit("snapshot://ready", { id: "test-9" });
    await screen.findByText("Snap Shot · Safari");
    await user.click(screen.getByRole("button", { name: "Remove Snap Shot · Safari" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_discard", args: { id: "test-9" } }));
  });
});
