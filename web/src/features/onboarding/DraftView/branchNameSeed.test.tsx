import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { EngineStatus, Project } from "../../../api/types";
import { installMockFetch } from "../../../test-support";
import { wireWrites } from "../../../wire.test-support";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { DRAFT_KEY } from "../../../product/state/draftStore";
import { DraftView } from "./index";

vi.mock("../../../product/shell/shortcuts", () => ({ useCommand: () => undefined }));
vi.mock("../../workspace/app-chrome/GitHubStarPrompt", () => ({ recordUserPrompt: () => undefined }));
vi.mock("../useDraftCheckout", () => ({
  useDraftCheckout: () => ({ worktree: "seed-one", setWorktree: () => undefined, loading: false, error: null }),
}));
vi.mock("../useDraftModelDefaults", () => {
  const engine = { id: "codex", name: "Codex", installed: true, capabilities: { managed: true, permission_modes: ["supervised"], approvals: true, skill_delivery: [] } };
  return {
    useDraftModelDefaults: () => ({
      installed: [engine],
      engine,
      model: "m",
      effort: null,
      loading: false,
      error: null,
      explicit: false,
      choose: () => undefined,
      reload: () => undefined,
    }),
  };
});
vi.mock("../../repos", async (original) => ({ ...(await original<typeof import("../../repos")>()), useRepos: () => ({ view: null }) }));

beforeEach(() => {
  installMockFetch();
  useWorkspace.setState({
    projects: { data: [{ id: "p-app", name: "Fixture", trusted: true, is_git: true, path: "/fixture/a" } as Project], loading: false, error: null },
    engines: { data: [{ id: "codex", name: "Codex", auth: "logged_in", installed: true } as EngineStatus], loading: false, error: null },
    threads: {},
    managed: { data: null, loading: false, error: null },
  });
  useUi.setState({ lastModel: { engine: "codex", model: "m", effort: null }, drafts: { [DRAFT_KEY]: "  Fix   the login timeout  " } });
});

it("sends a branch-name seed and no title for a new worktree", async () => {
  const created = () => wireWrites().filter((call) => call.method === "POST" && call.path === "/api/threads");
  render(<DraftView projectId="p-app" />);
  const send = screen.getByRole("button", { name: "Send" });
  await waitFor(() => expect(send).toBeEnabled());
  await userEvent.setup().click(send);
  await waitFor(() => expect(created()).toHaveLength(1));
  expect(created()[0]?.body).toEqual({
    project_id: "p-app",
    engine: "codex",
    model: "m",
    title: null,
    worktree: "seed-one",
    branch_name_seed: "Fix the login timeout",
    skill_profile: null,
  });
});
