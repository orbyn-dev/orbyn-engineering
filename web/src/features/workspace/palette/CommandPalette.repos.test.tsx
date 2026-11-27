import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { flushSync } from "react-dom";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useRoute } from "../../../product/state/route";
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

/** The shell redraws the palette when the route changes, as the app's does. */
function Routed() {
  useRoute();
  return <CommandPalette start="files" onClose={() => undefined} />;
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

it("searches nothing until the repository list settles, so a pending list never searches the primary", async () => {
  // The list is held open while the person types: until it answers, the repository the chip names is unknown.
  const asked: string[] = [];
  let release = () => {};
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const url = String(input);
    asked.push(url);
    if (url.includes("/files/search")) return new Response(JSON.stringify([{ path: "plans/plan.md", line: 0, text: "" }]));
    if (url.includes("/api/threads/t/repos")) {
      await new Promise<void>((done) => { release = done; });
      return new Response(JSON.stringify(repos));
    }
    return new Response("[]");
  }));
  chooseRepo("t", OTHER);
  await goToFile("plan");
  await waitFor(() => expect(asked.some((url) => url.includes("/api/threads/t/repos"))).toBe(true));
  // Pending: not one search has been sent, so none of them was the primary's.
  expect(searchAsks(asked).length).toBe(0);
  release();
  // Settled: the read names the chosen repository, and the row it finds opens as that repository's tab.
  await waitFor(() => expect(repoIdAsked(asked)).toContain(OTHER));
  expect(repoIdAsked(asked).some((repoId) => repoId === null)).toBe(false);
  expect(await screen.findByRole("button", { name: "plans/plan.md" })).toBeTruthy();
});

it("opens a row in the repository it was found in when the chip moves on before the list redraws", async () => {
  // #1777 review: the rows on screen are the answer to the read the palette sent, and the chip's repository is read
  // from state that changes in the same commit. The old list is dropped by an effect, so one committed render shows the
  // rows found in `planner` while the chip already names the primary. flushSync commits that render without running the
  // effect, which is the window a selection lands in; the row must still open where it was found.
  const { asked } = serve();
  chooseRepo("t", OTHER);
  await goToFile("plan");
  const row = await screen.findByRole("button", { name: "plans/plan.md" });
  await waitFor(() => expect(repoIdAsked(asked)).toContain(OTHER));
  act(() => {
    flushSync(() => chooseRepo("t", null));
    // The row found in `planner` is still drawn against the primary scope, and this is the selection that lands in it.
    expect(row.isConnected).toBe(true);
    row.click();
  });
  expect(useUi.getState().panelTab).toBe(fileTabId({ kind: "thread", path: "plans/plan.md", repoId: OTHER }));
  expect(useUi.getState().fileReveal?.repoId).toBe(OTHER);
});

it("opens a row in the thread it was found in when browser Back changes the thread while it is open", async () => {
  // The reported path (#1777 review): Back moves the route under the mounted palette. The route is read during a render,
  // so one committed render draws the previous thread's rows while the new thread's checkout is current; the row must
  // open in the thread whose read found it, not in the checkout the route now names.
  const { asked } = serve();
  useWorkspace.setState({
    threads: { t: { id: "t", project_id: "p", cwd: "/repo" }, u: { id: "u", project_id: "p", cwd: "/other" } } as never,
  });
  const user = userEvent.setup();
  render(<Routed />);
  await user.type(await screen.findByRole("combobox"), "plan");
  const row = await screen.findByRole("button", { name: "plans/plan.md" });
  await waitFor(() => expect(repoIdAsked(asked).length).toBeGreaterThan(0));
  act(() => {
    window.location.hash = "#/threads/u";
    flushSync(() => window.dispatchEvent(new HashChangeEvent("hashchange")));
    // The row found in `t` is still drawn while the route names `u`.
    expect(row.isConnected).toBe(true);
    row.click();
  });
  expect(useUi.getState().fileReveal).toMatchObject({ threadId: "t", cwd: "/repo", path: "plans/plan.md" });
});

it("says the service's refusal and offers trust instead of an empty list when the repository is not trusted", async () => {
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const url = String(input);
    if (url.includes("/files/search")) return new Response(JSON.stringify({ code: "repo_untrusted", message: "orbyn-planner is not trusted." }), { status: 409 });
    if (url.includes("/api/threads/t/repos")) return new Response(JSON.stringify(repos));
    return new Response("[]");
  }));
  chooseRepo("t", OTHER);
  await goToFile("plan");
  expect(await screen.findByText("orbyn-planner is not trusted.")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Trust repository…" })).toBeTruthy();
  // The refusal is not drawn as an empty result list.
  expect(screen.queryByText("No results")).toBeNull();
});

