import { act, render, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { createReposMock } from "../../../mock/repos";
import { useWorkspace } from "../../product/state/workspace";
import { setReposApi } from "./api";
import { chooseRepo, RepoChooser, repoLabel, usePickedRepo, useRepoChoice } from "./choice";
import type { ThreadRepo } from "./contract";

const repo = (id: string, name: string, over: Partial<ThreadRepo> = {}): ThreadRepo => ({ repo_id: id, name, branch: "fix/x", added: 0, deleted: 0, trusted: true, ...over });
const two = [repo("p", "app"), repo("planner", "planner")];

beforeEach(() => { useWorkspace.setState({ threads: { t: { id: "t", project_id: "p" } as never } }); });
afterEach(() => { chooseRepo("t", null); setReposApi(null); });

function Harness() {
  const choice = useRepoChoice("t");
  return <><RepoChooser choice={choice} /><output>{choice.repoId ?? "primary"}</output></>;
}

it("defaults to the primary (the project's own repository) and sends no repo_id for it", () => {
  const { result } = renderHook(() => usePickedRepo("t", [repo("planner", "planner"), repo("p", "app")]));
  expect(result.current.selected?.repo_id).toBe("p");
  expect(result.current.repoId).toBeUndefined();
});

it("takes the first repository as the primary when the list does not hold the project's", () => {
  useWorkspace.setState({ threads: { t: { id: "t", project_id: "other" } as never } });
  const { result } = renderHook(() => usePickedRepo("t", two));
  expect(result.current.selected?.repo_id).toBe("p");
  expect(result.current.repoId).toBeUndefined();
});

it("a switch changes the selected repository and its repo_id, and picking the primary again drops it", () => {
  const { result } = renderHook(() => usePickedRepo("t", two));
  act(() => result.current.choose("planner"));
  expect(result.current.selected?.repo_id).toBe("planner");
  expect(result.current.repoId).toBe("planner");
  act(() => result.current.choose("p"));
  expect(result.current.repoId).toBeUndefined();
});

it("a thread with no list (404), or in one repository, has no choice and no repo_id", () => {
  for (const list of [null, [], [repo("p", "app")]]) {
    const { result } = renderHook(() => usePickedRepo("t", list));
    expect(result.current).toMatchObject({ multi: false, selected: null, repoId: undefined });
  }
});

it("draws the chooser with the primary selected, switches on a pick and checks the chosen one", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: two } }));
  render(<Harness />);
  const chip = await screen.findByRole("button", { name: "Repository: app" });
  await user.click(chip);
  expect(screen.getByRole("menuitem", { name: "app" })).toHaveAttribute("aria-current", "true");
  await user.click(screen.getByRole("menuitem", { name: "planner" }));
  expect(await screen.findByRole("button", { name: "Repository: planner" })).toBeVisible();
  expect(screen.getByRole("status")).toHaveTextContent("planner");
});

it("draws nothing when the repositories answer 404", async () => {
  setReposApi(createReposMock({ threads: {} }));
  render(<Harness />);
  await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("primary"));
  expect(screen.queryByRole("button", { name: /^Repository:/ })).toBeNull();
});

it("draws a service name as one capped line of text", () => {
  expect(repoLabel("a\nb")).toBe("a b");
  expect(repoLabel("x".repeat(200))).toHaveLength(48);
});
