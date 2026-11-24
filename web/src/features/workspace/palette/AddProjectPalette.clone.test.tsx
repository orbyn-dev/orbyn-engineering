import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { api as client } from "../../../api/client";
import { installMockFetch } from "../../../test-support";
import { AddProjectPalette } from "./AddProjectPalette";

let mock: MockApi;

beforeEach(() => {
  mock = installMockFetch();
  mock.folders.set("C:\\Users\\me", {});
});
afterEach(() => vi.restoreAllMocks());

/** Add project → Clone a repository, with the field the dialog opened on. */
async function openClone(onClose: () => void) {
  const user = userEvent.setup();
  render(<AddProjectPalette onClose={onClose} onSubdialog={() => {}} />);
  await screen.findByRole("textbox", { name: "Folder path" });
  await user.click(await screen.findByRole("button", { name: /Clone a repository/ }));
  const dialog = await screen.findByRole("dialog", { name: "Clone a repository" });
  return { user, dialog, field: within(dialog).getByLabelText("Git URL or owner/name") as HTMLInputElement };
}

it("keeps the Clone dialog open with the reason for an address it cannot send, and sends nothing", async () => {
  const start = vi.spyOn(client, "startClone");
  const onClose = vi.fn();
  const { user, dialog, field } = await openClone(onClose);
  await user.type(field, "bad url with spaces");
  await user.click(within(dialog).getByRole("button", { name: "Clone" }));
  // The dialog's own error pattern: one alert line, and the field takes the focus back to be fixed.
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("The address cannot hold spaces or control characters.");
  expect(field).toHaveFocus();
  expect(dialog).toBeVisible();
  expect(onClose).not.toHaveBeenCalled();
  expect(start).not.toHaveBeenCalled();
});

it.each([
  // The flows the clone jobs take beyond a GitHub address: a local path (spaces and all) and an
  // http URL are sent as typed.
  ["/Users/me/My Projects/repo.git", "/Users/me/My Projects/repo.git"],
  ["http://127.0.0.1:1234/x.git", "http://127.0.0.1:1234/x.git"],
  ["orbyn-dev/orbyn-notes", "https://github.com/orbyn-dev/orbyn-notes.git"],
])("sends %s, without a validation stop", async (typed, sent) => {
  const start = vi.spyOn(client, "startClone");
  const { user, dialog, field } = await openClone(() => {});
  await user.type(field, typed);
  await user.click(within(dialog).getByRole("button", { name: "Clone" }));
  expect(start).toHaveBeenCalledWith(sent);
  expect(within(dialog).queryByRole("alert")).toBeNull();
});
