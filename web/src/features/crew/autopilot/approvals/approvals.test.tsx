import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { installMockFetch } from "../../../../test-support";
import { ApprovalsTab } from "./ApprovalsTab";
import { ApprovalCards } from "./ApprovalRows";
import { crewApi } from "./api";
import { rowOf } from "./ledger";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

async function open(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  const user = userEvent.setup();
  render(<ApprovalsTab />);
  await screen.findByRole("region", { name: "Approvals" });
  return user;
}

const rowOf2 = (name: string | RegExp) => screen.getByRole("button", { name }).closest('[role="row"]') as HTMLElement;
const drawer = (name: string) => screen.getByRole("complementary", { name });

describe("the approval log", () => {
  it("lists every request with its hard rule, the lead, the second key and one result", async () => {
    await open("crew");
    expect(screen.getByText("Autopilot Full · the lead decides, inside limits set in code")).toBeVisible();
    expect(screen.getByText("Today · Sat 7 Nov 2026".replace("Today · ", ""))).toBeVisible();
    const table = screen.getByRole("table", { name: "Approval log" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Time", "Request", "Hard rule (code)", "The lead", "Second key", "Result"]);
    const read = rowOf2(/Read auth\/redirect\.ts/);
    expect(read).toHaveTextContent("14:12");
    expect(read).toHaveTextContent("Task task-redirect · inside its lease");
    expect(read).toHaveTextContent("Always allowedNot askedNot neededAllowed");
    expect(rowOf2(/Install zod/)).toHaveTextContent("PassedYes: it’s pinned to one version and the brief asks for schema testsopencode · yesAllowed");
    expect(rowOf2(/Write \.\.\/shared/)).toHaveTextContent("DeniedNot askedNot askedDenied by rule");
    expect(rowOf2(/scripts\/gen\.sh/)).toHaveTextContent("PassedNo: the script isn’t in the brief, and the worker didn’t say what it changesNot askedDenied by lead");
    expect(rowOf2(/gen-migration/)).toHaveTextContent("opencode · noKeys disagree · denied");
    expect(rowOf2(/Publish the package/)).toHaveTextContent("Person-onlyNot askedNot askedDenied · for your review");
    expect(screen.getByText("Every decision is in the ledger with its reason.")).toBeVisible();
    // A short log has no filters.
    expect(screen.queryByRole("searchbox")).toBeNull();
  });

  it("says what it will hold before any agent asks for anything", async () => {
    await open("crew-empty");
    expect(await screen.findByText("No requests yet")).toBeVisible();
    expect(screen.getByText(/When an agent asks to run a command, install something or reach the network/)).toBeVisible();
    expect(screen.queryByRole("table")).toBeNull();
  });

  it("filters and searches the loaded requests, and says that is what they cover", async () => {
    const user = await open("crew-many");
    expect(await screen.findByRole("status")).toHaveTextContent("Showing 1 to 18 of the 18 loaded requests");
    expect(screen.getByRole("button", { name: /^All 18/ })).toBeVisible();
    await user.click(screen.getByRole("button", { name: /^Keys disagree 3/ }));
    await waitFor(() => expect(within(screen.getByRole("table", { name: "Approval log" })).getAllByRole("row")).toHaveLength(4));
    await user.click(screen.getByRole("button", { name: /^All 18/ }));
    await user.type(screen.getByRole("searchbox", { name: "Find a request" }), "publish");
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Showing 1 to 3 of the 18 loaded requests"));
  });

  it("reads the next page of the ledger when the list is scrolled to its end", async () => {
    await open("crew-many");
    const list = await screen.findByRole("region", { name: "Requests" });
    Object.defineProperty(list, "clientHeight", { value: 100, configurable: true });
    Object.defineProperty(list, "scrollHeight", { value: 120, configurable: true });
    list.dispatchEvent(new Event("scroll", { bubbles: true }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("of the 36 loaded requests"));
  });

  it("draws each request as a card in the 320 px panel", async () => {
    mock.reset("crew");
    const page = await crewApi.approvals({ offset: 0 });
    const rows = page.rows.map((one) => rowOf(one, [])).flatMap((one) => (one ? [one] : []));
    const opened: string[] = [];
    const user = userEvent.setup();
    render(<ApprovalCards rows={rows.slice(1, 4)} onSelect={(id) => opened.push(id)} />);
    const cards = screen.getAllByRole("listitem");
    expect(cards).toHaveLength(3);
    expect(cards[0]).toHaveTextContent("14:37");
    expect(cards[0]).toHaveTextContent("Run node db/gen-migration.js");
    expect(cards[0]).toHaveTextContent("Keys disagree · denied");
    await user.click(screen.getByRole("button", { name: /scripts\/gen\.sh/ }));
    expect(opened).toEqual(["4"]);
  });
});

describe("the decision chain", () => {
  it("shows an allowed request with both keys and what the lead was shown", async () => {
    const user = await open("crew");
    await user.click(screen.getByRole("button", { name: /Install zod/ }));
    const d = drawer("Install zod@3.24.1");
    expect(d).toHaveTextContent("Allowed");
    expect(d).toHaveTextContent("Task task-redirect");
    expect(d).toHaveTextContent("Actioninstall");
    const steps = within(within(d).getByRole("list", { name: "Decision chain" })).getAllByRole("listitem");
    expect(steps[0]).toHaveTextContent("Hard rulePassed. Not a denied kind of action.");
    expect(steps[1]).toHaveTextContent("The lead said yesIt’s pinned to one version and the brief asks for schema tests.");
    expect(steps[2]).toHaveTextContent("Second key: yesopencode, a different vendor from the lead.");
    expect(d).toHaveTextContent("It never saw raw page or tool text.");
    expect(within(d).getByRole("button", { name: "Close" })).toBeVisible();
    // Nothing in the app opens a ledger yet, so no button pretends to.
    expect(within(d).queryByRole("button", { name: "Open in the ledger" })).toBeNull();
  });

  it("names the hard rule for a request denied by code, and that nobody was asked", async () => {
    const user = await open("crew");
    await user.click(screen.getByRole("button", { name: /Write \.\.\/shared/ }));
    const d = drawer("Write ../shared/config.json");
    expect(d).toHaveTextContent("Denied by ruleHard limit");
    expect(d).toHaveTextContent("Denied by a hard ruleWriting or deleting outside the worktree is always denied.");
    expect(d).toHaveTextContent("A rule decided. The lead can’t overrule it.");
    expect(d).toHaveTextContent("A second key applies to medium risk.");
    expect(within(d).getByRole("button", { name: "See the hard-denied list" })).toBeVisible();
  });

  it("opens the list of what is always denied from a rule denial", async () => {
    const user = await open("crew");
    await user.click(screen.getByRole("button", { name: /Write \.\.\/shared/ }));
    await user.click(screen.getByRole("button", { name: "See the hard-denied list" }));
    const dialog = await screen.findByRole("dialog", { name: "What Autopilot can never do" });
    expect(await within(dialog).findByText("Writing or deleting outside the worktree")).toBeVisible();
    expect(within(dialog).getByText("Reading or sending secrets and credentials")).toBeVisible();
    expect(within(dialog).getByText("Why code and not a model")).toBeVisible();
  });

  it("gives the lead's no and says no second key was asked", async () => {
    const user = await open("crew");
    await user.click(screen.getByRole("button", { name: /scripts\/gen\.sh/ }));
    const d = drawer("Run scripts/gen.sh");
    expect(d).toHaveTextContent("Denied by lead");
    expect(d).toHaveTextContent("The lead said noThe script isn’t in the brief, and the worker didn’t say what it changes.");
    expect(d).toHaveTextContent("Not askedNo second key after a no.");
  });

  it("says in plain words that two keys disagreed, and that the request was denied", async () => {
    const user = await open("crew");
    await user.click(screen.getByRole("button", { name: /gen-migration/ }));
    const d = drawer("Run node db/gen-migration.js");
    expect(d).toHaveTextContent("Second key: noopencode · a different vendor: it writes to a table outside the brief.");
    expect(d).toHaveTextContent("Denied, because the keys disagree");
    expect(d).toHaveTextContent("nobody is asked to break the tie unless the task needs this action to go on");
  });

  it("marks a person-only request amber and says nothing is blocked; answering it waits for the route", async () => {
    const user = await open("crew");
    await user.click(screen.getByRole("button", { name: /Publish the package/ }));
    const d = drawer("Publish the package to the registry");
    expect(d).toHaveTextContent("Denied · for your reviewPerson-only");
    expect(d).toHaveTextContent("Publishing or messaging outside the repo is something you do yourself.");
    expect(d).toHaveTextContent("The lead can’t approve it.");
    expect(d).toHaveTextContent("A person-only action has no second key.");
    expect(d).toHaveTextContent("Waiting for your review, not blocking anything");
    const dismiss = within(d).getByRole("button", { name: "Dismiss" });
    expect(dismiss).toBeDisabled();
    expect(dismiss).toHaveAttribute("title", "Crew is not connected to this app yet");
    expect(d).toHaveTextContent("Crew is not connected to this app yet");
  });

  it("under Full with one vendor, denies the request and leaves a Needs you item that blocks nothing", async () => {
    const user = await open("crew-one-vendor");
    await user.click(screen.getByRole("button", { name: /Install zod/ }));
    const d = drawer("Install zod@3.24.1");
    expect(d).toHaveTextContent("Denied · no second key");
    expect(d).toHaveTextContent("Second key: not availableMedium risk needs an approver on a different vendor, and no second vendor is signed in.");
    expect(d).toHaveTextContent("Denied under Full, not asked");
    expect(d).toHaveTextContent("“Sign in to a second engine to allow: install zod@3.24.1”");
    expect(within(d).getByRole("button", { name: "Sign in to a second engine" })).toBeVisible();
  });

  it("closes with Escape", async () => {
    const user = await open("crew");
    await user.click(screen.getByRole("button", { name: /Install zod/ }));
    expect(drawer("Install zod@3.24.1")).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("complementary")).toBeNull();
  });
});

describe("trip wires", () => {
  it("stops the task amber on too many denials, with the requests and why each was denied", async () => {
    await open("crew-tripwire");
    const stop = await screen.findByRole("region", { name: "Stopped: task-client" });
    expect(stop).toHaveTextContent("Stopped · too many denials.");
    expect(stop).toHaveTextContent("The task “task-client” was denied 3 times in a row, so Crew stopped it. Nothing was lost; its worktree is intact.");
    const table = await within(stop).findByRole("table", { name: "Denied requests" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Time", "Request", "Why it was denied", "Result"]);
    await waitFor(() => expect(table).toHaveTextContent("Fetch https://cdn.example/specNetwork to hosts the repo hasn’t allowedDenied by rule"));
    expect(table).toHaveTextContent("Run scripts/gen.shNot in the briefDenied by lead");
    expect(within(stop).getByRole("status")).toHaveClass("ap-banner-am");
    expect(stop).toHaveTextContent("The app stops the task on 3 denials in a row, with the reason “too many denials”. The second rule is 20 denials in one run.");
    expect(within(stop).getByRole("button", { name: "Review the denials" })).toBeVisible();
    // The brief editor is not built here, so no button pretends to open one.
    expect(within(stop).queryByRole("button", { name: "Write a new brief" })).toBeNull();
  });

  it("says text looked like injected instructions, and that nothing was lost; the ledger keeps no text to quote", async () => {
    await open("crew-injected");
    const stop = await screen.findByRole("region", { name: "Stopped: task-client" });
    expect(stop).toHaveTextContent("Stopped · possible injected instructions.");
    expect(stop).toHaveTextContent("read text that looks like instructions to the agent, so Crew stopped it. Nothing was lost; its worktree is intact.");
    expect(stop).toHaveTextContent("Text a worker reads is data, never an instruction.");
    expect(within(stop).queryByRole("table")).toBeNull();
    expect(within(stop).queryByRole("button", { name: "Review the text" })).toBeNull();
  });
});
