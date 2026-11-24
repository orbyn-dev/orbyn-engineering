import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { api as client } from "../../../api/client";
import { readShellStatus } from "../../../api/shell";
import { installMockFetch } from "../../../test-support";
import { AddProjectPalette } from "./AddProjectPalette";

let mock: MockApi;
/** The folders the service lists: each path holds the subfolders named, and the home folder answers a bare ask. */
const given = (home: string, folders: Record<string, string[]>) => mock.folders.set(home, folders);

beforeEach(() => {
  mock = installMockFetch();
  given("C:\\Users\\me", {});
});
afterEach(() => vi.unstubAllGlobals());

const open = async () => {
  render(<AddProjectPalette onClose={() => {}} onSubdialog={() => {}} />);
  const field = await screen.findByRole("textbox", { name: "Folder path" });
  return { field: field as HTMLInputElement, user: userEvent.setup() };
};

it("starts the field at the home root in the home folder's own style", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["Documents"] });
  const { field } = await open();
  await waitFor(() => expect(field).toHaveValue("~\\"));
});

it("keeps a POSIX home on its forward slashes", async () => {
  given("/Users/me", { "/Users/me": ["Documents"] });
  const { field } = await open();
  await screen.findByRole("button", { name: /Documents/ });
  expect(field).toHaveValue("~/");
});

it("writes the folder a person opens in the home's separators, not the other platform's", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["Documents"], "C:\\Users\\me\\Documents": [] });
  const { field, user } = await open();
  await user.click(await screen.findByRole("button", { name: /Documents/ }));
  await waitFor(() => expect(field).toHaveValue("~\\Documents\\"));
});

it("labels the row that goes up with the folder's name", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["Documents"], "C:\\Users\\me\\Documents": [] });
  const { user } = await open();
  await user.click(await screen.findByRole("button", { name: /Documents/ }));
  const above = await screen.findByRole("button", { name: "me" });
  expect(screen.queryByText("..")).toBeNull();
  // The row still goes up: it opens the folder it is named after.
  await user.click(above);
  expect(await screen.findByRole("button", { name: /Documents/ })).toBeVisible();
});

it("says a folder has none below it, even though the row above is there", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["All Users"], "C:\\Users\\me\\All Users": [] });
  const { user } = await open();
  await user.click(await screen.findByRole("button", { name: /All Users/ }));
  expect(await screen.findByText("No folders here")).toBeVisible();
  expect(screen.getByRole("button", { name: "me" })).toBeVisible();
});

it("says No results when the typed name matches no folder", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["Documents"] });
  const { field, user } = await open();
  await screen.findByRole("button", { name: /Documents/ });
  await user.type(field, "zzz");
  expect(await screen.findByText("No results")).toBeVisible();
});

type Win = { __TAURI_INTERNALS__?: { invoke: (command: string) => unknown } };
/** The page inside the desktop app, whose shell answers `pick_folder` with `answer`. */
async function inDesktop(answer: unknown) {
  vi.spyOn(client, "shell").mockResolvedValue({ attached: true, windows: 1, facts: { cli: null, permissions: [] } });
  const invoke = vi.fn(() => answer);
  (window as unknown as Win).__TAURI_INTERNALS__ = { invoke };
  await readShellStatus();
  return invoke;
}
afterEach(async () => {
  (window as unknown as Win).__TAURI_INTERNALS__ = undefined;
  vi.spyOn(client, "shell").mockResolvedValue({ attached: false, windows: 0, facts: { cli: null, permissions: [] } });
  await readShellStatus();
  vi.restoreAllMocks();
});

it("in the desktop app Browse opens the native dialog and only fills the field with a folder on another drive", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["Documents"], "D:\\work": ["app"] });
  const invoke = await inDesktop("D:\\work");
  const added = vi.spyOn(client, "addProject");
  const { field, user } = await open();
  await user.click(await screen.findByRole("button", { name: /Browse this computer/ }));
  await waitFor(() => expect(field).toHaveValue("D:\\work\\"));
  expect(invoke).toHaveBeenCalledWith("pick_folder");
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(await screen.findByRole("button", { name: /D:\\work\\app/ })).toBeVisible();
  // Picking never adds: that is the person's Enter on the field.
  expect(added).not.toHaveBeenCalled();
});

it("cancelling the native dialog leaves the field and the folder where they were", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["Documents"] });
  const invoke = await inDesktop(null);
  const { field, user } = await open();
  await screen.findByRole("button", { name: /Documents/ });
  await user.click(screen.getByRole("button", { name: /Browse this computer/ }));
  await waitFor(() => expect(invoke).toHaveBeenCalled());
  expect(field).toHaveValue("~\\");
  expect(screen.queryByRole("alert")).toBeNull();
});

it("a dialog that fails says so and keeps the field", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["Documents"] });
  await inDesktop(42);
  const { field, user } = await open();
  await screen.findByRole("button", { name: /Documents/ });
  await user.click(screen.getByRole("button", { name: /Browse this computer/ }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Type the path instead");
  expect(field).toHaveValue("~\\");
});

it("in a plain browser tab Browse opens the in-app browser at the folder the field names", async () => {
  given("C:\\Users\\me", { "C:\\Users\\me": ["Documents"], "D:\\work": ["app"], "D:\\work\\app": [] });
  const { field, user } = await open();
  await screen.findByRole("button", { name: /Documents/ });
  await user.clear(field);
  await user.type(field, "D:\\work\\");
  await screen.findByRole("button", { name: /app/ });
  await user.click(screen.getByRole("button", { name: /Browse this computer/ }));
  const dialog = await screen.findByRole("dialog");
  expect(await within(dialog).findByText("D:\\work")).toBeVisible();
});
