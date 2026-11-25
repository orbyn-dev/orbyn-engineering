import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import type { TrackerRow } from "./contract";
import { IssuesPage } from "./IssuesPage";
import { setIssuesApi } from "./api";
import { resetIssueLinks } from "./links";

// A browser tab has no desktop shell, so `canSendPersonKey` is false there and the Person-class create is not offered.
const shell = vi.hoisted(() => ({ canSend: true }));
vi.mock("../../api/personKey", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../api/personKey")>();
  return { ...real, canSendPersonKey: () => shell.canSend };
});

const REPO = "orbyn-dev/orbyn-engineering";
const GITLAB = "group/app";
const TEMPLATES = [{ id: "bug", name: "Bug report", description: "Something is broken", body: "Steps to reproduce:", labels: [{ name: "bug" }] }];
const trackers = (): TrackerRow[] => [
  { tracker: "github", connected: true, scope: REPO },
  { tracker: "gitlab", connected: true, scope: GITLAB },
  { tracker: "linear", connected: false, scope: null },
];

beforeEach(() => { resetIssueLinks(); shell.canSend = true; });
afterEach(() => { setIssuesApi(null); window.location.hash = ""; vi.restoreAllMocks(); });

const list = () => { window.location.hash = "#/issues"; render(<IssuesPage />); };
const open = async (user: ReturnType<typeof userEvent.setup>) => {
  await user.click(await screen.findByRole("button", { name: "New issue" }));
  return screen.findByRole("dialog", { name: "New issue" });
};

it("opens one dialog from the page bar with Tracker, Title, Description, Labels and Assignees, and no Template row on GitHub", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  list();
  const dialog = await open(user);
  expect(within(dialog).getByRole("button", { name: `Tracker: GitHub · ${REPO}` })).toBeVisible();
  expect(within(dialog).getByRole("textbox", { name: "Title" })).toBeVisible();
  expect(within(dialog).getByRole("textbox", { name: "Description" })).toBeVisible();
  expect(await within(dialog).findByRole("group", { name: "Labels" })).toBeVisible();
  expect(await within(dialog).findByRole("group", { name: "Assignees" })).toBeVisible();
  expect(within(dialog).queryByRole("button", { name: /^Template:/ })).toBeNull();
  // The title rule: Create issue is off until the title has a character.
  expect(within(dialog).getByRole("button", { name: "Create issue" })).toBeDisabled();
  await user.type(within(dialog).getByRole("textbox", { name: "Title" }), "x");
  expect(within(dialog).getByRole("button", { name: "Create issue" })).toBeEnabled();
  // Opening and typing write nothing.
  expect(mock.writes).toEqual([]);
});

it("files one issue with one create, the label names and the assignees' candidate ids, then opens the issue", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  list();
  const dialog = await open(user);
  await user.type(within(dialog).getByRole("textbox", { name: "Title" }), "New dialog test");
  await user.type(within(dialog).getByRole("textbox", { name: "Description" }), "A body.");
  // The rows wait on the candidates read that tells the dialog this tracker has a label and people list.
  await within(dialog).findByRole("group", { name: "Labels" });
  await user.click(within(dialog).getByRole("button", { name: "Choose labels" }));
  const labels = await within(dialog).findByRole("group", { name: "Choose labels" });
  await user.click(await within(labels).findByRole("checkbox", { name: "bug" }));
  await user.click(within(labels).getByRole("button", { name: "Apply" }));
  await user.click(within(dialog).getByRole("button", { name: "Choose assignees" }));
  const people = await within(dialog).findByRole("group", { name: "Choose assignees" });
  await user.click(await within(people).findByRole("checkbox", { name: "riley · Riley Chen" }));
  await user.click(within(people).getByRole("button", { name: "Apply" }));
  expect(within(dialog).getByText("bug")).toBeVisible();
  expect(within(dialog).getByText("riley")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Create issue" }));
  await waitFor(() => expect(window.location.hash).toMatch(/#\/issues\/github\/orbyn-dev%2Forbyn-engineering\/\d+$/));
  expect(mock.writes.filter((write) => write.startsWith("create:"))).toHaveLength(1);
  expect(mock.creates).toHaveLength(1);
  expect(mock.creates[0]).toMatchObject({ tracker: "github", scope: REPO, title: "New dialog test", body: "A body.", labels: ["bug"], assignees: ["u-1"] });
  expect(await screen.findByRole("heading", { name: "New dialog test" })).toBeVisible();
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("one click is one create: a second click while the write is in flight sends nothing, and every field is off", async () => {
  const mock = createIssuesMock({ delayMs: 150 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  list();
  const dialog = await open(user);
  await user.type(within(dialog).getByRole("textbox", { name: "Title" }), "One call");
  await user.dblClick(within(dialog).getByRole("button", { name: "Create issue" }));
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
  expect(within(dialog).getByRole("textbox", { name: "Title" })).toBeDisabled();
  await waitFor(() => expect(mock.creates).toHaveLength(1));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.creates).toHaveLength(1);
});

it("a refused create keeps the draft, says nothing changed, and sends the same draft once more on the next click", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  mock.failNext("create");
  const user = userEvent.setup();
  list();
  const dialog = await open(user);
  await user.type(within(dialog).getByRole("textbox", { name: "Title" }), "Try again");
  await user.click(within(dialog).getByRole("button", { name: "Create issue" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("GitHub did not answer. Nothing changed.");
  expect(within(dialog).getByRole("textbox", { name: "Title" })).toHaveValue("Try again");
  expect(within(dialog).getByRole("button", { name: "Create issue" })).toBeEnabled();
  await user.click(within(dialog).getByRole("button", { name: "Create issue" }));
  await waitFor(() => expect(mock.creates).toHaveLength(2));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.creates[1]).toMatchObject({ title: "Try again" });
});

it("a create whose outcome is unknown offers Check the list, which re-reads the list and never re-sends the draft", async () => {
  const mock = createIssuesMock({ createUnknown: true });
  setIssuesApi(mock);
  const user = userEvent.setup();
  list();
  const dialog = await open(user);
  await user.type(within(dialog).getByRole("textbox", { name: "Title" }), "Maybe created");
  const before = mock.listed.length;
  await user.click(within(dialog).getByRole("button", { name: "Create issue" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("GitHub did not answer, so the issue was possibly created. Re-read the list before writing it again.");
  expect(within(dialog).queryByRole("button", { name: "Create issue" })).toBeNull();
  expect(within(dialog).getByRole("button", { name: "Check the list" })).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeVisible();
  expect(within(dialog).getByRole("textbox", { name: "Title" })).toBeDisabled();
  await user.click(within(dialog).getByRole("button", { name: "Check the list" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(mock.listed.length).toBeGreaterThan(before));
  expect(mock.creates).toHaveLength(1);
  expect(window.location.hash).toBe("#/issues");
});

it("a first create that meets no such route puts New issue back to Planned and shows no success", async () => {
  const mock = createIssuesMock({ noCreate: true });
  setIssuesApi(mock);
  const user = userEvent.setup();
  list();
  const dialog = await open(user);
  await user.type(within(dialog).getByRole("textbox", { name: "Title" }), "No route");
  await user.click(within(dialog).getByRole("button", { name: "Create issue" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  const button = screen.getByRole("button", { name: "New issue" });
  expect(button).toBeDisabled();
  expect(button.parentElement).toHaveTextContent("Planned");
  expect(window.location.hash).toBe("#/issues");
  expect(mock.creates).toHaveLength(1);
});

it("draws the Template row only where the service offers templates, fills a missing description, and never merges the template's labels", async () => {
  const mock = createIssuesMock({ trackers: trackers(), templates: { gitlab: TEMPLATES }, unsupportedCandidates: ["gitlab"] });
  setIssuesApi(mock);
  const user = userEvent.setup();
  list();
  const dialog = await open(user);
  // GitHub: the two pickers and no Template row.
  expect(within(dialog).queryByRole("button", { name: /^Template:/ })).toBeNull();
  expect(await within(dialog).findByRole("group", { name: "Labels" })).toBeVisible();
  // GitLab: a Template row, and no label or people rows (its candidates read is 422).
  await user.click(within(dialog).getByRole("button", { name: `Tracker: GitHub · ${REPO}` }));
  const trackerMenu = await screen.findByRole("menu", { name: "Tracker" });
  await user.click(within(trackerMenu).getByRole("menuitem", { name: `GitLab · ${GITLAB}` }));
  const templateButton = await within(dialog).findByRole("button", { name: "Template: Blank" });
  expect(within(dialog).queryByRole("group", { name: "Labels" })).toBeNull();
  expect(within(dialog).queryByRole("group", { name: "Assignees" })).toBeNull();
  await user.type(within(dialog).getByRole("textbox", { name: "Title" }), "Template issue");
  await user.click(templateButton);
  const templateMenu = await screen.findByRole("menu", { name: "Template" });
  await user.click(within(templateMenu).getByRole("menuitem", { name: "Bug report" }));
  expect(within(dialog).getByRole("textbox", { name: "Description" })).toHaveValue("Steps to reproduce:");
  await user.click(within(dialog).getByRole("button", { name: "Create issue" }));
  await waitFor(() => expect(mock.creates).toHaveLength(1));
  expect(mock.creates[0]).toMatchObject({ tracker: "gitlab", scope: GITLAB, title: "Template issue", body: "Steps to reproduce:", template: "bug" });
  // The template's own labels are never merged into the create's.
  expect(mock.creates[0]!.labels).toBeUndefined();
});

it("a template fills only a missing description: text the person typed is never replaced", async () => {
  const mock = createIssuesMock({ trackers: trackers(), templates: { gitlab: TEMPLATES }, unsupportedCandidates: ["gitlab"] });
  setIssuesApi(mock);
  const user = userEvent.setup();
  list();
  const dialog = await open(user);
  await user.click(within(dialog).getByRole("button", { name: `Tracker: GitHub · ${REPO}` }));
  const trackerMenu = await screen.findByRole("menu", { name: "Tracker" });
  await user.click(within(trackerMenu).getByRole("menuitem", { name: `GitLab · ${GITLAB}` }));
  const templateButton = await within(dialog).findByRole("button", { name: "Template: Blank" });
  await user.type(within(dialog).getByRole("textbox", { name: "Description" }), "My own words");
  await user.click(templateButton);
  const templateMenu = await screen.findByRole("menu", { name: "Template" });
  await user.click(within(templateMenu).getByRole("menuitem", { name: "Bug report" }));
  expect(within(dialog).getByRole("textbox", { name: "Description" })).toHaveValue("My own words");
});

it("does not offer the Person-class New issue control in a browser tab with no desktop shell", async () => {
  shell.canSend = false;
  setIssuesApi(createIssuesMock());
  window.location.hash = "#/issues";
  render(<IssuesPage />);
  expect(await screen.findByRole("button", { name: "Refresh" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "New issue" })).toBeNull();
});
