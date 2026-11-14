import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { useConnection } from "../../../../product/state/connection";
import { installMockFetch } from "../../../../test-support";
import { CrewNavRow } from "./CrewNavRow";
import { FlowView } from "./FlowView";
import { FlowZoom } from "./FlowZoom";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  useConnection.setState({ remote: false });
  window.location.hash = "";
  window.localStorage.clear();
});

const calls = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));
const writes = () => vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method && init.method !== "GET");
const urls = () => vi.mocked(fetch).mock.calls.map(([url]) => String(url));

async function open(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  const user = userEvent.setup();
  const view = render(<FlowView />);
  return { user, view };
}
const table = () => screen.findByRole("list", { name: "Tasks" });
const rowsOf = async () => within(await table()).getAllByRole("listitem");

describe("Flow on the real routes", () => {
  it("draws nothing and reads no Flow route while Crew Preview is off (the default scenario answers 404)", async () => {
    mock.reset("default");
    const { container } = render(<FlowView />);
    await waitFor(() => expect(calls("/api/crew/preview")).toHaveLength(1));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
    expect(calls("/api/crew/flow")).toHaveLength(0);
  });

  it("lists the crew's tasks, the ones that need you first, with the ten cells of each and the short crew id", async () => {
    await open("crew");
    const rows = await rowsOf();
    expect(rows).toHaveLength(6);
    expect(rows[0]).toHaveTextContent("Project icon picker");
    expect(rows[0]).toHaveTextContent("Build: Needs you");
    expect(rows[1]).toHaveTextContent("Use next in the auth redirect");
    expect(rows[1]).toHaveTextContent("Checks: Running 2/3");
    expect(rows[1]).toHaveTextContent("Code review: Running ×3");
    expect(rows[1]).toHaveTextContent("Round 1");
    expect(rows[1]).toHaveTextContent("implement · claude · owner Lead");
    const queued = rows.find((row) => within(row).queryByText("Move the CHANGELOG entry"))!;
    expect(queued).toHaveTextContent("Queued");
    const merged = rows.find((row) => within(row).queryByText("Update the login docs"))!;
    expect(merged).toHaveTextContent("Security review: Skipped");
    expect(merged).toHaveTextContent("You: Waiting for your review after the merge");
    const failed = rows.find((row) => within(row).queryByText("Rename the legacy returnTo helper"))!;
    expect(failed).toHaveTextContent("Round 3");
    expect(screen.getByRole("combobox", { name: "Crew" })).toHaveDisplayValue("crew-redirec · 6 tasks, 1 finished");
    expect(screen.getByText("6 of 6 shown")).toBeVisible();
  });

  it("counts the filter chips over the whole crew and a chip narrows the list with a read of its own", async () => {
    const { user } = await open("crew");
    await table();
    const bar = screen.getByRole("group", { name: "Filter tasks" });
    expect(within(bar).getAllByRole("button").map((one) => one.textContent)).toEqual(["All 6", "Needs you 1", "Reviewing 1", "Failed 2", "Passed 1"]);
    expect(within(bar).getByRole("button", { name: "All 6" })).toHaveAttribute("aria-pressed", "true");
    await user.click(within(bar).getByRole("button", { name: "Failed 2" }));
    await waitFor(() => expect(within(bar).getByRole("button", { name: "Failed 2" })).toHaveAttribute("aria-pressed", "true"));
    await waitFor(async () => expect(await rowsOf()).toHaveLength(2));
    expect(urls().some((url) => url.includes("/api/crew/flow/tasks?") && url.includes("filter=failed"))).toBe(true);
    // The counts stay those of the whole crew.
    expect(within(bar).getByRole("button", { name: "All 6" })).toBeVisible();
    expect(screen.getByText("2 of 2 shown")).toBeVisible();
    expect(writes()).toHaveLength(0);
  });

  it("finds a task by its text and narrows by engine, as reads", async () => {
    const { user } = await open("crew");
    await table();
    await user.type(screen.getByRole("searchbox"), "login");
    await waitFor(() => expect(urls().some((url) => url.includes("q=login"))).toBe(true));
    await waitFor(async () => expect(await rowsOf()).toHaveLength(2));
    await user.selectOptions(screen.getByRole("combobox", { name: "Engine" }), "codex");
    await waitFor(() => expect(urls().some((url) => url.includes("engine=codex"))).toBe(true));
    await waitFor(async () => expect(await rowsOf()).toHaveLength(2));
    expect(writes()).toHaveLength(0);
  });

  it("switches to another crew", async () => {
    const { user } = await open("crew");
    await table();
    await user.selectOptions(screen.getByRole("combobox", { name: "Crew" }), "crew-docs-0002");
    await waitFor(async () => expect(await rowsOf()).toHaveLength(1));
    expect(screen.getByText("1 of 1 shown")).toBeVisible();
  });

  it("zooms one task: stages with their lanes, brief, route, findings, gates, rounds, merge, main and you, as plain text", async () => {
    const { user } = await open("crew");
    await user.click(within(await screen.findByRole("list", { name: "Tasks" })).getByRole("button", { name: /Update the login docs/ }));
    expect(await screen.findByRole("heading", { name: "Update the login docs" })).toBeVisible();
    const stages = [...screen.getByRole("list", { name: "Stages" }).querySelectorAll<HTMLElement>(":scope > [role=listitem]")];
    expect(stages).toHaveLength(10);
    expect(stages[3]).toHaveTextContent("Checks2 of 2");
    expect(stages[4]).toHaveTextContent("Code review");
    expect(stages[4]).toHaveTextContent("Docs · codex · GPT-5.5No findings");
    expect(stages[5]).toHaveTextContent("Security reviewSkipped");
    expect(stages[7]).toHaveTextContent("Merge queueMerged");
    expect(stages[8]).toHaveTextContent("MainHealthy");
    const brief = screen.getByRole("region", { name: "Brief" });
    expect(brief).toHaveTextContent("Say how the redirect works");
    expect(brief).toHaveTextContent("40 turns · 30 min");
    expect(screen.getByRole("region", { name: "Route" })).toHaveTextContent("Terra");
    const gates = screen.getByRole("region", { name: "Gates" });
    // A task that is over shows a gate's reason; the service's text is plain text, never markup.
    expect(gates).toHaveTextContent("2 of 2 checks ran on the commit and passed");
    expect(gates).toHaveTextContent("Merged by Crew <b>docs</b>");
    expect(gates.querySelector("b b")).toBeNull();
    expect(screen.getByRole("region", { name: "Rounds" })).toHaveTextContent("Round 1 · passed");
    expect(screen.getByRole("region", { name: "Merge and main" })).toHaveTextContent("low");
    expect(screen.getByRole("region", { name: "You" })).toHaveTextContent("waiting");
    // No check, command, output or "proves" text is in the data, so none is drawn.
    expect(document.body.textContent).not.toMatch(/exit \d|Proves|npm test|tsc --noEmit/);
  });

  it("draws Cancel, Re-run checks and Review inert, and calls no write route", async () => {
    const { user } = await open("crew");
    await user.click(within(await screen.findByRole("list", { name: "Tasks" })).getByRole("button", { name: /Update the login docs/ }));
    await screen.findByRole("heading", { name: "Update the login docs" });
    for (const name of ["Cancel", "Re-run checks", "Review"]) expect(screen.getByRole("button", { name })).toBeDisabled();
    expect(screen.getByText(/Cancel and Re-run checks: Available when the write route ships/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Review" })).toHaveAttribute("aria-description", "The Review queue is not in this app yet");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await user.click(screen.getByRole("button", { name: "Re-run checks" }));
    expect(writes()).toHaveLength(0);
    expect(urls().some((url) => /cancel|rerun/.test(url))).toBe(false);
    await user.click(screen.getByRole("button", { name: "All tasks" }));
    expect(await screen.findByRole("list", { name: "Tasks" })).toBeVisible();
  });

  it("shows a confirmed finding that blocks, and no gate reason to a task that is not over", async () => {
    const { user } = await open("crew");
    await user.click(within(await screen.findByRole("list", { name: "Tasks" })).getByRole("button", { name: /Add a rate limit/ }));
    const findings = await screen.findByRole("region", { name: "Findings" });
    expect(findings).toHaveTextContent("The limit resets on every request");
    expect(findings).toHaveTextContent("confirmed");
    expect(findings).toHaveTextContent("auth/login.ts:41");
    expect(findings).toHaveTextContent("blocks");
  });

  it("says a task Flow does not know is not there (404 on the zoom)", async () => {
    mock.reset("crew");
    render(<FlowZoom id="task-gone" onBack={() => undefined} />);
    expect(await screen.findByText("This task isn't in Flow")).toBeVisible();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says there are no crews, and a crew with no tasks", async () => {
    await open("crew-empty");
    expect(await screen.findByText("No crews running")).toBeVisible();
  });

  it("says a crew has no tasks yet", async () => {
    await open("crew-flow-empty");
    expect(await screen.findByText("No tasks yet")).toBeVisible();
  });

  it("says the list was cut at the service's cap when it is truncated", async () => {
    await open("crew-flow-truncated");
    expect(await screen.findByRole("note")).toHaveTextContent("This crew has more tasks than one read covers; the newest are shown.");
  });

  it("pages with Show more, as reads", async () => {
    const { user } = await open("crew-flow-many");
    expect(await rowsOf()).toHaveLength(50);
    expect(screen.getByText("50 of 60 shown")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Show more" }));
    await waitFor(async () => expect(await rowsOf()).toHaveLength(60));
    expect(screen.queryByRole("button", { name: "Show more" })).toBeNull();
    expect(urls().some((url) => url.includes("offset=50"))).toBe(true);
    expect(writes()).toHaveLength(0);
  });

  it("names an unreadable service and retries with reads only", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/crews", 1);
    const user = userEvent.setup();
    render(<FlowView />);
    expect(await screen.findByText("Can't read Flow")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await table()).toBeVisible();
    expect(writes()).toHaveLength(0);
  });

  it("names a task list that cannot be read and retries it", async () => {
    mock.reset("crew");
    mock.fail("/api/crew/flow/tasks", 1);
    const user = userEvent.setup();
    render(<FlowView />);
    expect(await screen.findByText("Can't read Flow")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await table()).toBeVisible();
    expect(writes()).toHaveLength(0);
  });
});

describe("the Crew row of the left bar", () => {
  it("shows with the Preview label and opens the Crew view on Flow while Crew Preview is on", async () => {
    mock.reset("crew");
    render(<CrewNavRow route={{ name: "home" }} />);
    const link = await screen.findByRole("link", { name: /Crew/ });
    expect(link).toHaveTextContent("Preview");
    expect(link).toHaveAttribute("href", "#/crew/flow");
    expect(link).not.toHaveAttribute("aria-current");
  });

  it("is marked current on a Crew page and draws nothing while Crew Preview is off", async () => {
    mock.reset("crew");
    const on = render(<CrewNavRow route={{ name: "crew", tab: "flow" }} />);
    expect(await screen.findByRole("link", { name: /Crew/ })).toHaveAttribute("aria-current", "page");
    on.unmount();
    mock.reset("default");
    const { container } = render(<CrewNavRow route={{ name: "home" }} />);
    await waitFor(() => expect(calls("/api/info").length).toBeGreaterThan(0));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
    // The row reads the service's info, never a Crew route: with Preview off that route answers 404, which the browser logs.
    expect(calls("/api/crew")).toHaveLength(0);
  });
});
