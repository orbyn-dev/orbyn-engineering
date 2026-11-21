import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { createReposMock } from "../../../mock/repos";
import type { Thread } from "../../api/types";
import { setReposApi } from "../repos";
import { TopBar } from "./ThreadView";

vi.mock("./RelatedThreads", () => ({ RelatedThreads: () => null }));
vi.mock("./GitControl", () => ({ GitActions: () => null, BranchShortcut: () => null }));
vi.mock("./ThreadHeaderMenu", () => ({ ThreadHeaderMenu: () => null }));
vi.mock("./PullChip", () => ({ PullChip: () => <span>PullChip</span> }));
vi.mock("../issues", () => ({ IssueChip: () => <span>IssueChip</span> }));

afterEach(() => setReposApi(null));
const thread = { id: "thread-1", project_id: "p1", title: "Fix login redirect", worktree: null, active_run: null, last_run_state: null } as unknown as Thread;
const draw = () => render(<TopBar thread={thread} project={null} detailsShown={false} panelShown={false} onToggleDetails={() => {}} onTogglePanel={() => {}} />);

it("has no Repos chip in the header while the service reports no repositories for the thread", async () => {
  draw();
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByRole("button", { name: /^Repos/ })).toBeNull();
  expect(screen.getByText("IssueChip")).toBeVisible();
});

it("draws the Repos chip in the header beside the issue and pull request chips for a thread in two repositories", async () => {
  setReposApi(createReposMock());
  draw();
  const chip = await screen.findByRole("button", { name: "Repos 2" });
  const group = chip.parentElement!;
  expect([...group.children].slice(0, 3).map((node) => node.textContent)).toEqual(["Repos 2", "IssueChip", "PullChip"]);
});
