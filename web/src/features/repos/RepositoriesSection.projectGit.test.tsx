import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Project } from "../../generated/Project";
import { useWorkspace } from "../../product/state/workspace";
import { RepositoriesSection } from "./RepositoriesSection";
import { setReposApi } from "./api";

/**
 * The project row of the Repositories group for a service that has no repository routes (`docs/multi-repo.md` off): one
 * repository, the project's own folder, and a folder added without Git offers Set up Git. The write is the project route
 * `POST /api/projects/{id}/git/init`; the repository route is not used when there is no repository to name.
 */

const project = (over: Partial<Project> = {}): Project => ({
  id: "p1",
  name: "orbyn-notes",
  path: "~/Documents/Github/orbyn-notes",
  trusted: true,
  is_git: false,
  created_at: "2026-11-20T00:00:00Z",
  managed: false,
  scratch: false,
  ...over,
});

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

/** The service's answer for the one repository read (`404`: no repository routes), and for the writes after it. */
function service(writes: string[], write?: () => Response) {
  const fetching = vi.fn(async (url: string, init?: RequestInit) => {
    const path = String(url).replace(/^.*\/api/, "/api");
    const method = init?.method ?? "GET";
    if (method === "GET" && path === "/api/projects/p1/repos") return json({ code: "not_found", message: "No such route." }, 404);
    writes.push(`${method} ${path}`);
    return write ? write() : json(project({ is_git: true }));
  });
  vi.stubGlobal("fetch", fetching);
  return fetching;
}

const show = (row: Project | null) => {
  useWorkspace.setState({ projects: { data: row === null ? [] : [row], error: null, loading: false } });
  return render(<RepositoriesSection projectId="p1" projectName="orbyn" />);
};

beforeEach(() => {
  setReposApi(null);
});

afterEach(() => {
  setReposApi(null);
  vi.unstubAllGlobals();
});

it("draws the project's folder with Not a Git repository and Set up Git when the service has no repository routes", async () => {
  const writes: string[] = [];
  const fetching = service(writes);
  const { container } = show(project());
  const row = (await screen.findByText("orbyn-notes")).closest("li") as HTMLElement;
  expect(within(row).getByText("~/Documents/Github/orbyn-notes")).toBeVisible();
  expect(within(row).getByText("Not a Git repository")).toBeVisible();
  expect(within(row).getByText("Folder")).toBeVisible();
  expect(within(row).getByRole("button", { name: "Set up Git" })).toBeVisible();
  // Nothing but the one repository read, and no write offered by drawing.
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  expect(writes).toEqual([]);
  expect(container.querySelectorAll("li")).toHaveLength(1);
});

it("draws nothing for a folder that is already a repository, or one that is not trusted yet: neither offers a write", async () => {
  const writes: string[] = [];
  const fetching = service(writes);
  const git = show(project({ is_git: true }));
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  await act(async () => { await Promise.resolve(); });
  expect(git.container).toBeEmptyDOMElement();
  git.unmount();

  const untrusted = show(project({ trusted: false }));
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(2));
  await act(async () => { await Promise.resolve(); });
  expect(untrusted.container).toBeEmptyDOMElement();
  expect(screen.queryByRole("button", { name: "Set up Git" })).toBeNull();
  expect(writes).toEqual([]);
});

it("draws nothing when the project is not known yet: the folder's state is never guessed", async () => {
  const writes: string[] = [];
  const fetching = service(writes);
  const { container } = show(null);
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  await act(async () => { await Promise.resolve(); });
  expect(container).toBeEmptyDOMElement();
});

it("sets up Git through the project route, once, after the dialog's confirmation, and the row goes when it is a repository", async () => {
  const writes: string[] = [];
  service(writes);
  show(project());
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Set up Git" }));
  const dialog = await screen.findByRole("dialog", { name: "Set up Git in orbyn-notes?" });
  expect(within(dialog).getByText("~/Documents/Github/orbyn-notes")).toBeVisible();
  expect(writes).toEqual([]);
  await user.dblClick(within(dialog).getByRole("button", { name: "Set up Git" }));
  await waitFor(() => expect(writes).toEqual(["POST /api/projects/p1/git/init"]));
  // The answer is the project row the route returns: the person's screen now has a repository, so the row is gone.
  await waitFor(() => expect(screen.queryByText("orbyn-notes")).toBeNull());
  expect(useWorkspace.getState().projects.data?.find((entry) => entry.id === "p1")?.is_git).toBe(true);
});

it("writing nothing on Cancel, and nothing when the dialog is opened", async () => {
  const writes: string[] = [];
  service(writes);
  show(project());
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Set up Git" }));
  const dialog = await screen.findByRole("dialog", { name: "Set up Git in orbyn-notes?" });
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(writes).toEqual([]);
});

it("keeps the dialog open and says the service's refusal in its own words, and offers another try", async () => {
  const writes: string[] = [];
  let refuse = true;
  service(writes, () =>
    refuse
      ? json({ code: "invalid", message: "This project is already a Git repository." }, 400)
      : json(project({ is_git: true })),
  );
  show(project());
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Set up Git" }));
  const dialog = await screen.findByRole("dialog", { name: "Set up Git in orbyn-notes?" });
  await user.click(within(dialog).getByRole("button", { name: "Set up Git" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("This project is already a Git repository.");
  expect(screen.getByText("orbyn-notes")).toBeVisible();
  refuse = false;
  await user.click(within(dialog).getByRole("button", { name: "Set up Git" }));
  await waitFor(() => expect(screen.queryByText("orbyn-notes")).toBeNull());
  expect(writes).toEqual(["POST /api/projects/p1/git/init", "POST /api/projects/p1/git/init"]);
});
