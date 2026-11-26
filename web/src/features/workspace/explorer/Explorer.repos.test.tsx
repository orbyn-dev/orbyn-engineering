import { act, configure, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { dataChanged } from "../../../api/shell";
import { App } from "../../../app/App";
import { createReposMock } from "../../../../mock/repos";
import { startDataRefresh } from "../../../lib/dataRefresh";
import { fileTabId, useUi } from "../../../product/state/ui";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { installMockFetch } from "../../../test-support";
import { setReposApi, type RepoRow, type ThreadRepo } from "../../repos";
import { useExplorer } from "./store";

// F70 (#1058): the left explorer browses the open multi-repository thread's chosen repository through the thread's own
// file tree route (`?repo_id=`); a thread in one repository keeps exactly the explorer it had, with no chooser and no
// such read. The repository also governs the file tab: a file opens as its repository's tab, which names it. The primary
// repository's tab is still the thread's own for reads, writes and failures and only names the repository; another
// repository's tab reads and writes with its `repo_id`.
const THREAD = "t-done";
const PRIMARY = "p-app";
const OTHER = "orbyn-planner";

const treeOf = () => screen.getByRole("tree", { name: "Projects, worktrees and files" });
/** A row is named by what it holds: its name, then a change, a status or a button. A string matches its start. */
const named = (name: string | RegExp) => (typeof name === "string" ? new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`) : name);
const maybeRow = (name: string | RegExp) => within(treeOf()).queryByRole("treeitem", { name: named(name) });
const findRow = (name: string | RegExp) => screen.findByRole("treeitem", { name: named(name) });

const checkoutRepo = (repoId: string, name: string): ThreadRepo => ({ repo_id: repoId, name, branch: "orbyn/1058", added: 0, deleted: 0, trusted: true, files: [] });
const projectRepo = (id: string, name: string, primary: boolean): RepoRow => ({ id, name, path: `~/Github/${name}`, is_git: true, trusted: true, branch: "main", branches: ["main"], has_remote: false, primary, clone: null });

/** The two repositories of the mock's multi-repository thread, as the project lists them. */
function twoRepositories() {
  setReposApi(createReposMock({
    threads: { [THREAD]: [checkoutRepo(PRIMARY, "orbyn-engineering"), checkoutRepo(OTHER, "orbyn-planner")] },
    repositories: [projectRepo(PRIMARY, "orbyn-engineering", true), projectRepo(OTHER, "orbyn-planner", false)],
  }));
}

/**
 * What each repository's tree holds: names no project worktree has, so the rows below a checkout row can only come from
 * the thread's file tree route. The primary repository's tree also has a folder, to read one level deeper with its id.
 */
const FILES: Record<"primary" | "other", string[]> = {
  primary: ["main.ts", "checkout/app.ts", "README.md"],
  other: ["planner.ts", "plans/plan.md"],
};

/** One folder level of a repository's tree, as the service's file list route answers it (folders first, then names). */
function levelOf(path: string, repoId: string | null) {
  const files = repoId === null ? FILES.primary : FILES.other;
  const base = path ? `${path}/` : "";
  const seen = new Map<string, { name: string; path: string; is_dir: boolean; size: number; ignored: boolean }>();
  for (const file of files) {
    if (!file.startsWith(base)) continue;
    const [first, ...rest] = file.slice(base.length).split("/");
    if (!first) continue;
    const entry = seen.get(first) ?? { name: first, path: `${base}${first}`, is_dir: false, size: 10, ignored: false };
    if (rest.length > 0) entry.is_dir = true;
    seen.set(first, entry);
  }
  return [...seen.values()].sort((a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name));
}

/** Serves every thread's file tree route and records each URL it was asked for. Everything else is the app's mock API. */
function serveTrees() {
  const real = fetch;
  const reads: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    const url = String(input);
    if (/^\/api\/threads\/[^/?]+\/files\?/.test(url)) {
      reads.push(url);
      const query = new URL(url, "http://x").searchParams;
      return new Response(JSON.stringify(levelOf(query.get("path") ?? "", query.get("repo_id"))), { status: 200 });
    }
    return real(input, init);
  }));
  return reads;
}

// Whole-App renders that wait for the tree reads: at the file's top, so the first test gets the longer timeout too.
vi.setConfig({ testTimeout: 30_000 });
configure({ asyncUtilTimeout: 8000 });

beforeEach(() => {
  useExplorer.getState().reset();
  installMockFetch();
  useUi.setState({ projectFolders: true, panelOpen: false, panelTab: null, panelTabOrder: ["diff", "browser", "activity", "side", "tools"], panelTabsClosed: ["diff", "browser", "activity", "side", "tools"], layouts: {}, leftBarView: "files", explorerOpen: {}, sidebarWidth: 224 });
  useUi.getState().setPrefs({ welcomeDone: true });
  window.location.hash = `#/threads/${THREAD}`;
});

afterEach(() => {
  setReposApi(null);
  vi.restoreAllMocks();
});

/** The explorer's own repository chip, not the thread header's: they share one choice, so the left bar's is here. */
const sidebar = () => screen.getByRole("navigation", { name: "Workspace" });
const chooser = () => within(sidebar()).getByRole("button", { name: /^Repository:/ });

it("offers the repository chooser, opens the primary repository's tree, and sends no repo_id for it", async () => {
  twoRepositories();
  const reads = serveTrees();
  render(<App />);

  // One chip naming the repository the checkout row shows; the primary is the one until the person picks another.
  expect(await within(await screen.findByRole("navigation", { name: "Workspace" })).findByRole("button", { name: "Repository: orbyn-engineering" })).toBeVisible();
  const checkout = await findRow(/^orbyn\/1058/);
  expect(checkout).toHaveAttribute("aria-expanded", "true");
  expect(within(checkout).getByRole("img", { name: "This thread’s repository" })).toBeInTheDocument();
  // Its root is read through the thread's file tree route, with no repo_id: the primary repository.
  expect(await findRow(/^main\.ts/)).toBeInTheDocument();
  await waitFor(() => expect(reads).toHaveLength(1));
  const asked = new URL(reads[0]!, "http://x");
  expect(asked.pathname).toBe(`/api/threads/${THREAD}/files`);
  expect(asked.searchParams.get("path")).toBe("");
  expect(asked.searchParams.has("repo_id")).toBe(false);
  expect(reads.every((url) => !url.includes("repo_id="))).toBe(true);
});

it("switching loads the other repository's tree with repo_id, and each file opens as its repository's tab", async () => {
  const user = userEvent.setup();
  twoRepositories();
  const reads = serveTrees();
  render(<App />);
  await findRow(/^main\.ts/);

  await waitFor(() => expect(chooser()).toHaveAccessibleName("Repository: orbyn-engineering"));
  await user.click(chooser());
  await user.click(await screen.findByRole("menuitem", { name: "orbyn-planner" }));

  // The other repository's tree replaces the checkout row's: its own files, none of the primary's.
  expect(await findRow(/^planner\.ts/)).toBeInTheDocument();
  expect(maybeRow(/^main\.ts/)).toBeNull();
  expect(reads.some((url) => url.includes(`repo_id=${OTHER}`) && url.includes("path="))).toBe(true);
  const other = new URL(reads.find((url) => url.includes(`repo_id=${OTHER}`))!, "http://x");
  expect(other.pathname).toBe(`/api/threads/${THREAD}/files`);
  expect(other.searchParams.get("repo_id")).toBe(OTHER);
  expect(other.searchParams.get("path")).toBe("");

  // A folder of that repository is one level deeper, with the same repo_id ...
  await user.click(await findRow(/^plans/));
  await waitFor(() => expect(reads.some((url) => url.includes("path=plans") && url.includes(`repo_id=${OTHER}`))).toBe(true));
  // ... and its file opens as the thread's tab of that repository.
  await user.click(await findRow(/^plan\.md/));
  expect(useUi.getState().panelTab).toBe(fileTabId({ kind: "thread", path: "plans/plan.md", repoId: OTHER }));

  // The primary repository's own file opens as the thread's own tab, which names the repository: its read, write and
  // failure paths stay the thread's own, and its chip and Copy path say `<name>/<path>` rather than the task folder's.
  await waitFor(() => expect(chooser()).toHaveAccessibleName("Repository: orbyn-planner"));
  await user.click(chooser());
  await user.click(await screen.findByRole("menuitem", { name: "orbyn-engineering" }));
  await user.click(await findRow(/^main\.ts/));
  expect(useUi.getState().panelTab).toBe(fileTabId({ kind: "thread", path: "main.ts", repoId: PRIMARY, primary: true }));
});

it("an ordinary thread draws no chooser and asks for no repository tree", async () => {
  // No thread repositories: the service's 404, as an ordinary thread answers.
  setReposApi(createReposMock({ threads: {} }));
  const reads = serveTrees();
  render(<App />);

  // The project's worktree explorer, exactly as today.
  expect(await screen.findByText("README.md")).toBeInTheDocument();
  await waitFor(() => expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.some((call) => String(call[0]).includes("/worktrees"))).toBe(true));
  expect(within(screen.getByRole("navigation", { name: "Workspace" })).queryByRole("button", { name: /^Repository:/ })).toBeNull();
  expect(screen.queryByRole("img", { name: "This thread’s repository" })).toBeNull();
  expect(reads).toEqual([]);
});

it("a thread in one repository keeps today's explorer: no chooser and no repository tree read", async () => {
  setReposApi(createReposMock({ threads: { [THREAD]: [checkoutRepo(PRIMARY, "orbyn-engineering")] } }));
  const reads = serveTrees();
  render(<App />);

  expect(await screen.findByText("README.md")).toBeInTheDocument();
  expect(within(screen.getByRole("navigation", { name: "Workspace" })).queryByRole("button", { name: /^Repository:/ })).toBeNull();
  expect(reads).toEqual([]);
});

it("switching from a multi-repository thread to an ordinary one draws no chooser, no checkout row and reads no repository tree", async () => {
  const user = userEvent.setup();
  twoRepositories();
  const reads = serveTrees();
  render(<App />);
  expect(await findRow(/^main\.ts/)).toBeInTheDocument();

  await user.click(await findRow("Tool call states"));
  await waitFor(() => expect(window.location.hash).toBe("#/threads/t-tools"));
  // Nothing of the multi-repository thread's list is drawn for the ordinary one ...
  expect(within(sidebar()).queryByRole("button", { name: /^Repository:/ })).toBeNull();
  expect(screen.queryByRole("img", { name: "This thread’s repository" })).toBeNull();
  expect(await findRow(/^README\.md/)).toBeInTheDocument();
  // ... and a real pause shows the old list never reached the new thread's tree read.
  await new Promise((done) => setTimeout(done, 300));
  expect(reads.filter((url) => url.includes("/api/threads/t-tools/files"))).toEqual([]);
});

it("reads the thread's repository tree again on re-entry", async () => {
  const user = userEvent.setup();
  twoRepositories();
  const reads = serveTrees();
  render(<App />);
  await findRow(/^main\.ts/);
  await waitFor(() => expect(reads).toHaveLength(1));

  await user.click(await findRow("Tool call states"));
  await waitFor(() => expect(window.location.hash).toBe("#/threads/t-tools"));

  const before = reads.length;
  await user.click(await findRow("Explain how the event log resumes"));
  await waitFor(() => expect(window.location.hash).toBe(`#/threads/${THREAD}`));

  expect(await findRow(/^main\.ts/)).toBeInTheDocument();
  await waitFor(() => expect(reads.length).toBeGreaterThan(before));
});

it("reads the thread's repository tree again when the project's files change", async () => {
  const stop = startDataRefresh();
  twoRepositories();
  const reads = serveTrees();
  render(<App />);
  await findRow(/^main\.ts/);
  await waitFor(() => expect(reads).toHaveLength(1));

  act(() => dataChanged("explorer:p-app"));
  await waitFor(() => expect(reads).toHaveLength(2));
  stop();
});

it("reads the thread's repositories again when its Changes do, so one added to the running thread can be picked", async () => {
  const user = userEvent.setup();
  const mock = createReposMock({
    threads: { [THREAD]: [checkoutRepo(PRIMARY, "orbyn-engineering"), checkoutRepo(OTHER, "orbyn-planner")] },
    repositories: [projectRepo(PRIMARY, "orbyn-engineering", true), projectRepo(OTHER, "orbyn-planner", false), projectRepo("orbyn-docs", "orbyn-docs", false)],
  });
  setReposApi(mock);
  serveTrees();
  render(<App />);
  await waitFor(() => expect(chooser()).toHaveAccessibleName("Repository: orbyn-engineering"));

  await act(async () => {
    await mock.addThreadRepo(THREAD, "orbyn-docs");
    useThreadLogs.getState().touchDiff(THREAD);
  });
  await user.click(chooser());
  expect(await screen.findByRole("menuitem", { name: "orbyn-docs" })).toBeInTheDocument();
});
