import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { createReposMock, repoFixtures } from "../../../mock/repos";
import { RepositoriesSection } from "./RepositoriesSection";
import { setReposApi } from "./api";

afterEach(() => setReposApi(null));

const open = async (mock = createReposMock()) => {
  setReposApi(mock);
  render(<RepositoriesSection projectId="p1" projectName="orbyn" />);
  const user = userEvent.setup();
  const row = (await screen.findByText("orbyn-planner")).closest("li") as HTMLElement;
  await user.click(within(row).getByRole("button", { name: "Actions for orbyn-planner" }));
  await user.click(await screen.findByRole("menuitem", { name: "Publish to GitHub…" }));
  return { mock, user, dialog: await screen.findByRole("dialog", { name: "Publish repository" }) };
};
const toStepTwo = async (user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement) => {
  await user.click(within(dialog).getByRole("radio", { name: /GitHub/ }));
  await user.click(within(dialog).getByRole("button", { name: "Next" }));
};

it("step 1 offers the providers: only a ready one can be picked, Next stays off until then and nothing is written", async () => {
  const { mock, user, dialog } = await open();
  expect(within(dialog).getByText("Step 1 of 3 · Provider")).toBeVisible();
  expect(within(dialog).getByText("Uses the gh sign-in on this computer")).toBeVisible();
  expect(within(dialog).getAllByText("Setup Required")).toHaveLength(3);
  for (const name of [/GitLab/, /Azure DevOps/, /Forgejo/]) expect(within(dialog).getByRole("radio", { name })).toBeDisabled();
  expect(within(dialog).getByRole("button", { name: "Next" })).toBeDisabled();
  await user.click(within(dialog).getByRole("radio", { name: /GitHub/ }));
  expect(within(dialog).getByRole("button", { name: "Next" })).toBeEnabled();
  await user.click(within(dialog).getByRole("button", { name: "Next" }));
  expect(within(dialog).getByText("Step 2 of 3 · Repository")).toBeVisible();
  expect(mock.writes).toEqual([]);
});

it("step 2 names the folder and the branch pushed, Back returns to the provider, and Publish writes once with the choices", async () => {
  const { mock, user, dialog } = await open();
  await toStepTwo(user, dialog);
  expect(within(dialog).getByRole("textbox", { name: "Repository" })).toHaveValue("orbyn-dev/orbyn-planner");
  expect(within(dialog).getByText("~/Documents/Github/orbyn-planner")).toBeVisible();
  expect(within(dialog).getByText("feat/calendar")).toBeVisible();
  expect(within(dialog).getByRole("radio", { name: /Private/ })).toBeChecked();
  expect(within(dialog).getByText("Only invited people")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Back" }));
  expect(within(dialog).getByText("Step 1 of 3 · Provider")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Next" }));
  const field = within(dialog).getByRole("textbox", { name: "Repository" });
  await user.clear(field);
  await user.type(field, "not a name");
  expect(within(dialog).getByRole("button", { name: "Publish" })).toBeDisabled();
  await user.clear(field);
  await user.type(field, "orbyn-dev/planner");
  await user.click(within(dialog).getByRole("radio", { name: /Public/ }));
  await user.click(within(dialog).getByText("Advanced"));
  expect(within(dialog).getByRole("textbox", { name: "Remote" })).toHaveValue("origin");
  await user.click(within(dialog).getByRole("radio", { name: "HTTPS" }));
  await user.dblClick(within(dialog).getByRole("button", { name: "Publish" }));
  await waitFor(() => expect(mock.writes).toEqual(["publish:orbyn-planner:orbyn-dev/planner:public:origin:https"]));
});

it("step 3 says the repository was published and links to it; Done closes", async () => {
  const { user, dialog } = await open();
  await toStepTwo(user, dialog);
  await user.click(within(dialog).getByRole("button", { name: "Publish" }));
  expect(await within(dialog).findByText("Repository published")).toBeVisible();
  expect(within(dialog).getByText("Step 3 of 3 · Summary")).toBeVisible();
  expect(within(dialog).getByText("feat/calendar is now live on GitHub.")).toBeVisible();
  const link = within(dialog).getByRole("link", { name: "Open on GitHub" });
  expect(link).toHaveAttribute("href", "https://github.com/orbyn-dev/orbyn-planner");
  expect(link).toHaveAttribute("rel", expect.stringContaining("noopener"));
  await user.click(within(dialog).getByRole("button", { name: "Done" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

it("says Repository created, with the remote line, when nothing was pushed", async () => {
  const mock = createReposMock();
  const publish = mock.publish;
  mock.publish = async (...args) => ({ ...(await publish(...args)), pushed: false });
  const { user, dialog } = await open(mock);
  await toStepTwo(user, dialog);
  await user.click(within(dialog).getByRole("button", { name: "Publish" }));
  expect(await within(dialog).findByText("Repository created")).toBeVisible();
  expect(within(dialog).getByText("Remote “origin” is set up. Make a commit and push it to share your code.")).toBeVisible();
});

it("a failure says Publish failed with the host's message on step 2, and Publish can be pressed again", async () => {
  const mock = createReposMock();
  const { user, dialog } = await open(mock);
  await toStepTwo(user, dialog);
  mock.failNext("publish");
  await user.click(within(dialog).getByRole("button", { name: "Publish" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Publish failed. The service could not do that.");
  expect(within(dialog).getByText("Step 2 of 3 · Repository")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Publish" }));
  expect(await within(dialog).findByText("Repository published")).toBeVisible();
  expect(mock.writes).toHaveLength(2);
});

it("shows a hostile repository name as text", async () => {
  const rows = repoFixtures().map((row) => (row.id === "orbyn-planner" ? { ...row, name: "<img src=x onerror=alert(1)>" } : row));
  setReposApi(createReposMock({ repositories: rows }));
  render(<RepositoriesSection projectId="p1" projectName="orbyn" />);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Actions for <img src=x onerror=alert(1)>" }));
  await user.click(await screen.findByRole("menuitem", { name: "Publish to GitHub…" }));
  await screen.findByRole("dialog", { name: "Publish repository" });
  expect(document.querySelector("img")).toBeNull();
});
