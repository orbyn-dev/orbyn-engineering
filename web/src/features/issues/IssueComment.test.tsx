import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import type { Project, Thread } from "../../api/types";
import { useWorkspace } from "../../product/state/workspace";
import { IssuesPage } from "./IssuesPage";
import { setIssuesApi } from "./api";
import { resetIssueLinks } from "./links";

const project = { id: "p1", name: "orbyn-engineering", path: "/work/orbyn" } as Project;
const thread = { id: "thread-1", project_id: "p1", title: "Fix login redirect", engine: "claude", cwd: "/", worktree: null } as Thread;
const ISSUE = "#/issues/github/orbyn-dev%2Forbyn-engineering/412";

beforeEach(() => {
  resetIssueLinks();
  useWorkspace.setState({ projects: { data: [project], error: null, loading: false }, threads: { "thread-1": thread } });
});
afterEach(() => { setIssuesApi(null); window.location.hash = ""; vi.restoreAllMocks(); });

const open = () => { window.location.hash = ISSUE; return render(<IssuesPage />); };
const box = async () => (await screen.findByRole("textbox", { name: "Comment" })) as HTMLTextAreaElement;
const commentButton = () => screen.getByRole("button", { name: "Comment" });

it("shows the comments as the activity so far and posts one comment with one write", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  // The tracker's comments are the activity, oldest first, under the description.
  expect(await screen.findByText("Reproduced. The redirect reads", { exact: false })).toBeVisible();
  expect(screen.getByText("The picker has the same problem", { exact: false })).toBeVisible();
  expect(screen.getAllByRole("article")).toHaveLength(3);
  const area = await box();
  // An untouched box is not a mistake, and there is nothing to send.
  expect(commentButton()).toBeDisabled();
  expect(screen.queryByRole("alert")).toBeNull();
  await user.type(area, "Fixed on main.");
  expect(commentButton()).toBeEnabled();
  await user.click(commentButton());
  await waitFor(() => expect(area.value).toBe(""));
  expect(mock.writes).toEqual(["comment:412:Fixed on main."]);
  // The comment the service answered joins the activity, after the tracker's own.
  await waitFor(() => expect(screen.getAllByRole("article")).toHaveLength(4));
  expect(screen.getAllByRole("article")[3]).toHaveTextContent("Fixed on main.");
});

it("one click is one write: a second click while it is in flight sends nothing", async () => {
  const mock = createIssuesMock({ delayMs: 30 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.type(await box(), "Once");
  await user.dblClick(commentButton());
  await waitFor(() => expect(mock.writes).toEqual(["comment:412:Once"]));
});

it("two submits in the same tick are one write: the page's own guard refuses the second, whatever the button shows", async () => {
  const mock = createIssuesMock({ delayMs: 30 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.type(await box(), "Once");
  const form = screen.getByRole("form", { name: "Comment" });
  // Both fire before React re-renders the busy button, as Enter held down or a double submit can.
  act(() => { fireEvent.submit(form); fireEvent.submit(form); });
  await waitFor(() => expect(mock.writes).toEqual(["comment:412:Once"]));
  await new Promise((done) => setTimeout(done, 80));
  expect(mock.writes).toEqual(["comment:412:Once"]);
});

it("sends the comment exactly as typed: the contract refuses a blank body and does not trim a real one", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.type(await box(), "  hi  ");
  await user.click(commentButton());
  await waitFor(() => expect(mock.writes).toEqual(["comment:412:  hi  "]));
});

it("a failed write loses nothing: the draft stays, the form says nothing changed, and the retry works", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  mock.failNext("comment");
  const user = userEvent.setup();
  open();
  const area = await box();
  await user.type(area, "My careful reply");
  await user.click(commentButton());
  expect(await screen.findByRole("alert")).toHaveTextContent("GitHub did not answer. Nothing changed.");
  expect(area).toHaveValue("My careful reply");
  await user.click(commentButton());
  await waitFor(() => expect(area.value).toBe(""));
  expect(screen.getByText("My careful reply")).toBeVisible();
});

it("a blank comment sends nothing, and the empty mistake is said only after something was typed", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  const area = await box();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(commentButton()).toBeDisabled();
  await user.type(area, "   ");
  expect(screen.getByRole("alert")).toHaveTextContent("The comment cannot be empty.");
  expect(commentButton()).toBeDisabled();
  await user.click(commentButton());
  expect(mock.writes).toEqual([]);
});

it("refuses a comment over 64 KiB before sending anything", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  open();
  const area = await box();
  fireEvent.change(area, { target: { value: "x".repeat(65_537) } });
  expect(screen.getByRole("alert")).toHaveTextContent("The comment is over 64 KiB.");
  expect(commentButton()).toBeDisabled();
  // At the limit it may go.
  fireEvent.change(area, { target: { value: "x".repeat(65_536) } });
  expect(screen.queryByRole("alert")).toBeNull();
  expect(commentButton()).toBeEnabled();
  expect(mock.writes).toEqual([]);
});

it("a first write that meets 'no such route' says so and the box goes back to Planned: no success is shown", async () => {
  const mock = createIssuesMock({ noComment: true });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  await user.type(await box(), "Trying anyway");
  await user.click(commentButton());
  expect(await screen.findByText("Planned")).toBeVisible();
  expect(commentButton()).toBeDisabled();
  expect(await box()).toBeDisabled();
  expect(mock.writes).toEqual(["comment:412:Trying anyway"]);
  await user.click(commentButton());
  expect(mock.writes).toEqual(["comment:412:Trying anyway"]);
});
