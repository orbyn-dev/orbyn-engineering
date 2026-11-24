import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
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
