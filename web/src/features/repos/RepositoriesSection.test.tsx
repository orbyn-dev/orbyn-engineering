import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createReposMock, repoFixtures } from "../../../mock/repos";
import { RepositoriesSection } from "./RepositoriesSection";
import { setReposApi } from "./api";

afterEach(() => { setReposApi(null); vi.unstubAllGlobals(); });

const show = (mock = createReposMock()) => {
  setReposApi(mock);
  render(<RepositoriesSection projectId="p1" projectName="orbyn" />);
  return mock;
};
const rowOf = async (name: string) => (await screen.findByText(name)).closest("li") as HTMLElement;

it("draws nothing, and no error, when the service answers the list with 404 (flag off or routes missing); the only request is that read", async () => {
  const fetching = vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify({ code: "not_found", message: "No such route." }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  const { container } = render(<RepositoriesSection projectId="p1" projectName="orbyn" />);
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  await act(async () => { await Promise.resolve(); });
  expect(container).toBeEmptyDOMElement();
  expect(fetching.mock.calls.map((call) => `${call[1]?.method} ${String(call[0]).replace(/^.*\/api/, "/api")}`)).toEqual(["GET /api/projects/p1/repos"]);
});

it("lists each repository with its folder, branch and trust state, and the action that state offers", async () => {
  show();
  const engineering = await rowOf("orbyn-engineering");
  expect(within(engineering).getByText("~/Documents/Github/orbyn-engineering")).toBeVisible();
  expect(within(engineering).getByText("main")).toBeVisible();
  expect(within(engineering).getByText("Trusted")).toBeVisible();
  expect(within(engineering).queryByRole("button", { name: "Trust…" })).toBeNull();
  const engine = await rowOf("orbyn-engine");
  expect(within(engine).getByText("Not trusted")).toBeVisible();
  expect(within(engine).getByRole("button", { name: "Trust…" })).toBeVisible();
  const notes = await rowOf("orbyn-notes");
  expect(within(notes).getByText("Not a Git folder")).toBeVisible();
  expect(within(notes).getByText("No branch")).toBeVisible();
  expect(within(notes).getByRole("button", { name: "Set up Git" })).toBeVisible();
  expect(screen.getByText("Beta")).toBeVisible();
});

it("offers Publish only on a repository with no remote, and Trust only while it is not trusted", async () => {
  show();
  const user = userEvent.setup();
  await user.click(within(await rowOf("orbyn-engineering")).getByRole("button", { name: "Actions for orbyn-engineering" }));
  expect(await screen.findAllByRole("menuitem")).toHaveLength(1);
  expect(screen.getByRole("menuitem", { name: "Remove from project…" })).toBeVisible();
  await user.keyboard("{Escape}");
  await user.click(within(await rowOf("orbyn-planner")).getByRole("button", { name: "Actions for orbyn-planner" }));
  expect(await screen.findByRole("menuitem", { name: "Publish to GitHub…" })).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: "Trust this folder…" })).toBeNull();
  await user.keyboard("{Escape}");
  await user.click(within(await rowOf("orbyn-engine")).getByRole("button", { name: "Actions for orbyn-engine" }));
  expect(await screen.findByRole("menuitem", { name: "Trust this folder…" })).toBeVisible();
});

it("offers no Publish on a repository that is not trusted: publishing pushes", async () => {
  show(createReposMock({ repositories: repoFixtures().map((row) => (row.id === "orbyn-planner" ? { ...row, trusted: false } : row)) }));
  const user = userEvent.setup();
  await user.click(within(await rowOf("orbyn-planner")).getByRole("button", { name: "Actions for orbyn-planner" }));
  expect(await screen.findByRole("menuitem", { name: "Trust this folder…" })).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: "Publish to GitHub…" })).toBeNull();
});

it("removes a repository only after the dialog's Remove, once, and says the files stay", async () => {
  const mock = show();
  const user = userEvent.setup();
  await user.click(within(await rowOf("orbyn-planner")).getByRole("button", { name: "Actions for orbyn-planner" }));
  await user.click(await screen.findByRole("menuitem", { name: "Remove from project…" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove orbyn-planner from orbyn?" });
  expect(within(dialog).getByText("Orbyn stops tracking it in this project. Files on disk stay, and threads keep their history.")).toBeVisible();
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus());
  expect(mock.writes).toEqual([]);
  await user.dblClick(within(dialog).getByRole("button", { name: "Remove" }));
  await waitFor(() => expect(screen.queryByText("orbyn-planner")).toBeNull());
  expect(mock.writes).toEqual(["remove:orbyn-planner"]);
});

it("cancelling a dialog writes nothing", async () => {
  const mock = show();
  const user = userEvent.setup();
  await user.click(within(await rowOf("orbyn-engine")).getByRole("button", { name: "Trust…" }));
  const dialog = await screen.findByRole("dialog", { name: "Trust orbyn-engine?" });
  expect(within(dialog).getByText("Agents can read files, run commands and edit code in this folder. Trust folders you know.")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Don’t trust" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(mock.writes).toEqual([]);
});

it("trusts the one folder and no other", async () => {
  const mock = show();
  const user = userEvent.setup();
  await user.click(within(await rowOf("orbyn-engine")).getByRole("button", { name: "Trust…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Trust folder" }));
  await waitFor(() => expect(within(screen.getByText("orbyn-engine").closest("li") as HTMLElement).getByText("Trusted")).toBeVisible());
  expect(mock.writes).toEqual(["trust:orbyn-engine"]);
  expect(within(screen.getByText("orbyn-notes").closest("li") as HTMLElement).getByText("Not a Git folder")).toBeVisible();
});

it("sets up Git after a confirmation", async () => {
  const mock = show();
  const user = userEvent.setup();
  await user.click(within(await rowOf("orbyn-notes")).getByRole("button", { name: "Set up Git" }));
  expect(mock.writes).toEqual([]);
  await user.click(within(await screen.findByRole("dialog", { name: "Set up Git in orbyn-notes?" })).getByRole("button", { name: "Set up Git" }));
  await waitFor(() => expect(mock.writes).toEqual(["git:orbyn-notes"]));
});

it("clones from a Git URL, naming the project it joins, and never from text Git could read as an option", async () => {
  const mock = show();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add repository" }));
  await user.click(await screen.findByRole("menuitem", { name: "Clone a repository…" }));
  const dialog = await screen.findByRole("dialog", { name: "Clone a repository" });
  expect(within(dialog).getByText("orbyn")).toBeVisible();
  const field = within(dialog).getByRole("textbox", { name: "Git URL" });
  await user.type(field, "--upload-pack=touch x");
  expect(within(dialog).getByRole("button", { name: "Clone" })).toBeDisabled();
  await user.clear(field);
  for (const refused of ["orbyn-dev/orbyn-docs", "ext::sh -c x", "file:///etc/passwd", "https://u:pw@host/x.git", "https://host/x\u0000y"]) {
    await user.clear(field);
    await user.type(field, refused);
    expect(within(dialog).getByRole("button", { name: "Clone" })).toBeDisabled();
  }
  await user.clear(field);
  await user.type(field, "https://github.com/orbyn-dev/orbyn-docs.git");
  await user.dblClick(within(dialog).getByRole("button", { name: "Clone" }));
  await waitFor(() => expect(mock.writes).toEqual(["clone:https://github.com/orbyn-dev/orbyn-docs.git"]));
  const added = await rowOf("orbyn-docs");
  expect(within(added).getByText("Cloning")).toBeVisible();
  expect(within(added).queryByText(/%/)).toBeNull();
  expect(within(added).queryByRole("progressbar")).toBeNull();
  expect(within(added).queryByRole("button", { name: "Trust…" })).toBeNull();
});

it("creates a new folder by a safe name only, and cancels a running clone after a confirm", async () => {
  const mock = show(createReposMock({ repositories: [...repoFixtures(), { ...repoFixtures()[0]!, id: "big", name: "big", primary: false, clone: { state: "cloning" } }] }));
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add repository" }));
  await user.click(await screen.findByRole("menuitem", { name: "Create a new folder…" }));
  const dialog = await screen.findByRole("dialog", { name: "Create a repository" });
  const field = within(dialog).getByRole("textbox", { name: "Folder name" });
  for (const bad of ["a/b", "..", "a..b", ".hidden", "C:x", "-x"]) {
    await user.clear(field);
    await user.type(field, bad);
    expect(within(dialog).getByRole("button", { name: "Create" })).toBeDisabled();
  }
  await user.clear(field);
  await user.type(field, "orbyn-docs");
  await user.dblClick(within(dialog).getByRole("button", { name: "Create" }));
  await waitFor(() => expect(mock.writes).toEqual(["create:orbyn-docs"]));
  await user.click(within(await rowOf("big")).getByRole("button", { name: "Cancel" }));
  expect(mock.writes).toEqual(["create:orbyn-docs"]);
  await user.click(within(await screen.findByRole("dialog", { name: "Cancel cloning big?" })).getByRole("button", { name: "Cancel clone" }));
  await waitFor(() => expect(mock.writes).toEqual(["create:orbyn-docs", "cancel-clone:big"]));
});

it("adds a folder in place, not trusted", async () => {
  const mock = show();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add repository" }));
  await user.click(await screen.findByRole("menuitem", { name: "Add a folder…" }));
  const dialog = await screen.findByRole("dialog", { name: "Add a folder" });
  expect(within(dialog).getByRole("button", { name: "Add" })).toBeDisabled();
  await user.type(within(dialog).getByRole("textbox", { name: "Folder on this computer" }), "~/code/orbyn-docs");
  await user.click(within(dialog).getByRole("button", { name: "Add" }));
  await waitFor(() => expect(mock.writes).toEqual(["add:~/code/orbyn-docs"]));
  expect(within(await rowOf("orbyn-docs")).getByText("Not trusted")).toBeVisible();
});

it("keeps the dialog open and says why when the write fails, and allows another try", async () => {
  const mock = show();
  mock.failNext("remove");
  const user = userEvent.setup();
  await user.click(within(await rowOf("orbyn-planner")).getByRole("button", { name: "Actions for orbyn-planner" }));
  await user.click(await screen.findByRole("menuitem", { name: "Remove from project…" }));
  const dialog = await screen.findByRole("dialog");
  await user.click(within(dialog).getByRole("button", { name: "Remove" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("The service could not do that.");
  expect(screen.getByText("orbyn-planner")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Remove" }));
  await waitFor(() => expect(screen.queryByText("orbyn-planner")).toBeNull());
  expect(mock.writes).toEqual(["remove:orbyn-planner", "remove:orbyn-planner"]);
});

it("shows path and names as text, never as markup", async () => {
  const hostile = { ...repoFixtures()[0]!, id: "x", name: "<img src=x onerror=alert(1)>", path: "/tmp/<b>bold</b>\nnext" };
  show(createReposMock({ repositories: [hostile] }));
  expect(await screen.findByText("<img src=x onerror=alert(1)>")).toBeVisible();
  expect(document.querySelector("img")).toBeNull();
  expect(document.querySelector("b")?.textContent).not.toBe("bold");
});

it("does not run anything on open: no write happens until a dialog's confirm button is pressed", async () => {
  const mock = show();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add repository" }));
  await user.click(await screen.findByRole("menuitem", { name: "Clone a repository…" }));
  await screen.findByRole("dialog", { name: "Clone a repository" });
  await user.keyboard("{Escape}");
  expect(mock.writes).toEqual([]);
});

it("a running clone draws no percentage or bar even when the answer carries a figure", async () => {
  show(createReposMock({ repositories: [...repoFixtures(), { ...repoFixtures()[0]!, id: "big", name: "big", primary: false, trusted: false, clone: { state: "cloning", percent: 55 } }] }));
  const big = await rowOf("big");
  expect(within(big).getByText("Cloning")).toBeVisible();
  expect(big.textContent).not.toMatch(/%/);
  expect(within(big).queryByRole("progressbar")).toBeNull();
  expect(within(big).queryByRole("button", { name: "Trust…" })).toBeNull();
});

it("a cancelled clone stays listed as failed, with no Trust offered for it, and Remove clears it", async () => {
  const mock = show(createReposMock({ repositories: [...repoFixtures(), { ...repoFixtures()[0]!, id: "big", name: "big", primary: false, trusted: false, clone: { state: "cloning" } }] }));
  const user = userEvent.setup();
  await user.click(within(await rowOf("big")).getByRole("button", { name: "Cancel" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Cancel clone" }));
  await waitFor(() => expect(within(screen.getByText("big").closest("li") as HTMLElement).getByText("Clone failed. Cancelled.")).toBeVisible());
  expect(mock.writes).toEqual(["cancel-clone:big"]);
  const big = screen.getByText("big").closest("li") as HTMLElement;
  expect(within(big).queryByRole("button", { name: "Trust…" })).toBeNull();
  await user.click(within(big).getByRole("button", { name: "Actions for big" }));
  expect(screen.queryByRole("menuitem", { name: "Trust this folder…" })).toBeNull();
  expect(screen.getByRole("menuitem", { name: "Remove from project…" })).toBeVisible();
});

it("offers the trust prompt for untrusted rows only: a folder or clone added untrusted shows it, a created folder starts trusted", async () => {
  const mock = show(createReposMock({ repositories: [repoFixtures()[0]!] }));
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add repository" }));
  await user.click(await screen.findByRole("menuitem", { name: "Create a new folder…" }));
  const create = await screen.findByRole("dialog", { name: "Create a repository" });
  expect(within(create).getByText(/It starts trusted/)).toBeVisible();
  expect(within(create).queryByText(/not trusted until/)).toBeNull();
  await user.type(within(create).getByRole("textbox", { name: "Folder name" }), "made");
  await user.click(within(create).getByRole("button", { name: "Create" }));
  const made = await rowOf("made");
  expect(within(made).getByText("Trusted")).toBeVisible();
  expect(within(made).queryByRole("button", { name: "Trust…" })).toBeNull();
  await user.click(await screen.findByRole("button", { name: "Add repository" }));
  await user.click(await screen.findByRole("menuitem", { name: "Add a folder…" }));
  const add = await screen.findByRole("dialog", { name: "Add a folder" });
  expect(within(add).getByText(/not trusted until you trust it/)).toBeVisible();
  await user.type(within(add).getByRole("textbox", { name: "Folder on this computer" }), "/work/added");
  await user.click(within(add).getByRole("button", { name: "Add" }));
  const added = await rowOf("added");
  expect(within(added).getByText("Not trusted")).toBeVisible();
  expect(within(added).getByRole("button", { name: "Trust…" })).toBeVisible();
  expect(mock.writes).toEqual(["create:made", "add:/work/added"]);
});

it("an untrusted folder with no Git is trusted first: Trust is offered and Set up Git is not, until it is trusted", async () => {
  show(createReposMock({ repositories: [repoFixtures()[0]!, { ...repoFixtures()[3]!, trusted: false }] }));
  const notes = await rowOf("orbyn-notes");
  expect(within(notes).getByText("Not trusted")).toBeVisible();
  expect(within(notes).getByRole("button", { name: "Trust…" })).toBeVisible();
  expect(within(notes).queryByRole("button", { name: "Set up Git" })).toBeNull();
});
