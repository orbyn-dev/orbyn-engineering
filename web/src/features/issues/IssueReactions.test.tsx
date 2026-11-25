import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import { useUi } from "../../product/state/ui";
import { IssuesPage } from "./IssuesPage";
import { setIssuesApi } from "./api";
import { resetIssueLinks } from "./links";

// A browser tab has no desktop shell, so `canSendPersonKey` is false there and the Person-class react is not offered.
const shell = vi.hoisted(() => ({ canSend: true }));
vi.mock("../../api/personKey", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../api/personKey")>();
  return { ...real, canSendPersonKey: () => shell.canSend };
});

const ISSUE = "#/issues/github/orbyn-dev%2Forbyn-engineering/412";
const open = () => { window.location.hash = ISSUE; render(<IssuesPage />); };
const bar = async () => screen.findByRole("group", { name: "Reactions" });
const chips = (element: HTMLElement) => [...element.querySelectorAll(".iss-react-chip")].map((chip) => chip.textContent);

beforeEach(() => { resetIssueLinks(); shell.canSend = true; });
afterEach(() => { setIssuesApi(null); window.location.hash = ""; vi.restoreAllMocks(); });

it("draws the counts the last write reported as words and counts, and adds one with one write", async () => {
  const mock = createIssuesMock({ reactions: { "412": [{ content: "THUMBS_UP", count: 3 }, { content: "HEART", count: 1 }] } });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  const reactions = await bar();
  await waitFor(() => expect(chips(reactions)).toEqual(["Thumbs up3", "Heart1"]));
  await user.click(within(reactions).getByRole("button", { name: "Add reaction" }));
  const menu = await screen.findByRole("menu", { name: "Add reaction" });
  await user.click(within(menu).getByRole("menuitem", { name: "Heart" }));
  await waitFor(() => expect(chips(reactions)).toEqual(["Thumbs up3", "Heart2"]));
  expect(mock.writes).toEqual(["react:412:heart:on"]);
});

it("a plain detail read carries no counts: the bar offers only Add reaction until a write answers", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  const reactions = await bar();
  expect(chips(reactions)).toEqual([]);
  await user.click(within(reactions).getByRole("button", { name: "Add reaction" }));
  const menu = await screen.findByRole("menu", { name: "Add reaction" });
  await user.click(within(menu).getByRole("menuitem", { name: "Rocket" }));
  await waitFor(() => expect(chips(reactions)).toEqual(["Rocket1"]));
  expect(mock.writes).toEqual(["react:412:rocket:on"]);
});

it("lists the eight reactions the route takes, in the frame's order", async () => {
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  open();
  const reactions = await bar();
  await user.click(within(reactions).getByRole("button", { name: "Add reaction" }));
  const menu = await screen.findByRole("menu", { name: "Add reaction" });
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
    "Thumbs up", "Thumbs down", "Laugh", "Hooray", "Confused", "Heart", "Rocket", "Eyes",
  ]);
});

it("is add only: the counts are text and the only control adds another", async () => {
  const mock = createIssuesMock({ reactions: { "412": [{ content: "THUMBS_UP", count: 3 }] } });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  const reactions = await bar();
  await waitFor(() => expect(chips(reactions)).toEqual(["Thumbs up3"]));
  // No toggle, no "take mine back": the chip is text and the one button adds a reaction.
  expect(within(reactions).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([null]);
  expect(within(reactions).getAllByRole("button", { name: "Add reaction" })).toHaveLength(1);
  await user.click(reactions.querySelector(".iss-react-chip")!);
  expect(mock.writes).toEqual([]);
});

it("a refused add says nothing changed and changes no count", async () => {
  const mock = createIssuesMock({ reactions: { "412": [{ content: "THUMBS_UP", count: 3 }] } });
  setIssuesApi(mock);
  mock.failNext("react");
  const user = userEvent.setup();
  open();
  const reactions = await bar();
  await waitFor(() => expect(chips(reactions)).toEqual(["Thumbs up3"]));
  await user.click(within(reactions).getByRole("button", { name: "Add reaction" }));
  const menu = await screen.findByRole("menu", { name: "Add reaction" });
  await user.click(within(menu).getByRole("menuitem", { name: "Heart" }));
  await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Couldn't add the reaction. Nothing changed. GitHub did not answer."]));
  expect(chips(reactions)).toEqual(["Thumbs up3"]);
});

it("a first no such route hides the bar and shows no success", async () => {
  const mock = createIssuesMock({ noReact: true });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  const reactions = await bar();
  await user.click(within(reactions).getByRole("button", { name: "Add reaction" }));
  const menu = await screen.findByRole("menu", { name: "Add reaction" });
  await user.click(within(menu).getByRole("menuitem", { name: "Thumbs up" }));
  await waitFor(() => expect(screen.queryByRole("group", { name: "Reactions" })).toBeNull());
  expect(mock.writes).toEqual(["react:412:thumbs_up:on"]);
  expect(useUi.getState().toasts).toEqual([]);
});

it("a write in flight takes the choice once: a second add while it runs sends nothing", async () => {
  const mock = createIssuesMock({ delayMs: 150 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  open();
  const reactions = await bar();
  await user.click(within(reactions).getByRole("button", { name: "Add reaction" }));
  const menu = await screen.findByRole("menu", { name: "Add reaction" });
  await user.click(within(menu).getByRole("menuitem", { name: "Thumbs up" }));
  expect(within(reactions).getByRole("button", { name: "Add reaction" })).toBeDisabled();
  await waitFor(() => expect(chips(reactions)).toEqual(["Thumbs up1"]));
  expect(mock.writes).toEqual(["react:412:thumbs_up:on"]);
});

it("does not offer the Person-class reaction bar in a browser tab with no desktop shell", async () => {
  shell.canSend = false;
  setIssuesApi(createIssuesMock());
  open();
  expect(await screen.findByRole("heading", { name: "Login sends people back to /home" })).toBeVisible();
  expect(screen.queryByRole("group", { name: "Reactions" })).toBeNull();
});
