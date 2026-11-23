import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
  expect(screen.getByText("Orbyn cannot ask before Kiro acts.")).toBeVisible();
  expect(screen.queryByRole("button", { name: /^Permissions:/ })).toBeNull();
});

it("makes the heading's project name a real switcher: tabbable, marked as a picker, opened with Enter", async () => {
  const user = userEvent.setup();
  useWorkspace.setState({
    projects: { data: [{ id: "a", name: "Fixture", trusted: true, is_git: true, path: "/fixture/a" } as Project], loading: false, error: null },
    engines: { data: [], loading: false, error: null },
    threads: {},
    managed: { data: null, loading: false, error: null },
  });
  render(<DraftView projectId="a" />);
  const hero = screen.getByRole("button", { name: "Fixture" });
  expect(hero).toHaveAttribute("aria-haspopup", "listbox");
  expect(hero).toHaveAttribute("aria-expanded", "false");
  // The designs draw a picker with a down chevron; the button carries it and it stays out of the accessible name.
  expect(hero.querySelector("svg")).not.toBeNull();
  hero.focus();
  expect(hero).toHaveFocus();
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("listbox", { name: "Projects" })).toBeVisible();
  expect(hero).toHaveAttribute("aria-expanded", "true");
});

it("draws an engine value that is not a control as a span, so it is neither tabbable nor a button", () => {
  useWorkspace.setState({
    projects: { data: [{ id: "a", name: "Fixture", trusted: true, is_git: true, path: "/fixture/a" } as Project], loading: false, error: null },
    engines: { data: [], loading: false, error: null },
    threads: {},
    managed: { data: null, loading: false, error: null },
  });
  render(<DraftView projectId="a" />);
  // With no engine installed the composer shows "No engine": a value, not something to press.
  const value = screen.getByText("No engine");
  expect(value.classList.contains("composer-control")).toBe(true);
  expect(value.closest("button")).toBeNull();
  expect(value.closest("[tabindex]")).toBeNull();
  expect(screen.queryByRole("button", { name: /No engine/ })).toBeNull();
});
