import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { fileTabId, useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { chooseRepo } from "../../repos";
import { CommandPalette } from "./CommandPalette";

// F70 (#1001): "Go to file" and "Search in project" read the thread's checkout in the repository the
// explorer's chip names, so another repository's file is found and opens as that repository's tab.
const OTHER = "planner";

const repos = [
  { repo_id: "p", name: "app", branch: "orbyn/1058", added: 0, deleted: 0, trusted: true, files: [] },
  { repo_id: OTHER, name: OTHER, branch: "orbyn/1058", added: 0, deleted: 0, trusted: true, files: [] },
];

/** Answers the thread's repository read and the file search, and keeps every URL asked. */
function serve() {
  const asked: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const url = String(input);
    asked.push(url);
    if (url.includes("/files/search")) return new Response(JSON.stringify([{ path: "plans/plan.md", line: 0, text: "" }]));
    if (url.includes("/api/threads/t/repos")) return new Response(JSON.stringify(repos));
    return new Response("[]");
  }));
  return { asked };
}

const searchAsks = (asked: string[]) => asked.filter((url) => url.includes("/files/search")).map((url) => new URL(url, "http://127.0.0.1").searchParams);
const repoIdAsked = (asked: string[]) => searchAsks(asked).map((params) => params.get("repo_id"));

beforeEach(() => {
  window.location.hash = "#/threads/t";
  useUi.setState({ panelTab: null, panelTabOrder: [] });
  useWorkspace.setState({ threads: { t: { id: "t", project_id: "p", cwd: "/repo" } } as never });
});
afterEach(() => {
  chooseRepo("t", null);
  vi.restoreAllMocks();
});

/** Types a query into "Go to file" and returns the list it drew. */
async function goToFile(query: string) {
  const user = userEvent.setup();
  render(<CommandPalette start="files" onClose={() => undefined} />);
  await user.type(await screen.findByRole("combobox"), query);
  return user;
}

it("searches the repository the explorer's chip names, and its file opens as that repository's tab", async () => {
  const { asked } = serve();
  chooseRepo("t", OTHER);
  const user = await goToFile("plan");
  const row = await screen.findByRole("button", { name: "plans/plan.md" });
  // The read names the chosen repository, never the primary one of the same thread.
  await waitFor(() => expect(repoIdAsked(asked)).toContain(OTHER));
  expect(repoIdAsked(asked).some((repoId) => repoId === null)).toBe(false);
  await user.click(row);
  expect(useUi.getState().panelTab).toBe(fileTabId({ kind: "thread", path: "plans/plan.md", repoId: OTHER }));
});

it("leaves the read and the tab with no repo_id for the primary repository, as before", async () => {
  const { asked } = serve();
  const user = await goToFile("plan");
  const row = await screen.findByRole("button", { name: "plans/plan.md" });
  await waitFor(() => expect(searchAsks(asked).length).toBeGreaterThan(0));
  expect(repoIdAsked(asked).every((repoId) => repoId === null)).toBe(true);
  await user.click(row);
  expect(useUi.getState().panelTab).toBe(fileTabId({ kind: "thread", path: "plans/plan.md" }));
});
