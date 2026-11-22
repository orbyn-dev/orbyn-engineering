import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import type { EngineStatus, Project } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { DraftView } from "./DraftView";
import { useDraftModelDefaults } from "./useDraftModelDefaults";

vi.mock("../../product/shell/shortcuts", () => ({ useCommand: () => undefined }));
vi.mock("../workspace/app-chrome/GitHubStarPrompt", () => ({ recordUserPrompt: () => undefined }));
vi.mock("./useDraftCheckout", () => ({ useDraftCheckout: () => ({ worktree: null, setWorktree: () => undefined, loading: false, error: null }) }));
vi.mock("./useDraftModelDefaults", () => ({ useDraftModelDefaults: vi.fn() }));
vi.mock("../repos", async (original) => ({ ...(await original<typeof import("../repos")>()), useRepos: () => ({ view: null }) }));
vi.mock("../../product/state/modelStore", async (original) => {
  const actual = await original<typeof import("../../product/state/modelStore")>();
  return { ...actual, useEngineModels: () => null, resolveModel: () => null };
});

const noDefaults = {
  installed: [],
  engine: null,
  model: null,
  effort: null,
  loading: false,
  error: null,
  explicit: false,
  choose: () => undefined,
  reload: () => undefined,
};

beforeEach(() => {
  vi.mocked(useDraftModelDefaults).mockReturnValue(noDefaults);
});

it("keeps the draft frame mounted and unseen until projects and engines arrive, so nothing shifts", () => {
  useWorkspace.setState({ projects: { data: null, loading: true, error: null }, engines: { data: null, loading: true, error: null } });
  render(<DraftView projectId={null} />);
  const frame = document.querySelector(".draft-layout");
  const composer = screen.getByRole("textbox", { name: "Message" });
  expect(frame).toHaveAttribute("data-pending");
  expect(document.querySelector(".draft-main")).toBeInTheDocument();

  act(() => {
    useWorkspace.setState({
      projects: { data: [{ id: "a", name: "Fixture", trusted: true, is_git: true, path: "/fixture/a" } as Project], loading: false, error: null },
      engines: { data: [{ id: "codex", name: "Codex", installed: true } as EngineStatus], loading: false, error: null },
    });
  });
  expect(document.querySelector(".draft-layout")).toBe(frame);
  expect(frame).not.toHaveAttribute("data-pending");
  expect(screen.getByRole("textbox", { name: "Message" })).toBe(composer);
  expect(composer).toHaveFocus();
});

it("shows the visible Unmanaged word on the draft's engine chip and in place of the permission control", () => {
  const kiro = {
    id: "kiro", name: "Kiro", installed: true, tier: "acp",
    capabilities: { managed: false, permission_modes: ["supervised"], approvals: false, skill_delivery: [] },
  } as unknown as EngineStatus;
  vi.mocked(useDraftModelDefaults).mockReturnValue({
    ...noDefaults,
    installed: [kiro],
    engine: kiro,
  });
  useWorkspace.setState({
    projects: {
      data: [{ id: "a", name: "Fixture", trusted: true, is_git: true, path: "/fixture/a" } as Project],
      loading: false,
      error: null,
    },
    engines: { data: [kiro], loading: false, error: null },
    threads: {},
    managed: { data: null, loading: false, error: null },
  });
  render(<DraftView projectId="a" />);
  expect(screen.getByRole("button", { name: "Kiro" })).toHaveTextContent("Unmanaged");
  expect(screen.getAllByText("Unmanaged").length).toBeGreaterThanOrEqual(2);
  expect(screen.queryByRole("button", { name: /^Permissions:/ })).toBeNull();
});
