import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createProfilesMock, profileFixtures } from "../../../mock/profiles";
import { ProfilesSettings } from "./ProfilesSettings";
import { setProfilesApi } from "./api";

afterEach(() => { setProfilesApi(null); vi.unstubAllGlobals(); });

async function menuOf(user: ReturnType<typeof userEvent.setup>, name: string, item: string) {
  await user.click(await screen.findByRole("button", { name: `Actions for ${name}` }));
  await user.click(await screen.findByRole("menuitem", { name: item }));
}

it("draws nothing, and no error, when the profiles route answers 404", async () => {
  const fetching = vi.fn(async () => new Response(JSON.stringify({ code: "not_found", message: "No such route." }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  const { container } = render(<ProfilesSettings />);
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(container).toBeEmptyDOMElement();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("lists the built-ins first, each as read-only, and the saved ones with their reason when the engine is missing", async () => {
  setProfilesApi(createProfilesMock());
  render(<ProfilesSettings />);
  const list = await screen.findByRole("list", { name: "Profiles" });
  const rows = within(list).getAllByRole("listitem");
  expect(rows.map((li) => li.querySelector(".prf-name")?.textContent)).toEqual(["Careful", "Fast", "Cheap", "Night shift", "Codex builder", "Hands off"]);
  expect(within(rows[0]!).getByText("Built-in")).toBeVisible();
  expect(within(rows[0]!).getByText("Keep the thread’s engine · Builder · Supervised · Autopilot Off")).toBeVisible();
  expect(within(rows[4]!).getByText("Engine not found")).toBeVisible();
  expect(within(rows[4]!).getByText(/Codex is not installed\./)).toBeVisible();
  expect(screen.queryByText(/read.only mode/i)).toBeNull();
});

it("offers Edit and Delete only on a saved profile, and only Duplicate on a built-in", async () => {
  setProfilesApi(createProfilesMock());
  render(<ProfilesSettings />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Actions for Careful" }));
  expect(await screen.findByRole("menuitem", { name: "Duplicate" })).toBeVisible();
  expect(screen.queryByRole("menuitem", { name: "Edit" })).toBeNull();
  expect(screen.queryByRole("menuitem", { name: "Delete" })).toBeNull();
  await user.keyboard("{Escape}");
  await user.click(await screen.findByRole("button", { name: "Actions for Night shift" }));
  expect(await screen.findByRole("menuitem", { name: "Edit" })).toBeVisible();
  expect(screen.getByRole("menuitem", { name: "Delete" })).toBeVisible();
});

it("duplicates a built-in into an editable copy with a free numbered name, and saves it once", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  render(<ProfilesSettings />);
  const user = userEvent.setup();
  await menuOf(user, "Fast", "Duplicate");
  const name = await screen.findByLabelText("Name");
  expect(name).toHaveValue("Fast (2)");
  const save = screen.getByRole("button", { name: "Save" });
  await user.dblClick(save);
  await waitFor(() => expect(mock.writes).toEqual(["create:Fast (2)"]));
  expect(await screen.findByText("Saved Fast (2).")).toBeVisible();
});

it("keeps what was typed and turns Save off when the name is already used, ignoring case", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  render(<ProfilesSettings />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add profile" }));
  await user.type(await screen.findByLabelText("Name"), "fAST");
  expect(await screen.findByText(/A profile called Fast already exists/)).toBeVisible();
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  expect(screen.getByLabelText("Name")).toHaveValue("fAST");
  expect(mock.writes).toEqual([]);
});

it("shows the service's name_taken under the name and keeps the text, when the list was stale", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  render(<ProfilesSettings />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add profile" }));
  await user.type(await screen.findByLabelText("Name"), "Brand new");
  mock.failNext("name_taken");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText(/A profile with that name already exists/)).toBeVisible();
  expect(screen.getByLabelText("Name")).toHaveValue("Brand new");
});

it("offers no read-only permission mode and no effort or model field", async () => {
  setProfilesApi(createProfilesMock());
  render(<ProfilesSettings />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add profile" }));
  const modes = within(await screen.findByRole("radiogroup", { name: "Permission mode" })).getAllByRole("radio").map((radio) => radio.textContent);
  expect(modes).toEqual(["Supervised", "Auto edits", "Auto", "Full access"]);
  expect(screen.queryByLabelText(/effort/i)).toBeNull();
  expect(screen.queryByLabelText(/^model$/i)).toBeNull();
});

it("says the profile changed and offers Reload on a revision conflict, keeping the edits", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  render(<ProfilesSettings />);
  const user = userEvent.setup();
  await menuOf(user, "Night shift", "Edit");
  const description = await screen.findByLabelText("Description");
  await user.clear(description);
  await user.type(description, "Edited here");
  mock.touch("01j0000000000000000000a001");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText(/changed since you opened it/)).toBeVisible();
  expect(screen.getByLabelText("Description")).toHaveValue("Edited here");
  expect(screen.getByRole("button", { name: "Reload" })).toBeVisible();
});

it("deletes a saved profile after one confirm, with Cancel focused, and says nothing else changes", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  render(<ProfilesSettings />);
  const user = userEvent.setup();
  await menuOf(user, "Night shift", "Delete");
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText(/keep the choices they started with/)).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  await user.click(within(dialog).getByRole("button", { name: "Delete" }));
  await waitFor(() => expect(mock.writes).toEqual(["remove:01j0000000000000000000a001"]));
  await waitFor(() => expect(screen.queryByText("Night shift")).toBeNull());
});

it("treats a profile that is already gone as done, with the list read again", async () => {
  const mock = createProfilesMock();
  setProfilesApi(mock);
  render(<ProfilesSettings />);
  const user = userEvent.setup();
  await menuOf(user, "Night shift", "Delete");
  mock.failNext("not_found");
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete" }));
  expect(await screen.findByText(/already gone/)).toBeVisible();
});

it("turns Add profile and Duplicate off at the limit, with the reason", async () => {
  const saved = profileFixtures().filter((one) => !one.builtin);
  setProfilesApi(createProfilesMock({ limit: saved.length }));
  render(<ProfilesSettings />);
  const add = await screen.findByRole("button", { name: "Add profile" });
  expect(add).toBeDisabled();
  expect(add).toHaveAttribute("title", expect.stringMatching(/Delete one to add another/));
});

it("shows service text escaped and cut: a name with markup is text, never an element", async () => {
  const hostile = { ...profileFixtures()[3]!, id: "01jevil", name: `<img src=x onerror="alert(1)">${"n".repeat(100)}`, description: "<script>alert(1)</script>" };
  setProfilesApi(createProfilesMock({ profiles: [hostile] }));
  const { container } = render(<ProfilesSettings />);
  await screen.findByRole("list", { name: "Profiles" });
  expect(container.querySelector("img, script")).toBeNull();
  expect(container.textContent).toContain("<img src=x");
  expect(container.textContent).not.toContain("n".repeat(80));
});

