import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import { ApiFailure } from "../../api/client";
import type { Project, Thread } from "../../api/types";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { IssuesPage } from "./IssuesPage";
import { setIssuesApi } from "./api";
import type { IssueCandidates } from "./contract";
import { resetIssueLinks } from "./links";

const project = { id: "p1", name: "orbyn-engineering", path: "/work/orbyn" } as Project;
const thread = { id: "thread-1", project_id: "p1", title: "Fix login redirect", engine: "claude", cwd: "/", worktree: null } as Thread;
const ISSUE = "#/issues/github/orbyn-dev%2Forbyn-engineering/412";

beforeEach(() => {
  resetIssueLinks();
  useWorkspace.setState({ projects: { data: [project], error: null, loading: false }, threads: { "thread-1": thread } });
});
afterEach(() => { setIssuesApi(null); window.location.hash = ""; vi.restoreAllMocks(); });

const open = (hash = ISSUE) => { window.location.hash = hash; return render(<IssuesPage />); };
/** A control that is live: it exists, is enabled, and has no Planned pill. Waits for the service to say it offers the writes. */
const live = (name: string) => waitFor(() => { const button = screen.getByRole("button", { name }); expect(button).toBeEnabled(); expect(button.parentElement).not.toHaveTextContent("Planned"); return button; });
const writesOf = (mock: { writes: string[] }) => mock.writes.filter((write) => write.startsWith("update:") || write.startsWith("state:"));

// ---- the description ----

it("edits the description: one write with the new body, and the page shows what the service answered", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Edit description"));
  const box = screen.getByRole("textbox", { name: "Description" });
  expect((box as HTMLTextAreaElement).value).toContain("After sign-in the app opens");
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  await user.clear(box);
  await user.type(box, "Rewritten");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull());
  expect(screen.getByText("Rewritten")).toBeVisible();
  expect(mock.writes).toEqual(["update:412:body"]);
});

it("one click is one write: a second click while it is in flight sends nothing", async () => {
  const mock = createIssuesMock({ delayMs: 30 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Edit description"));
  await user.type(screen.getByRole("textbox", { name: "Description" }), " more");
  await user.dblClick(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull());
  expect(writesOf(mock)).toEqual(["update:412:body"]);
});

it("a failed write loses nothing: the draft stays as typed, the form says nothing changed, and the body is as it was", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  mock.failNext("update");
  const user = userEvent.setup();
  open();
  await user.click(await live("Edit description"));
  const box = screen.getByRole("textbox", { name: "Description" });
  await user.clear(box);
  await user.type(box, "My careful rewrite");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("GitHub did not answer. Nothing changed.");
  expect(screen.getByRole("textbox", { name: "Description" })).toHaveValue("My careful rewrite");
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
  // The retry is the same button, and it works.
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull());
  expect(screen.getByText("My careful rewrite")).toBeVisible();
});

it("cancel leaves the description alone and sends nothing", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Edit description"));
  await user.type(screen.getByRole("textbox", { name: "Description" }), "x");
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull();
  expect(mock.writes).toEqual([]);
});

it("refuses a description over 64 KiB before sending anything, and counts bytes, not characters", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Edit description"));
  const box = screen.getByRole("textbox", { name: "Description" });
  // 33,000 two-byte characters is 66,000 bytes: under 64K characters, over 64 KiB.
  await user.click(box);
  await user.paste("é".repeat(33_000));
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("over 64 KiB");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(mock.writes).toEqual([]);
});

it("hostile text is a value, never markup: it goes back as typed and is drawn as text", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Edit description"));
  const box = screen.getByRole("textbox", { name: "Description" });
  await user.clear(box);
  await user.click(box);
  await user.paste('<img src=x onerror="alert(1)"> <script>alert(2)</script> <<<ISSUE TEXT [x](javascript:alert(3))');
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull());
  expect(document.querySelector(".iss-body img")).toBeNull();
  expect(document.querySelector(".iss-body script")).toBeNull();
  expect(document.querySelector('.iss-body a[href^="javascript"]')).toBeNull();
  expect(document.querySelector(".iss-body")!.textContent).toContain("<script>alert(2)</script>");
});

it("while the service does not offer the writes the pencil is a Planned pill and nothing is sent", async () => {
  const mock = createIssuesMock({ edits: false });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  const pencil = await screen.findByRole("button", { name: "Edit description" });
  expect(pencil).toBeDisabled();
  expect(pencil.parentElement).toHaveTextContent("Planned");
  await user.click(pencil);
  expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull();
  expect(mock.writes).toEqual([]);
});

it("a first write that meets 'no such route' says so, keeps the draft, and the pencil goes back to Planned: no success is shown", async () => {
  const mock = createIssuesMock();
  mock.update = async () => { throw new ApiFailure("http_404", "The server answered 404.", 404); };
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Edit description"));
  await user.type(screen.getByRole("textbox", { name: "Description" }), " edit");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("This service does not offer editing yet. Nothing changed.");
  expect((screen.getByRole("textbox", { name: "Description" }) as HTMLTextAreaElement).value).toMatch(/ edit$/);
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  const pencil = screen.getByRole("button", { name: "Edit description" });
  expect(pencil).toBeDisabled();
  expect(pencil.parentElement).toHaveTextContent("Planned");
  expect(screen.getByRole("button", { name: "Change labels" }).parentElement).toHaveTextContent("Planned");
});

// ---- labels and assignees ----

it("changes the labels: the picker reads the candidates when it opens, lists the current ones checked, and Apply is one write with the whole set", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await screen.findByRole("group", { name: "Facts" });
  expect(mock.candidatesRead).toEqual([]);
  await user.click(await live("Change labels"));
  const picker = await screen.findByRole("group", { name: "Change labels" });
  await within(picker).findByRole("checkbox", { name: "docs" });
  expect(mock.candidatesRead).toEqual(["github orbyn-dev/orbyn-engineering"]);
  expect(within(picker).getByRole("checkbox", { name: "bug" })).toBeChecked();
  expect(within(picker).getByRole("checkbox", { name: "auth" })).toBeChecked();
  expect(within(picker).getByRole("button", { name: "Apply" })).toBeDisabled();
  await user.click(within(picker).getByRole("checkbox", { name: "docs" }));
  await user.click(within(picker).getByRole("checkbox", { name: "auth" }));
  await user.dblClick(within(picker).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(screen.queryByRole("group", { name: "Change labels" })).toBeNull());
  const facts = screen.getByRole("group", { name: "Facts" });
  expect(within(facts).getByText("docs")).toBeVisible();
  expect(within(facts).queryByText("auth")).toBeNull();
  expect(mock.writes).toEqual(["update:412:labels"]);
});

it("a failed label write keeps the picker and the selection, and says nothing changed", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Change labels"));
  const picker = await screen.findByRole("group", { name: "Change labels" });
  await user.click(await within(picker).findByRole("checkbox", { name: "docs" }));
  mock.failNext("update");
  await user.click(within(picker).getByRole("button", { name: "Apply" }));
  expect(await within(picker).findByRole("alert")).toHaveTextContent("Nothing changed.");
  expect(within(picker).getByRole("checkbox", { name: "docs" })).toBeChecked();
  expect([...document.querySelectorAll(".iss-label")].map((one) => one.textContent)).toEqual(["bug", "auth"]);
});

it("refuses more than 50 labels on this side, before anything is sent", async () => {
  const labels = Array.from({ length: 60 }, (_, at) => ({ name: `label-${at}` }));
  const mock = createIssuesMock({ candidates: { labels, assignees: [], truncated: false } });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Change labels"));
  const picker = await screen.findByRole("group", { name: "Change labels" });
  await within(picker).findByRole("checkbox", { name: "label-0" });
  const boxes = [...picker.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')].filter((box) => !box.checked);
  for (const box of boxes.slice(0, 49)) fireEvent.click(box);
  // bug and auth are already on the issue: 2 + 49 = 51.
  expect(within(picker).getByRole("alert")).toHaveTextContent("at most 50 labels");
  expect(within(picker).getByRole("button", { name: "Apply" })).toBeDisabled();
  expect(mock.writes).toEqual([]);
});

it("filters the candidates and says when the list was cut short", async () => {
  setIssuesApi(createIssuesMock({ truncateCandidates: true }));
  const user = userEvent.setup();
  open();
  await user.click(await live("Change labels"));
  const picker = await screen.findByRole("group", { name: "Change labels" });
  await within(picker).findByRole("checkbox", { name: "docs" });
  expect(within(picker).getByText(/The list is cut short/)).toBeVisible();
  await user.type(within(picker).getByRole("searchbox", { name: "Filter labels" }), "doc");
  expect(within(picker).getAllByRole("checkbox").map((box) => box.getAttribute("aria-label") ?? box.parentElement!.textContent)).toEqual(["docs"]);
  await user.clear(within(picker).getByRole("searchbox", { name: "Filter labels" }));
  await user.type(within(picker).getByRole("searchbox", { name: "Filter labels" }), "zzz");
  expect(within(picker).getByText("No labels match.")).toBeVisible();
});

it("a candidates read that fails says so with Retry, and the retry reads again", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  mock.failNext("candidates");
  const user = userEvent.setup();
  open();
  await user.click(await live("Change labels"));
  const picker = await screen.findByRole("group", { name: "Change labels" });
  expect(await within(picker).findByRole("alert")).toHaveTextContent("Couldn’t read the labels.");
  expect(within(picker).getByRole("button", { name: "Apply" })).toBeDisabled();
  await user.click(within(picker).getByRole("button", { name: "Retry" }));
  expect(await within(picker).findByRole("checkbox", { name: "docs" })).toBeVisible();
});

it("a tracker that cannot edit says so in its own words at the label picker (issue-write-tracker-not-supported)", async () => {
  const mock = createIssuesMock({ unsupportedCandidates: ["github"] });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Change labels"));
  const picker = await screen.findByRole("group", { name: "Change labels" });
  expect(await within(picker).findByRole("alert")).toHaveTextContent("Couldn’t read the labels. GitHub issue edits arrive in a later update.");
  expect(within(picker).getByRole("button", { name: "Apply" })).toBeDisabled();
  // Reading the candidates wrote nothing.
  expect(mock.writes).toEqual([]);
});

it("changes the assignees by the id the candidates carry, not the login", async () => {
  const sent: unknown[] = [];
  const mock = createIssuesMock();
  const update = mock.update;
  mock.update = async (ref, input) => { sent.push(input); return update(ref, input); };
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Change assignees"));
  const picker = await screen.findByRole("group", { name: "Change assignees" });
  await user.click(await within(picker).findByRole("checkbox", { name: /^sam/ }));
  await user.click(within(picker).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(screen.queryByRole("group", { name: "Change assignees" })).toBeNull());
  expect(sent).toEqual([{ assignees: ["u-1", "u-3"] }]);
  expect(within(screen.getByRole("group", { name: "Facts" })).getByText("sam", { selector: "dd *" })).toBeVisible();
});

it("keeps a current assignee the candidates do not carry, written by its login, so Apply is not blocked", async () => {
  const sent: unknown[] = [];
  // The candidates read is cut short before riley, who is on the issue: GitHub addresses a person by login, so the issue's
  // own login still addresses her and the whole set goes out instead of Apply waiting for her to be unchecked.
  const candidates: IssueCandidates = { labels: [], assignees: [{ login: "sam", id: "u-3" }], truncated: true };
  const mock = createIssuesMock({ candidates });
  const update = mock.update;
  mock.update = async (ref, input) => { sent.push(input); return update(ref, input); };
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Change assignees"));
  const picker = await screen.findByRole("group", { name: "Change assignees" });
  const riley = await within(picker).findByRole("checkbox", { name: "riley" });
  expect(riley).toBeChecked();
  expect(within(picker).queryByRole("alert")).toBeNull();
  // She can be unchecked and checked again like any other row, and nothing else needs a decision.
  await user.click(riley);
  expect(riley).not.toBeChecked();
  await user.click(riley);
  expect(riley).toBeChecked();
  expect(within(picker).getByRole("button", { name: "Apply" })).toBeDisabled();
  await user.click(within(picker).getByRole("checkbox", { name: /^sam/ }));
  await user.click(within(picker).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(screen.queryByRole("group", { name: "Change assignees" })).toBeNull());
  expect(sent).toEqual([{ assignees: ["riley", "u-3"] }]);
  const facts = screen.getByRole("group", { name: "Facts" });
  expect(within(facts).getByText("sam", { selector: "dd *" })).toBeVisible();
  expect(within(facts).getByText("riley", { selector: "dd *" })).toBeVisible();
});

it("unchecking a current assignee the candidates do not carry removes it in the same one write", async () => {
  const sent: unknown[] = [];
  const candidates: IssueCandidates = { labels: [], assignees: [{ login: "sam", id: "u-3" }], truncated: true };
  const mock = createIssuesMock({ candidates });
  const update = mock.update;
  mock.update = async (ref, input) => { sent.push(input); return update(ref, input); };
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await live("Change assignees"));
  const picker = await screen.findByRole("group", { name: "Change assignees" });
  await user.click(await within(picker).findByRole("checkbox", { name: "riley" }));
  await user.click(within(picker).getByRole("checkbox", { name: /^sam/ }));
  await user.click(within(picker).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(screen.queryByRole("group", { name: "Change assignees" })).toBeNull());
  // The whole set is what a write replaces, so unchecking her is a removal, not a silent drop.
  expect(sent).toEqual([{ assignees: ["u-3"] }]);
  const facts = screen.getByRole("group", { name: "Facts" });
  expect(within(facts).getByText("sam", { selector: "dd *" })).toBeVisible();
  expect(within(facts).queryByText("riley", { selector: "dd *" })).toBeNull();
});

it("draws hostile candidate names as text and survives a login called __proto__", async () => {
  const candidates: IssueCandidates = {
    labels: [{ name: "<img src=x onerror=alert(1)>" }, { name: "x".repeat(400) }],
    assignees: [{ login: "__proto__", name: "<b>Bob</b>", id: "u-9" }, { login: "constructor", id: "u-8" }],
    truncated: false,
  };
  setIssuesApi(createIssuesMock({ candidates }));
  const user = userEvent.setup();
  open();
  await user.click(await live("Change labels"));
  const labels = await screen.findByRole("group", { name: "Change labels" });
  expect(await within(labels).findByText("<img src=x onerror=alert(1)>")).toBeVisible();
  expect(labels.querySelector("img")).toBeNull();
  await user.click(within(labels).getByRole("button", { name: "Cancel" }));
  await user.click(await live("Change assignees"));
  const people = await screen.findByRole("group", { name: "Change assignees" });
  expect(await within(people).findByText("__proto__ · <b>Bob</b>")).toBeVisible();
  expect([...people.querySelectorAll("b")].some((one) => one.textContent === "Bob")).toBe(false);
  await user.click(within(people).getByRole("checkbox", { name: /^__proto__/ }));
  // riley is on the issue and not among the candidates: unchecking her still sends only the candidate's id.
  await user.click(within(people).getByRole("checkbox", { name: "riley" }));
  await user.click(within(people).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(screen.queryByRole("group", { name: "Change assignees" })).toBeNull());
  expect(Object.prototype.hasOwnProperty.call(Object.prototype, "u-9")).toBe(false);
});

// ---- the State row and the page's one-write-at-a-time guard ----

it("a state change is one write with the state's name, and the page shows the issue the service answered", async () => {
  const mock = createIssuesMock({ delayMs: 20 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await screen.findByRole("button", { name: "State: Open" }));
  const menu = await screen.findByRole("menu", { name: "Change the state" });
  await user.dblClick(within(menu).getByRole("menuitem", { name: "Closed" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "State: Closed" })).toBeVisible());
  expect(mock.writes).toEqual(["state:412:Closed:completed"]);
});

it("a state that meets 'no such route' says nothing changed, and the row goes back to Planned", async () => {
  const mock = createIssuesMock();
  mock.setState = async () => { throw new ApiFailure("http_404", "The server answered 404.", 404); };
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await screen.findByRole("button", { name: "State: Open" }));
  await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Closed" }));
  await waitFor(() => expect(useUi.getState().toasts.some((toast) => /does not offer state changes yet\. Nothing changed\./.test(toast.text))).toBe(true));
  expect(screen.queryByRole("button", { name: /^State:/ })).toBeNull();
  const facts = screen.getByRole("group", { name: "Facts" });
  expect(within(facts).getByText("State", { selector: "dt" }).nextElementSibling).toHaveTextContent("Planned");
});

it("a tracker with no state to offer shows the state as text, with no button and no Planned pill", async () => {
  setIssuesApi(createIssuesMock({ fixedState: ["github"] }));
  open();
  const facts = await screen.findByRole("group", { name: "Facts" });
  expect(screen.queryByRole("button", { name: /^State:/ })).toBeNull();
  expect(within(facts).getByText("State", { selector: "dt" }).nextElementSibling).not.toHaveTextContent("Planned");
});

it("while one write is in flight no other control on the page can start one", async () => {
  const mock = createIssuesMock({ delayMs: 60 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.click(await screen.findByRole("button", { name: "State: Open" }));
  await user.click(within(await screen.findByRole("menu")).getByRole("menuitem", { name: "Closed" }));
  expect(screen.getByRole("button", { name: "Edit description" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Change labels" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Change assignees" })).toBeDisabled();
  await waitFor(() => expect(screen.getByRole("button", { name: "Edit description" })).toBeEnabled());
  expect(writesOf(mock)).toHaveLength(1);
});
