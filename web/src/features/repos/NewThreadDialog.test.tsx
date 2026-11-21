import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { createReposMock, repoFixtures } from "../../../mock/repos";
import { NewThreadDialog } from "./NewThreadDialog";
import { setReposApi } from "./api";

afterEach(() => { setReposApi(null); window.location.hash = ""; });

const open = async (mock = createReposMock()) => {
  setReposApi(mock);
  render(<NewThreadDialog projectId="p1" projectName="orbyn" onClose={() => {}} />);
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

it("starts only in the ticked repositories, each from its chosen branch, with one write", async () => {
  const { mock, dialog } = await open();
  const user = userEvent.setup();
  await user.click(within(dialog).getByRole("checkbox", { name: /orbyn-planner/ }));
  expect(within(dialog).getByRole("button", { name: "Start in 1 repository" })).toBeVisible();
  await user.selectOptions(within(dialog).getByRole("combobox", { name: "Branch in orbyn-engineering" }), "develop");
  expect(mock.writes).toEqual([]);
  await user.dblClick(within(dialog).getByRole("button", { name: "Start in 1 repository" }));
  await waitFor(() => expect(mock.writes).toEqual(["start:orbyn-engineering@develop"]));
  expect(window.location.hash).toMatch(/^#\/threads\//);
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
