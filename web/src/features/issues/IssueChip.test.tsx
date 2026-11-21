import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createIssuesMock, issueFixtures } from "../../../mock/issues";
import type { Thread } from "../../api/types";
import { IssueChip } from "./IssueChip";
import { IssueMark } from "./IssueMark";
import { setIssuesApi } from "./api";
import { resetIssueLinks } from "./links";
import type { IssueRef } from "./contract";

const thread = { id: "thread-1", project_id: "p1" } as Thread;
const refOf = (key: string): IssueRef => {
  const one = issueFixtures().find((issue) => issue.key === key)!;
  return { tracker: one.tracker, scope: one.scope, key: one.key, display_key: one.display_key, title: one.title, state: one.state, labels: one.labels };
};

beforeEach(() => { resetIssueLinks(); window.location.hash = ""; });
afterEach(() => { setIssuesApi(null); vi.restoreAllMocks(); });

it("draws no chip while nothing is linked", async () => {
  setIssuesApi(createIssuesMock({ links: {} }));
  render(<IssueChip thread={thread} />);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByRole("button", { name: "Issues" })).toBeNull();
});

it("says the key, and the shape says the state, then lists the issue with its state, tracker and labels", async () => {
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  render(<IssueChip thread={thread} />);
  const chip = await screen.findByRole("button", { name: "Issues" });
  expect(chip).toHaveTextContent("#412");
  expect(chip.querySelector("[data-state=open]")).not.toBeNull();
  await user.click(chip);
  const pop = await screen.findByRole("dialog", { name: "Issues" });
  expect(within(pop).getByText("1 linked issue")).toBeVisible();
  const row = within(pop).getByRole("button", { name: "#412 Login sends people back to /home" });
  expect(within(row).getByText("Open")).toBeVisible();
  expect(within(row).getByText("GitHub")).toBeVisible();
  expect(within(row).getByText("bug")).toBeVisible();
  await user.click(row);
  expect(window.location.hash).toBe("#/issues/github/412");
});

it("reads Issues 3 with several, in the order they were linked, across trackers, closed ones kept", async () => {
  setIssuesApi(createIssuesMock({ links: { "thread-1": [refOf("412"), refOf("ORB-87"), refOf("391")] } }));
  const user = userEvent.setup();
  render(<IssueChip thread={thread} />);
  const chip = await screen.findByRole("button", { name: "Issues" });
  expect(chip).toHaveTextContent("Issues 3");
  await user.click(chip);
  const pop = await screen.findByRole("dialog", { name: "Issues" });
  expect(within(pop).getByText("3 linked issues")).toBeVisible();
  const names = within(pop).getAllByRole("button", { name: /^(#|ORB)/ }).map((row) => row.getAttribute("aria-label"));
  expect(names).toEqual(["#412 Login sends people back to /home", "ORB-87 Show Issues next to Pull requests", "#391 Settings rows jump when a scope changes"]);
  expect(within(pop).getByText("Closed")).toBeVisible();
  expect(within(pop).getByText("Linear")).toBeVisible();
});

it("unlinks from this thread with one write and leaves the issue alone", async () => {
  const mock = createIssuesMock({ links: { "thread-1": [refOf("412"), refOf("ORB-87")] } });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueChip thread={thread} />);
  await user.click(await screen.findByRole("button", { name: "Issues" }));
  await user.click(await screen.findByRole("button", { name: "Unlink #412" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Unlink #412" })).toBeNull());
  expect(mock.writes).toEqual(["unlink:thread-1:412"]);
});

it("links an issue found by number, words or link, and links nothing until Link is pressed", async () => {
  const mock = createIssuesMock({ links: { "thread-1": [refOf("412")] } });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueChip thread={thread} />);
  await user.click(await screen.findByRole("button", { name: "Issues" }));
  await user.click(await screen.findByRole("button", { name: "Link an issue…" }));
  const input = await screen.findByRole("combobox", { name: "Issue number or link" });
  await user.type(input, "picker");
  const option = await screen.findByRole("option", { name: /#409/ });
  expect(mock.writes).toEqual([]);
  await user.click(option);
  await user.click(screen.getByRole("button", { name: "Link" }));
  await waitFor(() => expect(mock.writes).toEqual(["link:thread-1:409"]));
  expect(await screen.findByRole("button", { name: "Issues" })).toHaveTextContent("Issues 2");
});

it("says a number the tracker does not have, names the repository and tracker, and links nothing", async () => {
  const mock = createIssuesMock({ links: { "thread-1": [refOf("412")] } });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueChip thread={thread} />);
  await user.click(await screen.findByRole("button", { name: "Issues" }));
  await user.click(await screen.findByRole("button", { name: "Link an issue…" }));
  await user.type(await screen.findByRole("combobox", { name: "Issue number or link" }), "9999");
  await user.click(screen.getByRole("button", { name: "Link" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("No issue #9999 in orbyn-dev/orbyn-engineering on GitHub.");
  expect(mock.writes).toEqual([]);
});

it("marks a row with the open ring or the closed tick, names the key in the accessible name, and draws nothing for none", async () => {
  setIssuesApi(createIssuesMock({ links: { a: [refOf("412")], b: [refOf("391")], c: [refOf("412"), refOf("ORB-87"), refOf("409")] } }));
  render(<><IssueMark threadId="a" /><IssueMark threadId="b" /><IssueMark threadId="c" /><IssueMark threadId="none" /></>);
  expect(await screen.findByRole("img", { name: "Issue #412, open" })).toBeVisible();
  expect(screen.getByRole("img", { name: "Issue #391, closed" })).toBeVisible();
  const many = screen.getByRole("img", { name: "3 issues: #412, ORB-87, #409" });
  expect(many).toHaveTextContent("3");
  expect(screen.getAllByRole("img")).toHaveLength(3);
});
