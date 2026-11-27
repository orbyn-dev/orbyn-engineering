import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EngineStatus, Project } from "../../api/types";
import { installMockFetch } from "../../test-support";
import { wire, wireWrites } from "../../wire.test-support";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { DRAFT_KEY, useDraftStore } from "../../product/state/draftStore";
import { createSnapshotMock } from "../../../mock/snapshot";
import { SnapshotRuntime } from "../snapshot";
import { installSnapshotTransport } from "../snapshot/shell";
import { resetSnapshotRuntimeForTests, useSnapshot } from "../snapshot/runtime";
import { macState } from "../snapshot/snapshotFixture.test-support";
import { DraftView } from "./DraftView";

const created = () => wireWrites().filter((call) => call.method === "POST" && call.path === "/api/threads");
const uploads = () => wire().filter((call) => /^\/api\/threads\/[^/]+\/attachments\?/.test(call.path));
const sources = () => wireWrites().filter((call) => /^\/api\/attachments\/[^/]+\/source$/.test(call.path));
const runs = () => wireWrites().filter((call) => /^\/api\/threads\/[^/]+\/runs$/.test(call.path));

let mock: ReturnType<typeof createSnapshotMock>;

beforeEach(() => {
  mock = createSnapshotMock({
    manual: true,
    state: macState({ supported: true, enabled: true, permissions: { screen_recording: true, accessibility: true }, needs_setup: null }),
  });
  installSnapshotTransport(mock);
  const service = installMockFetch();
  service.handle("PUT", "/api/projects/p-app/preferences", { values: { default_engine: "codex", default_model: "m", disabled_engines: [] } });
  useWorkspace.setState({
    projects: { data: [{ id: "p-app", name: "Fixture", trusted: true, is_git: true, path: "/fixture/a" } as Project], loading: false, error: null },
    engines: { data: [{ id: "codex", name: "Codex", auth: "logged_in", installed: true, capabilities: { managed: true } } as EngineStatus], loading: false, error: null },
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
  vi.unstubAllGlobals();
});

const view = () => render(<><SnapshotRuntime /><DraftView projectId="p-app" /></>);

describe("the new-thread composer", () => {
  it("holds a capture as a draft chip and uploads and sources it only when the person sends", async () => {
    const user = userEvent.setup();
    view();
    await user.click(await screen.findByRole("button", { name: "Snap Shot" }));
    await waitFor(() => expect(useSnapshot.getState().capture?.id).toBeTruthy());
    mock.emit("snapshot://ready", { id: useSnapshot.getState().capture!.id });
    expect(await screen.findByText("Snap Shot · Safari")).toBeVisible();
    expect(screen.getByText(/Not sent yet/)).toBeVisible();
    expect(uploads()).toHaveLength(0);
    expect(runs()).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(runs()).toHaveLength(1));
    const thread = runs()[0]?.path.split("/")[3];
    expect(created()).toHaveLength(1);
    expect(uploads().map((call) => call.path)).toEqual([`/api/threads/${thread}/attachments?name=${encodeURIComponent("Snap Shot · Safari.png")}`]);
    expect(sources()).toHaveLength(1);
    expect(sources()[0]?.body).toEqual({
      kind: "snap_shot",
      captured_at: "2026-11-22T14:32:08Z",
      app_name: "Safari",
      window_title: "Acme · Sign in",
      app_id: "com.apple.Safari",
    });
    expect(sources()[0]?.method).toBe("PUT");
    const attachmentId = sources()[0]?.path.split("/")[3];
    expect(runs()[0]?.path).toBe(`/api/threads/${thread}/runs`);
    expect((runs()[0]?.body as { attachment_ids?: string[] }).attachment_ids).toEqual([attachmentId]);
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
    expect(uploads()).toHaveLength(0);
  });
});
