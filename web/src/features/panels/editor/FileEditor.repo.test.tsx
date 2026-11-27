import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createReposMock } from "../../../../mock/repos";
import { chipLabel } from "../../../product/composer/composerContext";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { fileTabId } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { setReposApi, type RepoRow, type ThreadRepo } from "../../repos";
import { FileTab } from "../files/FileTab";
import { Viewer } from "../files/FileViewer";
import { EditorTitleExtras } from "./EditorTitleExtras";
import { fileKey, saveSession, useEditorSessions } from "./editorSessions";
import { useFileTabMenu } from "./useFileTabMenu";

// F70 (#1055): a file or editor tab can name a repository of a multi-repository thread; every read and write of it carries that repo_id.
const TEXT = "# planner\n";
const REPO = "planner";

interface Served {
  /** What the read answers, by repo_id (`undefined` is the primary repository's). */
  content: Record<string, string>;
  read?: { status: number; body: unknown };
  put?: { status: number; body: unknown };
  editors?: unknown[];
  opens: { path: string; editor: string; repo_id?: string }[];
  gets: string[];
  puts: { path: string; content: string; base_revision: string | null; repo_id?: string }[];
}

function serve(options: Partial<Served> = {}) {
  const state: Served = { content: { planner: TEXT, primary: "primary text\n" }, editors: [], opens: [], gets: [], puts: [], ...options };
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    const url = String(input);
    if (url.includes("/api/editors")) return new Response(JSON.stringify(state.editors));
    if (url.includes("/files/open")) {
      state.opens.push(JSON.parse(String(init?.body)));
      return new Response(null, { status: 202 });
    }
    if (init?.method === "PUT") {
      const body = JSON.parse(String(init.body));
      state.puts.push(body);
      if (state.put) return new Response(JSON.stringify(state.put.body), { status: state.put.status });
      return new Response(JSON.stringify({ path: body.path, revision: "r2", content: body.content, size: 1, binary: false, truncated: false }));
    }
    if (url.includes("/files/content")) {
      state.gets.push(url);
      if (state.read) return new Response(JSON.stringify(state.read.body), { status: state.read.status });
      const repo = new URL(url, "http://x").searchParams.get("repo_id") ?? "primary";
      return new Response(JSON.stringify({ path: "notes.txt", revision: `r-${repo}`, content: state.content[repo] ?? "", size: 10, binary: false, truncated: false }));
    }
    return new Response("{}", { status: 404 });
  }));
  return state;
}

const repos: ThreadRepo[] = [
  { repo_id: "p", name: "app", branch: "main", added: 0, deleted: 0, trusted: true, files: [] },
  { repo_id: REPO, name: "orbyn-planner", branch: "main", added: 0, deleted: 0, trusted: false, files: null },
];
const row = (id: string, trusted: boolean): RepoRow => ({ id, name: id === "p" ? "app" : "orbyn-planner", path: `~/Github/${id}`, is_git: true, trusted, branch: "main", branches: ["main"], has_remote: false, primary: id === "p", clone: null });

beforeEach(() => {
  useEditorSessions.setState({ sessions: {}, locked: {} });
  useComposerChips.setState({ chips: {} });
  useWorkspace.setState({ threads: { t: { id: "t", project_id: "p", cwd: "/repo" } as never } });
  HTMLDialogElement.prototype.showModal ??= function showModal(this: HTMLDialogElement) { this.setAttribute("open", ""); };
});
afterEach(() => { setReposApi(null); vi.restoreAllMocks(); });

const planner = () => screen.findByRole("textbox", { name: "Edit notes.txt" });

it("reads, saves and takes its revision from the repository named, and a primary tab sends no repo_id at all", async () => {
  const served = serve();
  const key = fileKey("t", "/repo", "notes.txt", REPO);
  render(<><EditorTitleExtras threadId="t" tabId="planner" onSave={() => void saveSession(key)} /><Viewer threadId="t" tabId="planner" path="notes.txt" repoId={REPO} line={null} /></>);
  const box = await planner();
  expect(box).toHaveValue(TEXT);
  expect(served.gets).toHaveLength(1);
  expect(served.gets[0]).toContain("repo_id=planner");
  await userEvent.type(box, "!");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(served.puts).toHaveLength(1));
  // The revision is the repository's own, and repo_id is in the body.
  expect(served.puts[0]).toEqual({ path: "notes.txt", content: `${TEXT}!`, base_revision: "r-planner", repo_id: REPO });
  expect(useEditorSessions.getState().sessions[key]?.repoId).toBe(REPO);
});

it("an ordinary or primary tab reads and writes exactly as before: no repo_id in the query or the body, and the same session key", async () => {
  const served = serve();
  const key = fileKey("t", "/repo", "notes.txt");
  expect(key).toBe(JSON.stringify(["t", "/repo", "notes.txt"]));
  render(<><EditorTitleExtras threadId="t" tabId="main" onSave={() => void saveSession(key)} /><Viewer threadId="t" tabId="main" path="notes.txt" line={null} /></>);
  const box = await planner();
  expect(box).toHaveValue("primary text\n");
  expect(served.gets[0]).not.toContain("repo_id");
  await userEvent.type(box, "!");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(served.puts).toHaveLength(1));
  expect(served.puts[0]).toEqual({ path: "notes.txt", content: "primary text\n!", base_revision: "r-primary" });
  expect("repo_id" in served.puts[0]!).toBe(false);
});

it("the same path in two repositories is two sessions that never share text", async () => {
  serve();
  render(<><Viewer threadId="t" tabId="a" path="notes.txt" line={null} /><Viewer threadId="t" tabId="b" path="notes.txt" repoId={REPO} line={null} /></>);
  await waitFor(() => expect(screen.getAllByRole("textbox", { name: "Edit notes.txt" })).toHaveLength(2));
  const sessions = useEditorSessions.getState().sessions;
  expect(Object.keys(sessions).sort()).toEqual([fileKey("t", "/repo", "notes.txt"), fileKey("t", "/repo", "notes.txt", REPO)].sort());
  expect(sessions[fileKey("t", "/repo", "notes.txt")]?.text).toBe("primary text\n");
  expect(sessions[fileKey("t", "/repo", "notes.txt", REPO)]?.text).toBe(TEXT);
});

it("another repository's revision refused on a write keeps the conflict handling: Changed on disk, Keep mine and Reload, with repo_id on the reload", async () => {
  const served = serve({ put: { status: 409, body: { code: "file_conflict", message: "changed", detail: { revision: "r9" } } } });
  const key = fileKey("t", "/repo", "notes.txt", REPO);
  render(<><EditorTitleExtras threadId="t" tabId="planner" onSave={() => void saveSession(key)} /><Viewer threadId="t" tabId="planner" path="notes.txt" repoId={REPO} line={null} /></>);
  await userEvent.type(await planner(), "!");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText("Changed on disk")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Trust repository…" })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Keep mine" }));
  await waitFor(() => expect(served.puts).toHaveLength(2));
  expect(served.puts[1]).toMatchObject({ base_revision: "r9", repo_id: REPO });
});

it("a write to an untrusted repository is not a changed file: it shows the service's message and the existing trust offer, and keeps the text", async () => {
  const user = userEvent.setup();
  setReposApi(createReposMock({ threads: { t: repos }, repositories: [row("p", true), row(REPO, false)] }));
  const served = serve({ put: { status: 409, body: { code: "repo_untrusted", message: "orbyn-planner is not trusted." } } });
  const key = fileKey("t", "/repo", "notes.txt", REPO);
  render(<><EditorTitleExtras threadId="t" tabId="planner" onSave={() => void saveSession(key)} /><Viewer threadId="t" tabId="planner" path="notes.txt" repoId={REPO} line={null} /></>);
  await user.type(await planner(), "!");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText(/orbyn-planner is not trusted\./)).toBeVisible();
  expect(screen.queryByText("Changed on disk")).toBeNull();
  expect(useEditorSessions.getState().sessions[key]?.text).toBe(`${TEXT}!`);
  await user.click(screen.getByRole("button", { name: "Trust repository…" }));
  expect(await screen.findByRole("dialog")).toBeVisible();
  // Nothing was written by opening the offer, and the refused write is not repeated.
  expect(served.puts).toHaveLength(1);
});

it("a read of an untrusted repository offers trust and draws no file", async () => {
  setReposApi(createReposMock({ threads: { t: repos }, repositories: [row("p", true), row(REPO, false)] }));
  serve({ read: { status: 409, body: { code: "repo_untrusted", message: "orbyn-planner is not trusted." } } });
  render(<Viewer threadId="t" tabId="planner" path="notes.txt" repoId={REPO} line={null} />);
  expect(await screen.findByText(/orbyn-planner is not trusted\./)).toBeVisible();
  expect(screen.getByRole("button", { name: "Trust repository…" })).toBeVisible();
  expect(screen.queryByRole("textbox")).toBeNull();
});

it("a path outside the repository, and a repository that is not the thread's, say so", async () => {
  serve({ read: { status: 400, body: { code: "invalid", message: "That path is outside the repository." } } });
  const { unmount } = render(<Viewer threadId="t" tabId="x" path="../secret" repoId={REPO} line={null} />);
  expect(await screen.findByText("That path is outside the repository.")).toBeVisible();
  unmount();
  serve({ read: { status: 404, body: { code: "not_found", message: "No such repository." } } });
  render(<Viewer threadId="t" tabId="y" path="notes.txt" repoId="ghost" line={null} />);
  expect(await screen.findByText("This file isn't in this repository.")).toBeVisible();
});

it("another repository's image is drawn from the raw route, named with its repo_id", async () => {
  serve();
  render(<Viewer threadId="t" tabId="img" path="logo.png" repoId={REPO} line={null} />);
  const img = await screen.findByRole("img");
  expect(new URL(img.getAttribute("src")!, "http://x").searchParams.get("repo_id")).toBe(REPO);
});

it("a file tab of the thread opens the repository's file, and a thread-less panel draws nothing for it", async () => {
  const served = serve();
  const id = fileTabId({ kind: "thread", path: "notes.txt", repoId: REPO });
  const { unmount } = render(<FileTab id={id} threadId="t" />);
  expect(await planner()).toHaveValue(TEXT);
  expect(served.gets[0]).toContain("repo_id=planner");
  unmount();
  const { container } = render(<FileTab id={id} threadId={null} />);
  expect(container).toBeEmptyDOMElement();
});

const VSCODE = { id: "vscode", name: "VS Code", launch_style: "goto", file_manager: false };

function RepoMenu({ tab }: { tab: string }) {
  const items = useFileTabMenu("t", tab, () => undefined);
  return (
    <>
      {items?.map((item) => (
        <div key={item.label}>
          <button onClick={item.onSelect}>{item.label}</button>
          {item.children?.map((child) => (
            <button key={child.label} onClick={child.onSelect}>{child.label}</button>
          ))}
        </div>
      ))}
    </>
  );
}

it("offers the outside editor on another repository's file tab, with its repo-relative path and repo_id", async () => {
  const served = serve({ editors: [VSCODE] });
  const tab = fileTabId({ kind: "thread", path: "notes.txt", repoId: REPO });
  render(<><Viewer threadId="t" tabId="planner" path="notes.txt" repoId={REPO} line={null} /><RepoMenu tab={tab} /></>);
  await planner();
  await waitFor(() => expect(screen.getByRole("button", { name: "Open in an outside editor" })).toBeVisible());
  await userEvent.click(await screen.findByRole("button", { name: "VS Code" }));
  await waitFor(() => expect(served.opens).toHaveLength(1));
  expect(served.opens[0]).toEqual({ path: "notes.txt", editor: "vscode", repo_id: REPO });
});

it("keeps the outside editor for a primary file tab without repo_id", async () => {
  const served = serve({ editors: [VSCODE] });
  const tab = fileTabId({ kind: "thread", path: "notes.txt" });
  render(<><Viewer threadId="t" tabId="main" path="notes.txt" line={null} /><RepoMenu tab={tab} /></>);
  await planner();
  await userEvent.click(await screen.findByRole("button", { name: "VS Code" }));
  await waitFor(() => expect(served.opens).toHaveLength(1));
  expect(served.opens[0]).toEqual({ path: "notes.txt", editor: "vscode" });
});

it("comments on a line of another repository's file with its repo_id and name, and shows only that repository's comments", async () => {
  serve();
  render(<Viewer threadId="t" tabId="planner" path="notes.txt" repoId={REPO} repoName="orbyn-planner" line={null} />);
  const box = await planner();
  act(() => box.focus());
  await userEvent.click(screen.getByRole("button", { name: "Comment on line 1" }));
  await userEvent.type(screen.getByRole("textbox", { name: "File comment" }), "why here?");
  await userEvent.click(screen.getByRole("button", { name: "Add comment" }));

  const chip = useComposerChips.getState().chips.t![0]!;
  // The comment names the repository of its tab, so the next message points at the right notes.txt of the thread.
  expect(chip).toMatchObject({ kind: "review", path: "notes.txt", line: 1, repoId: REPO, repoName: "orbyn-planner", sourceRevision: "r-planner", sourceCwd: "/repo" });
  expect(chipLabel(chip)).toBe("orbyn-planner/notes.txt:1");
  expect(screen.getByRole("list", { name: "File comments" })).toHaveTextContent("why here?");

  // The same path and revision of the thread's own checkout is another file with another comment: never listed here.
  act(() => useComposerChips.getState().add("t", { id: "primary", kind: "review", path: "notes.txt", line: 1, comment: "primary note", excerpt: "primary text", sourceRevision: "r-planner", sourceCwd: "/repo" }));
  expect(screen.getByRole("list", { name: "File comments" })).not.toHaveTextContent("primary note");
});

it("moves a repository's file comment with its line on save, and leaves another repository's comment alone", async () => {
  const served = serve();
  const key = fileKey("t", "/repo", "notes.txt", REPO);
  render(<><EditorTitleExtras threadId="t" tabId="planner" onSave={() => void saveSession(key)} /><Viewer threadId="t" tabId="planner" path="notes.txt" repoId={REPO} line={null} /></>);
  await planner();
  act(() => {
    useComposerChips.getState().add("t", { id: "mine", kind: "review", path: "notes.txt", line: 1, endLine: 1, comment: "here", excerpt: "# planner", sourceRevision: "r-planner", sourceCwd: "/repo", repoId: REPO, repoName: "orbyn-planner" });
    useComposerChips.getState().add("t", { id: "primary", kind: "review", path: "notes.txt", line: 1, endLine: 1, comment: "primary", excerpt: "primary text", sourceRevision: "r-planner", sourceCwd: "/repo" });
  });
  act(() => useEditorSessions.getState().patch(key, { text: `// header\n${TEXT}` }));
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(served.puts).toHaveLength(1));
  const chips = useComposerChips.getState().chips.t!;
  expect(chips.find((chip) => chip.id === "mine")).toMatchObject({ line: 2, endLine: 2, sourceRevision: "r2", comment: "here" });
  expect(chips.find((chip) => chip.id === "primary")).toMatchObject({ line: 1, sourceRevision: "r-planner", comment: "primary" });
});
