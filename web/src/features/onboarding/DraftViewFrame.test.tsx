import { act, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { EngineStatus, Project } from "../../api/types";
import { useWorkspace } from "../../state/workspace";
import { DraftView } from "./DraftView";

vi.mock("../../app/shortcuts", () => ({ useCommand: () => undefined }));
vi.mock("../workspace/app-chrome/GitHubStarPrompt", () => ({ recordUserPrompt: () => undefined }));
vi.mock("./useDraftCheckout", () => ({ useDraftCheckout: () => ({ worktree: null, setWorktree: () => undefined, loading: false, error: null }) }));
vi.mock("./useDraftModelDefaults", () => ({ useDraftModelDefaults: () => ({ installed: [], engine: null, model: null, effort: null, loading: false, error: null, choose: () => undefined, reload: () => undefined }) }));

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
