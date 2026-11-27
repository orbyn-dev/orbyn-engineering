import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { fileTabId, useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { useEditorSessions } from "../editor/editorSessions";
import { FileTab } from "./FileTab";

// #1581: a task-folder reference names a repository by its first segment, and the answer's repo_id is that
// repository, so the file opens as that repository's tab, whose read carries its repo_id.
const REPO = "planner";

beforeEach(() => {
  window.location.hash = "#/threads/t";
  useEditorSessions.setState({ sessions: {}, locked: {} });
  useUi.setState({ fileReveal: null });
  useWorkspace.setState({ threads: { t: { id: "t", project_id: "p", cwd: "/task" } as never } });
});
afterEach(() => {
  vi.restoreAllMocks();
});

/** Answers the resolution with the repository given, the worktree list empty and the file's bytes. */
function serve(repoId: string) {
  const asked: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const url = String(input);
    asked.push(url);
    if (url.endsWith("/paths/resolve")) {
      return new Response(JSON.stringify({ cwd: "/task", checkout_path: "notes.ts", repository_path: "notes.ts", repo_id: repoId }));
    }
    if (url.includes("worktrees")) return new Response(JSON.stringify({ worktrees: [] }));
    if (url.includes("/api/editors")) return new Response("[]");
    return new Response(JSON.stringify({ path: "notes.ts", revision: "r1", content: "export {};\n", size: 10, binary: false, truncated: false }));
  }));
  return asked;
}

const readPath = (asked: string[]) => new URL(asked.find((url) => url.includes("/files/content"))!, "http://127.0.0.1").searchParams;

it("opens the repository's tab for a task-folder reference that names another repository", async () => {
  const target = { kind: "thread", path: `${REPO}/notes.ts` } as const;
  const taskFolderTab = fileTabId(target);
  const repoTab = fileTabId({ kind: "thread", path: "notes.ts", repoId: REPO });
  serve(REPO);
  // The task-folder tab is open, so its removal is what the handoff does and not an empty order.
  useUi.getState().openFileTab(target);
  expect(useUi.getState().panelTabOrder).toContain(taskFolderTab);
  const view = render(<FileTab id={taskFolderTab} threadId="t" />);

  // The tab that opens is that repository's, at the path inside its worktree, and the task-folder tab is gone.
  await waitFor(() => expect(useUi.getState().panelTab).toBe(repoTab));
  expect(useUi.getState().panelTabOrder).toContain(repoTab);
  expect(useUi.getState().panelTabOrder).not.toContain(taskFolderTab);
  view.unmount();

  // Its read names the repository, so the bytes are that worktree's file and never the primary's.
  const asked = serve(REPO);
  render(<FileTab id={repoTab} threadId="t" />);
  expect(await screen.findByRole("textbox", { name: "Edit notes.ts" })).toBeInTheDocument();
  expect(readPath(asked).get("repo_id")).toBe(REPO);
  expect(readPath(asked).get("path")).toBe("notes.ts");
});

it("keeps the thread's own tab when the reference resolves in its own checkout", async () => {
  // The primary repository is the thread's own checkout, so its coordinates open the tab it always has.
  const taskFolderTab = fileTabId({ kind: "thread", path: "notes.ts" });
  const asked = serve("p");
  render(<FileTab id={taskFolderTab} threadId="t" />);

  expect(await screen.findByRole("textbox", { name: "Edit notes.ts" })).toBeInTheDocument();
  expect(useUi.getState().panelTabOrder).not.toContain(fileTabId({ kind: "thread", path: "notes.ts", repoId: "p" }));
  expect(readPath(asked).has("repo_id")).toBe(false);
});
