import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createReposMock } from "../../../mock/repos";
import type { RepoScan } from "./contract";
import { setReposApi } from "./api";
import { ScanDialog, type ScanMode } from "./Scan";

afterEach(() => { setReposApi(null); vi.unstubAllGlobals(); });

const open = (mode: ScanMode = "folder", mock = createReposMock()) => {
  setReposApi(mock);
  const changed = vi.fn();
  render(<ScanDialog projectId="p1" mode={mode} onClose={() => {}} onChanged={changed} />);
  return { mock, changed, dialog: screen.getByRole("dialog", { name: mode === "folder" ? "Scan a folder" : "Import a .code-workspace file" }) };
};
const scanOf = async (dialog: HTMLElement, path: string, run = "Scan") => {
  const user = userEvent.setup();
  await user.type(within(dialog).getByRole("textbox"), path);
  await user.click(within(dialog).getByRole("button", { name: run }));
  return { user, results: await within(dialog).findByRole("region", { name: "Scan results" }) };
};

it("scans only when asked, lists repo names with their state, and writes nothing", async () => {
  const { mock, dialog } = open();
  expect(mock.scans).toEqual([]);
  const { results } = await scanOf(dialog, "/work");
  expect(mock.scans).toEqual(["scan:/work"]);
  expect(mock.writes).toEqual([]);
  expect(within(results).getByText("orbyn-docs")).toBeVisible();
  expect(within(results).getByText("Already in a project")).toBeVisible();
  expect(within(results).getByRole("button", { name: "Add orbyn-docs" })).toBeEnabled();
  expect(within(results).queryByRole("button", { name: "Add orbyn-planner" })).toBeNull();
});

it("says in words why an imported workspace folder cannot be added, and offers no Add for it", async () => {
  const { dialog } = open("workspace");
  const { results } = await scanOf(dialog, "/w/x.code-workspace", "Read file");
  expect(within(results).getByText(/Outside the workspace file's folder/)).toBeVisible();
  expect(within(results).queryByRole("button", { name: "Add shared" })).toBeNull();
  expect(within(dialog).queryByRole("button", { name: "Browse…" })).toBeNull();
});

it("says each refusal and notes a truncated list", async () => {
  const scan = (): RepoScan => ({ path: "/w", is_git: false, code_workspace_file: "/w/a.code-workspace", truncated: true, folders: [
    { name: "gone", path: "/w/gone", is_git: false, source: "code_workspace", in_project: false, refused: "missing" },
    { name: "file", path: "/w/file", is_git: false, source: "code_workspace", in_project: false, refused: "not_directory" },
  ] });
  const { dialog } = open("workspace", createReposMock({ scan }));
  const { results } = await scanOf(dialog, "/w/a.code-workspace", "Read file");
  expect(within(results).getByText("This folder does not exist.")).toBeVisible();
  expect(within(results).getByText("This is a file, not a folder.")).toBeVisible();
  expect(within(results).getByRole("note")).toHaveTextContent("There are more folders than are listed here.");
});

it("adds one result with one write even on a double click, then waits for the trust prompt", async () => {
  const { mock, changed, dialog } = open();
  const { user, results } = await scanOf(dialog, "/work");
  await user.dblClick(within(results).getByRole("button", { name: "Add orbyn-docs" }));
  await waitFor(() => expect(mock.writes).toEqual(["add:/work/orbyn-docs"]));
  expect(await within(results).findByText("Added, not trusted")).toBeVisible();
  expect(changed).toHaveBeenCalledTimes(1);
  expect(within(results).queryByRole("button", { name: "Add orbyn-docs" })).toBeNull();
  await user.click(within(results).getByRole("button", { name: "Trust…" }));
  const prompt = await screen.findByRole("dialog", { name: /orbyn-docs/ });
  expect(mock.writes).toEqual(["add:/work/orbyn-docs"]);
  await user.click(within(prompt).getByRole("button", { name: /^(Trust|Yes)/ }));
  await waitFor(() => expect(mock.writes).toEqual(["add:/work/orbyn-docs", "trust:orbyn-docs"]));
  expect(await within(results).findByText("Added, trusted")).toBeVisible();
});

it("shows why an add failed, keeps the row addable and writes once", async () => {
  const { mock, dialog } = open();
  const { user, results } = await scanOf(dialog, "/work");
  mock.failNext("add");
  await user.click(within(results).getByRole("button", { name: "Add orbyn-docs" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Couldn't add orbyn-docs. The service could not do that.");
  expect(within(results).getByRole("button", { name: "Add orbyn-docs" })).toBeEnabled();
});

it("keeps the newest scan when an older one answers late", async () => {
  let release: (value: RepoScan | null) => void = () => {};
  const slow = new Promise<RepoScan | null>((resolve) => { release = resolve; });
  const mock = createReposMock();
  const real = mock.scan;
  let calls = 0;
  mock.scan = (project, path, signal) => (++calls === 1 ? slow : real(project, path, signal));
  const { dialog } = open("folder", mock);
  const user = userEvent.setup();
  const field = within(dialog).getByRole("textbox");
  await user.type(field, "/old");
  await user.click(within(dialog).getByRole("button", { name: "Scan" }));
  await user.clear(field);
  await user.type(field, "/new");
  await user.click(within(dialog).getByRole("button", { name: "Scan" }));
  const results = await within(dialog).findByRole("region", { name: "Scan results" });
  await act(async () => { release({ path: "/old", is_git: false, code_workspace_file: null, truncated: false, folders: [{ name: "stale", path: "/old/stale", is_git: true, source: "child", in_project: false, refused: null }] }); await Promise.resolve(); });
  expect(within(results).queryByText("stale")).toBeNull();
  expect(within(results).getByText("orbyn-docs")).toBeVisible();
});

it("renders hostile service text as text", async () => {
  const scan = (): RepoScan => ({ path: "/w", is_git: false, code_workspace_file: null, truncated: false, folders: [
    { name: "<img src=x onerror=alert(1)>", path: "/w/<b>x</b>", is_git: true, source: "child", in_project: false, refused: null },
  ] });
  const { dialog } = open("folder", createReposMock({ scan }));
  const { results } = await scanOf(dialog, "/w");
  expect(within(results).getByText("<img src=x onerror=alert(1)>")).toBeVisible();
  expect(results.querySelector("img, b")).toBeNull();
});

it("says so, with no error, when the service has no scan route", async () => {
  const fetching = vi.fn(async () => new Response(JSON.stringify({ code: "not_found", message: "x" }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  setReposApi(null);
  render(<ScanDialog projectId="p1" mode="folder" onClose={() => {}} onChanged={() => {}} />);
  const dialog = screen.getByRole("dialog", { name: "Scan a folder" });
  const user = userEvent.setup();
  await user.type(within(dialog).getByRole("textbox"), "/w");
  await user.click(within(dialog).getByRole("button", { name: "Scan" }));
  expect(await within(dialog).findByRole("status")).toHaveTextContent("cannot scan a folder");
  expect(within(dialog).queryByRole("alert")).toBeNull();
});
