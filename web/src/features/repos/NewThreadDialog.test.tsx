import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { createReposMock, repoFixtures } from "../../../mock/repos";
import { ApiFailure } from "../../api/client";
import { NewThreadDialog } from "./NewThreadDialog";
import { setReposApi } from "./api";

afterEach(() => { setReposApi(null); window.location.hash = ""; });

const open = async (mock = createReposMock(), note: string | null = null) => {
  setReposApi(mock);
  render(<NewThreadDialog projectId="p1" projectName="orbyn" start={{ engine: "claude", model: "opus", permission_mode: "auto_edits" }}
    engineControl={<button type="button">Claude Code · Opus</button>} permissionControl={<button type="button">Auto-accept edits</button>} note={note} onClose={() => {}} />);
  const dialog = await screen.findByRole("dialog", { name: /New thread in orbyn/ });
  await within(dialog).findAllByRole("checkbox");
  return { mock, dialog };
};

it("ticks the trusted repositories, dims an untrusted one, and counts the button", async () => {
  const { dialog } = await open();
  expect(within(dialog).getByRole("checkbox", { name: /orbyn-engineering/ })).toBeChecked();
  expect(within(dialog).getByRole("checkbox", { name: /orbyn-planner/ })).toBeChecked();
  const engine = within(dialog).getByRole("checkbox", { name: /orbyn-engine\b/ });
  expect(engine).not.toBeChecked();
  expect(engine).toBeDisabled();
  expect(within(dialog).getByText("Not trusted")).toBeVisible();
  expect(within(dialog).getByText(/orbyn-engine is not trusted, so it cannot be included. Trust it in Settings › Projects./)).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Start in 2 repositories" })).toBeEnabled();
});

const three = () => ["a", "b", "c"].map((id) => ({ ...repoFixtures()[0]!, id, name: id, primary: id === "a" }));

it("starts only in the ticked repositories, each from its chosen branch, with one write", async () => {
  const { mock, dialog } = await open(createReposMock({ repositories: three() }));
  const user = userEvent.setup();
  await user.click(within(dialog).getByRole("checkbox", { name: /^c/ }));
  expect(within(dialog).getByRole("button", { name: "Start in 2 repositories" })).toBeEnabled();
  await user.selectOptions(within(dialog).getByRole("combobox", { name: "Branch in a" }), "develop");
  expect(mock.writes).toEqual([]);
  await user.dblClick(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  await waitFor(() => expect(mock.writes).toEqual(["start:a@develop,b@main"]));
  expect(window.location.hash).toMatch(/^#\/threads\//);
});

it("does not start in one repository: the service wants two or more, so the button is off and says why", async () => {
  const { mock, dialog } = await open();
  await userEvent.click(within(dialog).getByRole("checkbox", { name: /orbyn-planner/ }));
  expect(within(dialog).getByRole("button", { name: "Start in 1 repository" })).toBeDisabled();
  expect(within(dialog).getByText(/Choose two or more repositories/)).toBeVisible();
  await userEvent.click(within(dialog).getByRole("button", { name: "Start in 1 repository" }));
  expect(mock.writes).toEqual([]);
});

it("draws the Engine and Permission rows the host hands in, and starts with that engine and permission, in one write", async () => {
  const { mock, dialog } = await open(createReposMock(), "Cursor does not ask before acting, so Supervised is not available here.");
  expect(within(dialog).getByText("Engine")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Claude Code · Opus" })).toBeVisible();
  expect(within(dialog).getByText("Permission")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Auto-accept edits" })).toBeVisible();
  expect(within(dialog).getByText(/Supervised is not available here/)).toBeVisible();
  await userEvent.dblClick(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  await waitFor(() => expect(mock.writes).toHaveLength(1));
  expect(mock.starts).toEqual([{ engine: "claude", model: "opus", permission_mode: "auto_edits" }]);
});

it("cannot start with nothing ticked", async () => {
  const { dialog } = await open();
  const user = userEvent.setup();
  await user.click(within(dialog).getByRole("checkbox", { name: /orbyn-engineering/ }));
  await user.click(within(dialog).getByRole("checkbox", { name: /orbyn-planner/ }));
  expect(within(dialog).getByRole("button", { name: "Start in 0 repositories" })).toBeDisabled();
});

it("stops ticking at the service's limit", async () => {
  const many = ["a", "b", "c"].map((id) => ({ ...repoFixtures()[0]!, id, name: id }));
  const { dialog } = await open(createReposMock({ repositories: many, maxPerThread: 2 }));
  expect(within(dialog).getByRole("checkbox", { name: /^b/ })).toBeChecked();
  expect(within(dialog).getByRole("checkbox", { name: /^c/ })).toBeDisabled();
  expect(within(dialog).getByText("A thread works in up to 2 repositories.")).toBeVisible();
});

it("keeps the dialog open when the service refuses", async () => {
  const mock = createReposMock();
  const { dialog } = await open(mock);
  mock.failNext("start");
  await userEvent.click(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("The service could not do that.");
});

it("shows why a permission mode the engine does not offer was refused, and keeps the dialog open to try again", async () => {
  const mock = createReposMock({ permissionModes: { claude: ["supervised"] } });
  const { dialog } = await open(mock);
  await userEvent.click(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  const alert = await within(dialog).findByRole("alert");
  expect(alert).toHaveTextContent("This engine does not offer that permission.");
  expect(alert).toHaveTextContent("claude does not offer the auto_edits permission mode.");
  expect(screen.getByRole("dialog", { name: /New thread in orbyn/ })).toBeVisible();
  expect(window.location.hash).not.toMatch(/^#\/threads\//);
  expect(within(dialog).getByRole("button", { name: "Start in 2 repositories" })).toBeEnabled();
});

it("never offers an untrusted repository: it cannot be ticked and is not sent", async () => {
  const { mock, dialog } = await open();
  await userEvent.click(within(dialog).getByRole("checkbox", { name: /orbyn-engine\b/ }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  await waitFor(() => expect(mock.writes).toEqual(["start:orbyn-engineering@main,orbyn-planner@feat/calendar"]));
});

it("shows the branch name only when two or more repositories are ticked", async () => {
  const { dialog } = await open();
  const user = userEvent.setup();
  expect(within(dialog).getByRole("textbox", { name: "Branch name" })).toBeVisible();
  await user.click(within(dialog).getByRole("checkbox", { name: /orbyn-planner/ }));
  expect(within(dialog).queryByRole("textbox", { name: "Branch name" })).toBeNull();
});

it("sends one validated branch name for every repository, and none when it is left empty", async () => {
  const first = await open();
  await userEvent.type(within(first.dialog).getByRole("textbox", { name: "Branch name" }), " feat/login-fix ");
  await userEvent.click(within(first.dialog).getByRole("button", { name: "Start in 2 repositories" }));
  await waitFor(() => expect(first.mock.branchNames).toEqual(["feat/login-fix"]));
});

it("starts without a branch name when the field is empty", async () => {
  const { mock, dialog } = await open();
  await userEvent.click(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  await waitFor(() => expect(mock.branchNames).toEqual([undefined]));
});

it("refuses a branch name Git would refuse, in words, and does not start", async () => {
  const { mock, dialog } = await open();
  await userEvent.type(within(dialog).getByRole("textbox", { name: "Branch name" }), "bad name");
  expect(within(dialog).getByText(/cannot hold spaces/)).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Start in 2 repositories" })).toBeDisabled();
  expect(mock.writes).toEqual([]);
});

it.each([
  ["repo_limit", 409, "A thread works in at most 8 repositories."],
  ["not_git", 409, "orbyn-planner is not a Git repository."],
  ["invalid", 400, "orbyn-planner already has a branch named feat/login-fix."],
  ["invalid", 400, "main is not a branch of orbyn-planner."],
  ["not_found", 404, "No such repository in this project."],
] as const)("shows the service's own message for %s (%i) and keeps the dialog open", async (code, status, message) => {
  const mock = createReposMock();
  const { dialog } = await open(mock);
  mock.rejectNext("start", new ApiFailure(code, message, status));
  await userEvent.click(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent(message);
  expect(within(dialog).getByRole("button", { name: "Start in 2 repositories" })).toBeEnabled();
  expect(window.location.hash).not.toMatch(/^#\/threads\//);
  expect(within(dialog).queryByRole("button", { name: /^Trust/ })).toBeNull();
});

it("offers the trust dialog for a repository the service says is not trusted, and starts nothing on its own", async () => {
  const mock = createReposMock();
  const { dialog } = await open(mock);
  mock.distrust("orbyn-planner");
  const user = userEvent.setup();
  await user.click(within(dialog).getByRole("button", { name: "Start in 2 repositories" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Trust orbyn-planner before starting a thread in it.");
  await user.click(await within(dialog).findByRole("button", { name: "Trust orbyn-planner…" }));
  const trust = await screen.findByRole("dialog", { name: /Trust orbyn-planner/ });
  expect(mock.writes).toEqual(["start:orbyn-engineering@main,orbyn-planner@feat/calendar"]);
  await user.click(within(trust).getByRole("button", { name: /^Trust/ }));
  await waitFor(() => expect(mock.writes).toEqual(["start:orbyn-engineering@main,orbyn-planner@feat/calendar", "trust:orbyn-planner"]));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: /Trust orbyn-planner/ })).toBeNull());
  // Trusting is not starting: the person starts again.
  expect(window.location.hash).not.toMatch(/^#\/threads\//);
  expect(within(dialog).queryByRole("button", { name: "Trust orbyn-planner…" })).toBeNull();
});
