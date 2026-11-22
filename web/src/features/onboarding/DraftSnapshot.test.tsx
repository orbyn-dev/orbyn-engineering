import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api, request } from "../../api/client";
import type { EngineStatus, Project, Run, Thread } from "../../api/types";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { DRAFT_KEY, useDraftStore } from "../../product/state/draftStore";
import { createSnapshotMock } from "../../../mock/snapshot";
import { SnapshotRuntime } from "../snapshot";
import { installSnapshotTransport } from "../snapshot/shell";
import { resetSnapshotRuntimeForTests, useSnapshot } from "../snapshot/runtime";
import { macState } from "../snapshot/snapshotFixture.test-support";
import { DraftView } from "./DraftView";

vi.mock("../../api/client", async (original) => {
  const actual = await original<typeof import("../../api/client")>();
  return {
    ...actual,
    request: vi.fn(),
    api: { ...actual.api, createThread: vi.fn(), startRun: vi.fn(), uploadAttachment: vi.fn(), attachmentSource: vi.fn() },
  };
});
vi.mock("../../product/shell/shortcuts", () => ({ useCommand: () => undefined }));
vi.mock("../workspace/app-chrome/GitHubStarPrompt", () => ({ recordUserPrompt: () => undefined }));
vi.mock("./useDraftCheckout", () => ({ useDraftCheckout: () => ({ worktree: null, setWorktree: () => undefined, loading: false, error: null }) }));
vi.mock("../../product/state/modelStore", () => ({ useEngineModels: () => null, resolveModel: () => null }));
vi.mock("../thread/composer/ComposerTraits", () => ({
  ACCESS: { supervised: { label: "Supervised", icon: () => null } },
  EffortTrigger: () => null,
  offeredModes: () => ["supervised"],
  shownAccess: (_engine: unknown, saved: string) => saved,
  useTraitsMenu: () => ({ menu: null, openAt: () => undefined }),
}));
vi.mock("../thread/composer/ModelPicker", () => ({ ModelPickerTrigger: () => null }));

const defaults = [
  { key: "default_engine", value: "codex", default: "", source: "project", project_scoped: true },
  { key: "default_model", value: "m", default: "", source: "project", project_scoped: true },
  { key: "disabled_engines", value: [], default: [], source: "default", project_scoped: true },
];

let mock: ReturnType<typeof createSnapshotMock>;

beforeEach(() => {
  mock = createSnapshotMock({
    manual: true,
    state: macState({ supported: true, enabled: true, permissions: { screen_recording: true, accessibility: true }, needs_setup: null }),
  });
  installSnapshotTransport(mock);
  vi.mocked(request).mockReset().mockResolvedValue(defaults as never);
  vi.mocked(api.createThread).mockReset().mockResolvedValue({ id: "created", project_id: "a", title: "Thread" } as Thread);
  vi.mocked(api.uploadAttachment).mockReset().mockResolvedValue({ id: "att-snap", thread_id: "created", name: "Snap Shot · Safari.png", mime: "image/png", size: 2n, created_at: "" });
  vi.mocked(api.attachmentSource).mockReset().mockResolvedValue();
  vi.mocked(api.startRun).mockReset().mockResolvedValue({ id: "run" } as Run);
  useWorkspace.setState({
    projects: { data: [{ id: "a", name: "Fixture", trusted: true, is_git: true, path: "/fixture/a" } as Project], loading: false, error: null },
    engines: { data: [{ id: "codex", name: "Codex", auth: "logged_in", installed: true, capabilities: { managed: true } } as EngineStatus], loading: false, error: null },
    putThread: vi.fn(),
  });
  useUi.setState({
    lastModel: { engine: "codex", model: "m", effort: null },
    drafts: { [DRAFT_KEY]: "Send this shot" },
  });
  useUi.getState().setPrefs({ snapshotEnabled: true });
  useDraftStore.setState({ files: [], snapShots: [] });
});

afterEach(() => {
  resetSnapshotRuntimeForTests();
  installSnapshotTransport(null);
  vi.restoreAllMocks();
});

const view = () => render(<><SnapshotRuntime /><DraftView projectId="a" /></>);

describe("the new-thread composer", () => {
  it("holds a capture as a draft chip and uploads and sources it only when the person sends", async () => {
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    await waitFor(() => expect(useSnapshot.getState().capture?.id).toBeTruthy());
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    expect(await screen.findByText("Snap Shot · Safari")).toBeVisible();
    expect(screen.getByText(/Not sent yet/)).toBeVisible();
    expect(api.uploadAttachment).not.toHaveBeenCalled();
    expect(api.startRun).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(api.createThread).toHaveBeenCalledTimes(1));
    expect(api.uploadAttachment).toHaveBeenCalledWith("created", expect.any(File));
    expect(api.attachmentSource).toHaveBeenCalledWith("att-snap", {
      kind: "snap_shot",
      captured_at: "2026-11-22T14:32:08Z",
      app_name: "Safari",
      window_title: "Acme · Sign in",
      app_id: "com.apple.Safari",
    });
    expect(vi.mocked(api.startRun).mock.calls[0]?.[2]).toEqual(["att-snap"]);
  });

  it("discards a held capture when its chip is removed before sending", async () => {
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    await screen.findByText("Snap Shot · Safari");
    await user.click(screen.getByRole("button", { name: "Remove Snap Shot · Safari" }));
    await waitFor(() => expect(mock.calls).toContainEqual({ command: "snapshot_discard", args: { id: "snap-1" } }));
    expect(screen.queryByText("Snap Shot · Safari")).toBeNull();
    expect(api.uploadAttachment).not.toHaveBeenCalled();
  });
});
