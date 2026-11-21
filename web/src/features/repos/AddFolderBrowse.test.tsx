import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";

const shell = vi.hoisted(() => ({ attached: true }));
vi.mock("../../api/shell", () => ({ inDesktopApp: () => shell.attached }));

import { createReposMock } from "../../../mock/repos";
import { AddFolderDialog } from "./Dialogs";
import { setReposApi } from "./api";

const bridgeWith = (invoke?: (command: string) => unknown) => { (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = invoke ? { invoke } : undefined; };
afterEach(() => { setReposApi(null); bridgeWith(); shell.attached = true; });

const open = () => {
  const mock = createReposMock();
  setReposApi(mock);
  render(<AddFolderDialog projectId="p1" onClose={() => {}} onDone={() => {}} />);
  return { mock, dialog: screen.getByRole("dialog", { name: "Add a folder" }) };
};

it("has no Browse button in a plain browser tab: the path field is the only way", () => {
  shell.attached = false;
  const { dialog } = open();
  expect(within(dialog).queryByRole("button", { name: "Browse…" })).toBeNull();
  expect(within(dialog).getByRole("textbox", { name: "Folder on this computer" })).toBeVisible();
});

it("fills the field from the native dialog and adds nothing until Add is pressed", async () => {
  bridgeWith(() => "/Users/me/orbyn-docs");
  const { mock, dialog } = open();
  const user = userEvent.setup();
  await user.click(within(dialog).getByRole("button", { name: "Browse…" }));
  await waitFor(() => expect(within(dialog).getByRole("textbox", { name: "Folder on this computer" })).toHaveValue("/Users/me/orbyn-docs"));
  expect(mock.writes).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Add" }));
  await waitFor(() => expect(mock.writes).toEqual(["add:/Users/me/orbyn-docs"]));
});

it("keeps what was typed when the dialog is cancelled", async () => {
  bridgeWith(() => null);
  const { dialog } = open();
  const user = userEvent.setup();
  await user.type(within(dialog).getByRole("textbox", { name: "Folder on this computer" }), "~/typed");
  await user.click(within(dialog).getByRole("button", { name: "Browse…" }));
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Browse…" })).not.toHaveAttribute("aria-busy"));
  expect(within(dialog).getByRole("textbox", { name: "Folder on this computer" })).toHaveValue("~/typed");
});

it("says to type the path when the shell cannot open the dialog, and opens it once per click", async () => {
  const invoke = vi.fn(() => { throw "unknown command"; });
  bridgeWith(invoke);
  const { dialog } = open();
  await userEvent.dblClick(within(dialog).getByRole("button", { name: "Browse…" }));
  expect(await within(dialog).findByText(/Type the path instead/)).toBeVisible();
  expect(invoke.mock.calls.length).toBeLessThanOrEqual(2);
});

it("never trusts or adds a picked path by itself, even a hostile-looking one", async () => {
  bridgeWith(() => "/tmp/<b>x</b>;rm -rf");
  const { mock, dialog } = open();
  await userEvent.click(within(dialog).getByRole("button", { name: "Browse…" }));
  await waitFor(() => expect(within(dialog).getByRole("textbox", { name: "Folder on this computer" })).toHaveValue("/tmp/<b>x</b>;rm -rf"));
  expect(mock.writes).toEqual([]);
  expect(document.querySelector("b")).toBeNull();
});
