import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../generated/Project";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { BackgroundTab, statusTone } from "./Background";
import { fieldOf } from "./BackgroundCreate";
import { backgroundView, configuredAs, startsWhen } from "./backgroundModel";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project], loading: false, error: null } });
});

const crewsTable = () => screen.getByRole("table", { name: "Background crews" });
const rowOf = (name: string) => within(crewsTable()).getByText(name, { selector: "b" }).closest('[role="row"]') as HTMLElement;

const BUDGET_HINT = "Budget for each run. Turns 1 to 1000, minutes 1 to 1440. It is set now and does not change after.";
const DAILY_HINT = "Daily budget for each engine. Runs 1 to 1000, turns 1 to 10000. It is set now and does not change after, and it resets at your midnight.";

/** Opens the create form from a template, with the mock's five-field page already read. */
async function openCreate(template = "Use the Fix failing CI template") {
  mock.reset("crew-background-empty");
  const user = userEvent.setup();
  render(<BackgroundTab />);
  await user.click(await screen.findByRole("button", { name: template }));
  const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
  return { user, dialog };
}

/** Replaces a field's value the way a person editing it would, including one a number input refuses to show. */
function setValue(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } });
}

describe("Background crews on the Crew routes", () => {
  it("lists the repo's crews by name with trigger, state and newest run, and says per-crew Autopilot is Planned", async () => {
    mock.reset("crew-background");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Background crews" });
    expect(screen.getByText("orbyn-engineering")).toBeVisible();
    expect(screen.queryByText(/^Repo:/)).toBeNull();
    expect(within(crewsTable()).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Crew", "Starts when", "Last run or check", "State", "Switch"]);
    expect(within(crewsTable()).getAllByRole("row").slice(1).map((row) => within(row).getByText(/^Template:/).previousSibling?.textContent)).toEqual(["Dependency bump", "Fix failing CI", "Flaky test finder", "Security advisories"]);
    expect(rowOf("Fix failing CI")).toHaveTextContent(/Template: fix failing CIWhen CI fails on main\w{3} \d{1,2} Nov, \d\d:\d\d · 12 turns, 9 minNeeds reviewOn/);
    expect(rowOf("Dependency bump")).toHaveTextContent(/\w{3} \d{1,2} Nov, \d\d:\d\d · running, 6 minRunningOn/);
    expect(rowOf("Flaky test finder")).toHaveTextContent("Every 24 hours");
    expect(rowOf("Security advisories")).toHaveTextContent("Security advisoriesTemplate: security advisoryWhen a dependency update opensNever runIdleOff");
    const stateChip = (crew: string, text: string) => within(rowOf(crew)).getByText(text, { selector: ".ap-chip" });
    expect(stateChip("Fix failing CI", "Needs review")).toHaveClass("ap-chip-am");
    expect(stateChip("Dependency bump", "Running")).toHaveClass("ap-chip-dim");
    expect(stateChip("Flaky test finder", "Idle")).toHaveClass("ap-chip-dim");
    expect(statusTone("Stopped · Budget")).toBe("am");
    expect(within(rowOf("Fix failing CI")).getByRole("switch", { name: "Fix failing CI on" })).toHaveTextContent("On");
    expect(screen.getByText("Per-crew Autopilot is Planned.")).toBeVisible();
    expect(screen.getByRole("heading", { name: "Each run has a budget" })).toBeVisible();
    expect(screen.getByText("A background crew’s budget is set when it is created: a number of turns and a number of minutes for each run. A run that reaches either one is Stopped with the reason Budget.")).toBeVisible();
    expect(screen.queryByText("Every run lands in the Review queue")).toBeNull();
    expect(screen.queryByText("Merged")).toBeNull();
    expect(screen.queryByRole("note")).toBeNull();
    const runs = screen.getByRole("table", { name: "Recent runs" });
    expect(within(runs).getByText("Stopped · Budget")).toBeVisible();
    expect(runs).toHaveTextContent("40 of 40 turns · 31 of 60 min");
    expect(runs).toHaveTextContent("Running · 6 min");
    expect(runs).toHaveTextContent(/Finished \d\d:\d\d · 12 turns, 9 min|Finished \w{3} \d{1,2} Nov · 12 turns, 9 min/);
    expect(runs).not.toHaveTextContent(/Ended|Landed in Review/);
    expect(within(runs).getAllByText("Needs review").length).toBeGreaterThan(0);
    expect(within(runs).queryByText("Failed")).toBeNull();
  });

  it("turns a crew on through the route, and says why when the route refuses", async () => {
    mock.reset("crew-background");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    const switchOf = async () => await screen.findByRole("switch", { name: "Security advisories on" });
    expect(await switchOf()).toHaveAttribute("aria-checked", "false");
    await user.click(await switchOf());
    await waitFor(async () => expect(await switchOf()).toHaveAttribute("aria-checked", "true"));
    mock.fail("/api/crew/background/bg-sec/disable", 1, 500, "internal", "The store is busy.");
    await user.click(await switchOf());
    expect(await screen.findByRole("alert")).toHaveTextContent("The store is busy.");
    expect(await switchOf()).toHaveAttribute("aria-checked", "true");
  });

  it("keeps a write that worked on screen and says so when the read after it fails", async () => {
    mock.reset("crew-background");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("switch", { name: "Security advisories on" }));
    mock.fail("/api/crew/background/bg-ci/runs", 1, 500, "internal", "The store is busy.");
    await user.click(await screen.findByRole("switch", { name: "Fix failing CI on" }));
    expect(await screen.findByText("Can’t refresh background crews")).toBeVisible();
    expect(crewsTable()).toBeVisible();
  });

  it("creates a crew from a template, named for it and off, and lists it", async () => {
    mock.reset("crew-background-empty");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    expect(await screen.findByText("No background crews yet")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Use the Dependency bump template" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    expect(within(dialog).getByRole("radio", { name: "Dependency bump" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: "A dependency update opens" })).toBeChecked();
    expect(within(dialog).getByLabelText("Name")).toHaveValue("Dependency bump");
    expect(dialog).toHaveTextContent("It is created off. Turn it on when you want its trigger to start runs. It is kept on this device.");
    expect(within(dialog).getByLabelText("Turns")).toHaveValue(40);
    expect(within(dialog).getByLabelText("Minutes")).toHaveValue(60);
    expect(within(dialog).getByText("Budget for each run. Turns 1 to 1000, minutes 1 to 1440. It is set now and does not change after.")).toBeVisible();
    // The daily per-engine budget follows the run budget's pattern: a pair with one hint under both.
    expect(within(dialog).getByLabelText("Runs a day")).toHaveValue(2);
    expect(within(dialog).getByLabelText("Turns a day")).toHaveValue(80);
    expect(within(dialog).getByText("Daily budget for each engine. Runs 1 to 1000, turns 1 to 10000. It is set now and does not change after, and it resets at your midnight.")).toBeVisible();
    const order = [...dialog.querySelectorAll("label")].map((one) => one.textContent);
    expect(order.indexOf("Name")).toBeLessThan(order.indexOf("Template"));
    expect(order.indexOf("Template")).toBeLessThan(order.findIndex((text) => text?.startsWith("Starts when")));
    expect(order.indexOf("Turns")).toBeLessThan(order.indexOf("Runs a day"));

    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(rowOf("Dependency bump")).toHaveTextContent("When a dependency update opensNever runIdleOff");
  });

  it("sends the daily per-engine budget and the person's clock with the create body", async () => {
    mock.reset("crew-background-empty");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "Use the Flaky test template" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    const dailyRuns = within(dialog).getByLabelText("Runs a day");
    await user.clear(dailyRuns);
    await user.type(dailyRuns, "4");
    const dailyTurns = within(dialog).getByLabelText("Turns a day");
    await user.clear(dailyTurns);
    await user.type(dailyTurns, "160");
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // The body the route reads: the run budget, the daily budget of the form, and this clock's offset, the way a scheduled task sends it.
    const create = vi.mocked(globalThis.fetch).mock.calls.filter(([url]) => url === "/api/crew/background/create").at(-1);
    expect(create).toBeDefined();
    const sent = JSON.parse(String(create?.[1]?.body)) as { budget: unknown; daily: unknown; utc_offset_minutes: number };
    expect(sent.budget).toEqual({ turns: 40, minutes: 60 });
    expect(sent.daily).toEqual({ runs: 4, turns: 160 });
    expect(sent.utc_offset_minutes).toBe(-new Date().getTimezoneOffset());
  });

  it("sends the form's default daily budget of 2 runs and 80 turns", async () => {
    const { user, dialog } = await openCreate();
    expect(within(dialog).getByLabelText("Runs a day")).toHaveValue(2);
    expect(within(dialog).getByLabelText("Turns a day")).toHaveValue(80);
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const create = vi.mocked(globalThis.fetch).mock.calls.filter(([url]) => url === "/api/crew/background/create").at(-1);
    const sent = JSON.parse(String(create?.[1]?.body)) as { daily: unknown };
    expect(sent.daily).toEqual({ runs: 2, turns: 80 });
  });

  it("shows the range under Runs a day while it is invalid, and restores its shared hint when it is valid", async () => {
    const { dialog } = await openCreate();
    const runs = within(dialog).getByLabelText("Runs a day");
    const create = within(dialog).getByRole("button", { name: "Create background crew" });
    for (const bad of ["", "0", "1001", "1.5", "abc"]) {
      setValue(runs, bad);
      expect(within(dialog).getByText("Use 1 to 1000 runs a day")).toBeVisible();
      expect(create).toBeDisabled();
      expect(runs).toHaveAttribute("aria-invalid", "true");
      expect(runs).toHaveAttribute("aria-describedby", "bg-daily-runs-error");
      expect(document.getElementById("bg-daily-runs-error")).toHaveTextContent("Use 1 to 1000 runs a day");
      expect(runs).toHaveAccessibleDescription("Use 1 to 1000 runs a day");
      expect(within(dialog).getByText(DAILY_HINT)).toBeVisible();
    }
    for (const good of ["1", "1000"]) {
      setValue(runs, good);
      expect(within(dialog).queryByText("Use 1 to 1000 runs a day")).toBeNull();
      expect(create).toBeEnabled();
      expect(runs).not.toHaveAttribute("aria-invalid");
      expect(runs).toHaveAttribute("aria-describedby", "bg-daily-hint");
      expect(runs).toHaveAccessibleDescription(DAILY_HINT);
    }
  });

  it("shows the range under Turns a day while it is invalid, and restores its shared hint when it is valid", async () => {
    const { dialog } = await openCreate();
    const turns = within(dialog).getByLabelText("Turns a day");
    const create = within(dialog).getByRole("button", { name: "Create background crew" });
    for (const bad of ["", "0", "10001", "1.5", "abc"]) {
      setValue(turns, bad);
      expect(within(dialog).getByText("Use 1 to 10000 turns a day")).toBeVisible();
      expect(create).toBeDisabled();
      expect(turns).toHaveAttribute("aria-invalid", "true");
      expect(turns).toHaveAccessibleDescription("Use 1 to 10000 turns a day");
      expect(within(dialog).getByText(DAILY_HINT)).toBeVisible();
    }
    for (const good of ["1", "10000"]) {
      setValue(turns, good);
      expect(within(dialog).queryByText("Use 1 to 10000 turns a day")).toBeNull();
      expect(create).toBeEnabled();
      expect(turns).not.toHaveAttribute("aria-invalid");
      expect(turns).toHaveAccessibleDescription(DAILY_HINT);
    }
  });

  it("shows the run budget ranges under Turns and Minutes", async () => {
    const { dialog } = await openCreate();
    const turns = within(dialog).getByLabelText("Turns");
    const minutes = within(dialog).getByLabelText("Minutes");
    const create = within(dialog).getByRole("button", { name: "Create background crew" });
    for (const bad of ["", "0", "1001", "1.5", "abc"]) {
      setValue(turns, bad);
      expect(within(dialog).getByText("Use 1 to 1000 turns")).toBeVisible();
      expect(create).toBeDisabled();
      expect(turns).toHaveAccessibleDescription("Use 1 to 1000 turns");
      expect(within(dialog).getByText(BUDGET_HINT)).toBeVisible();
    }
    for (const good of ["1", "1000"]) {
      setValue(turns, good);
      expect(within(dialog).queryByText("Use 1 to 1000 turns")).toBeNull();
      expect(turns).toHaveAccessibleDescription(BUDGET_HINT);
    }
    for (const bad of ["", "0", "1441", "1.5", "abc"]) {
      setValue(minutes, bad);
      expect(within(dialog).getByText("Use 1 to 1440 minutes")).toBeVisible();
      expect(create).toBeDisabled();
      expect(minutes).toHaveAccessibleDescription("Use 1 to 1440 minutes");
      expect(within(dialog).getByText(BUDGET_HINT)).toBeVisible();
    }
    for (const good of ["1", "1440"]) {
      setValue(minutes, good);
      expect(within(dialog).queryByText("Use 1 to 1440 minutes")).toBeNull();
      expect(minutes).toHaveAccessibleDescription(BUDGET_HINT);
    }
    expect(create).toBeEnabled();
  });

  it("replaces Every N hours' own hint with its range, and restores it when the value is valid", async () => {
    const { user, dialog } = await openCreate();
    await user.click(within(dialog).getByRole("radio", { name: "On a schedule" }));
    const hours = within(dialog).getByLabelText("Every N hours");
    const create = within(dialog).getByRole("button", { name: "Create background crew" });
    for (const bad of ["", "0", "721", "1.5", "abc"]) {
      setValue(hours, bad);
      expect(within(dialog).getByText("Use 1 to 720 hours")).toBeVisible();
      expect(within(dialog).queryByText("1 to 720.")).toBeNull();
      expect(create).toBeDisabled();
      expect(hours).toHaveAttribute("aria-invalid", "true");
      expect(hours).toHaveAccessibleDescription("Use 1 to 720 hours");
    }
    for (const good of ["1", "720"]) {
      setValue(hours, good);
      expect(within(dialog).queryByText("Use 1 to 720 hours")).toBeNull();
      expect(within(dialog).getByText("1 to 720.")).toBeVisible();
      expect(create).toBeEnabled();
      expect(hours).not.toHaveAttribute("aria-invalid");
      expect(hours).toHaveAccessibleDescription("1 to 720.");
    }
  });

  it("replaces Name and Label hints with their ranges while either is empty or too long", async () => {
    const { user, dialog } = await openCreate();
    const name = within(dialog).getByLabelText("Name");
    const create = within(dialog).getByRole("button", { name: "Create background crew" });
    setValue(name, "");
    expect(within(dialog).getByText("Use 1 to 80 characters")).toBeVisible();
    expect(create).toBeDisabled();
    expect(name).toHaveAccessibleDescription("Use 1 to 80 characters");
    setValue(name, "a");
    expect(within(dialog).queryByText("Use 1 to 80 characters")).toBeNull();
    expect(create).toBeEnabled();
    expect(name).toHaveAccessibleDescription("1 to 80 characters");
    setValue(name, "a".repeat(80));
    expect(create).toBeEnabled();
    setValue(name, "a".repeat(81));
    expect(within(dialog).getByText("Use 1 to 80 characters")).toBeVisible();
    expect(create).toBeDisabled();
    setValue(name, "Fix failing CI");

    await user.click(within(dialog).getByRole("radio", { name: "An issue gets a label" }));
    const label = within(dialog).getByLabelText("Label");
    expect(within(dialog).getByText("Use 1 to 100 characters")).toBeVisible();
    expect(create).toBeDisabled();
    expect(label).toHaveAccessibleDescription("Use 1 to 100 characters");
    setValue(label, "crew-fix");
    expect(within(dialog).queryByText("Use 1 to 100 characters")).toBeNull();
    expect(label).toHaveAccessibleDescription("1 to 100 characters. An issue that gets this label starts a run.");
    setValue(label, "a".repeat(100));
    expect(create).toBeEnabled();
    setValue(label, "a".repeat(101));
    expect(within(dialog).getByText("Use 1 to 100 characters")).toBeVisible();
    expect(create).toBeDisabled();
  });

  it.each([
    ["Runs a day", "the daily budget is 1 to 1000 runs"],
    ["Turns a day", "the daily budget is 1 to 10000 turns"],
  ])("shows the service's refusal under %s and drops it when the field is edited", async (field, message) => {
    const { user, dialog } = await openCreate();
    mock.fail("/api/crew/background/create", 1, 400, "invalid", message);
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    const shown = await within(dialog).findByText(message);
    expect(shown).toBeVisible();
    expect(within(dialog).getByLabelText(field)).toHaveAttribute("aria-invalid", "true");
    expect(within(dialog).getByLabelText(field)).toHaveAccessibleDescription(message);
    expect(within(dialog).getByText(DAILY_HINT)).toBeVisible();
    await user.type(within(dialog).getByLabelText(field), "1");
    expect(within(dialog).queryByText(message)).toBeNull();
    expect(within(dialog).getByLabelText(field)).toHaveAccessibleDescription(DAILY_HINT);
    expect(within(dialog).getByRole("button", { name: "Create background crew" })).toBeEnabled();
  });

  it("keeps the entered daily budget and the dialog open when the service refuses it as over the cap", async () => {
    const { user, dialog } = await openCreate();
    const runs = within(dialog).getByLabelText("Runs a day");
    const turns = within(dialog).getByLabelText("Turns a day");
    setValue(runs, "3");
    setValue(turns, "120");
    mock.fail("/api/crew/background/create", 1, 409, "budget_exceeded", "This crew would exceed the daily cap for its engine.");

    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    expect(await within(dialog).findByText("This crew would exceed the daily cap for its engine.")).toBeVisible();
    expect(runs).toHaveValue(3);
    expect(turns).toHaveValue(120);
    expect(screen.getByRole("dialog", { name: /New background crew/ })).toBe(dialog);
    expect(screen.queryByRole("table", { name: "Background crews" })).toBeNull();
    expect(screen.getByText("No background crews yet")).toBeVisible();
    expect(within(dialog).getByText(DAILY_HINT)).toBeVisible();
    const posts = vi.mocked(fetch).mock.calls.filter(([url, init]) => String(url) === "/api/crew/background/create" && init?.method === "POST");
    expect(posts).toHaveLength(1);
    expect(JSON.parse(String(posts[0]![1]?.body))).toMatchObject({ daily: { runs: 3, turns: 120 } });
  });

  it("keeps Create on when the optional CI workflow and Branch are empty", async () => {
    const { user, dialog } = await openCreate();
    const workflow = within(dialog).getByLabelText("CI workflow · optional");
    const branch = within(dialog).getByLabelText("Branch · optional");
    const create = within(dialog).getByRole("button", { name: "Create background crew" });
    expect(create).toBeEnabled();
    await user.type(workflow, "build.yml");
    await user.clear(workflow);
    await user.type(branch, "release");
    await user.clear(branch);
    expect(create).toBeEnabled();
    expect(within(dialog).queryByRole("alert")).toBeNull();
  });

  it("opens the Flaky test template on its schedule, and creates it", async () => {
    mock.reset("crew-background-empty");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "Use the Flaky test template" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    expect(within(dialog).getByRole("radio", { name: "On a schedule" })).toBeChecked();
    const hours = within(dialog).getByLabelText("Every N hours");
    expect(hours).toHaveValue(24);
    expect(hours).toHaveAccessibleDescription("1 to 720.");
    await user.clear(hours);
    await user.type(hours, "6");
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(rowOf("Flaky test")).toHaveTextContent("Every 6 hours");
  });

  it("creates an issue-label crew, and keeps Create off until the label is set", async () => {
    mock.reset("crew-background-empty");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "Use the Fix failing CI template" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    await user.click(within(dialog).getByRole("radio", { name: "An issue gets a label" }));
    expect(within(dialog).getByRole("button", { name: "Create background crew" })).toBeDisabled();
    const label = within(dialog).getByLabelText("Label");
    expect(label).toHaveAccessibleDescription("Use 1 to 100 characters");
    await user.type(label, "crew-fix");
    expect(label).toHaveAccessibleDescription("1 to 100 characters. An issue that gets this label starts a run.");
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(rowOf("Fix failing CI")).toHaveTextContent("When an issue gets the label crew-fix");
  });

  it("shows the route's refusal under the name field and keeps the dialog open", async () => {
    mock.reset("crew-background");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "New background crew" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    expect(within(dialog).getByLabelText("CI workflow · optional")).toHaveAttribute("placeholder", "ci.yml");
    expect(within(dialog).getByLabelText("Branch · optional")).toHaveAttribute("placeholder", "main");
    await user.type(within(dialog).getByLabelText("CI workflow · optional"), "build.yml");
    await user.type(within(dialog).getByLabelText("Branch · optional"), "release");
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    expect(await within(dialog).findByText("this project already has a background crew with that name")).toBeVisible();
    expect(within(dialog).getByLabelText("Name")).toHaveAttribute("aria-invalid", "true");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("links each input to its hint, and to the route's refusal while there is one, and drops the refusal when the field is edited", async () => {
    mock.reset("crew-background");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "New background crew" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    const name = within(dialog).getByLabelText("Name");
    expect(name).toHaveAccessibleDescription("1 to 80 characters");
    expect(within(dialog).getByLabelText("Turns")).toHaveAccessibleDescription("Budget for each run. Turns 1 to 1000, minutes 1 to 1440. It is set now and does not change after.");
    await user.clear(name);
    await user.type(name, "Fix failing CI");
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    expect(await within(dialog).findByText("this project already has a background crew with that name")).toBeVisible();
    expect(name).toHaveAccessibleDescription("this project already has a background crew with that name");
    await user.type(name, "!");
    expect(within(dialog).queryByText("this project already has a background crew with that name")).toBeNull();
    expect(name).not.toHaveAttribute("aria-invalid", "true");
    expect(name).toHaveAccessibleDescription("1 to 80 characters");
  });

  it("keeps Create off while a budget is outside the route's limits", async () => {
    mock.reset("crew-background");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "New background crew" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    const turns = within(dialog).getByLabelText("Turns");
    await user.clear(turns);
    await user.type(turns, "1001");
    expect(within(dialog).getByRole("button", { name: "Create background crew" })).toBeDisabled();
    expect(within(dialog).getByText(/Turns 1 to 1000, minutes 1 to 1440/)).toBeVisible();
    // The daily pair carries the same rule for each engine's day.
    const dailyTurns = within(dialog).getByLabelText("Turns a day");
    await user.clear(turns);
    await user.type(turns, "40");
    await user.clear(dailyTurns);
    await user.type(dailyTurns, "10001");
    expect(within(dialog).getByRole("button", { name: "Create background crew" })).toBeDisabled();
    expect(within(dialog).getByText(/Daily budget for each engine/)).toBeVisible();
  });

  it("lists nothing without a git project, and says why Create is off", async () => {
    mock.reset("crew-background");
    useWorkspace.setState({ projects: { data: [], loading: false, error: null } });
    render(<BackgroundTab />);
    expect(await screen.findByText("No git project")).toBeVisible();
    expect(screen.queryByRole("table", { name: "Background crews" })).toBeNull();
    expect(screen.getByRole("button", { name: "Use the Flaky test template" })).toBeDisabled();
  });

  it("says it can't read the crews, and reads again on Retry", async () => {
    mock.reset("crew-background");
    mock.fail("/api/crew/background", 1, 500, "internal", "The store is busy.");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Can’t read background crews");
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("table", { name: "Background crews" })).toBeVisible();
  });
});

describe("The watch kind in the create dialog (KC6)", () => {
  /** The form opened from the header's button, with the default template and name it fills in. */
  async function openBlank(scenario: "crew-background" | "crew-background-empty" = "crew-background") {
    mock.reset(scenario);
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "New background crew" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    await user.click(within(dialog).getByRole("radio", { name: "Watch" }));
    return { user, dialog };
  }
  const createButton = (dialog: HTMLElement) => within(dialog).getByRole("button", { name: "Create background crew" });

  it("offers Watch with its command, schedule and failing action, and asks for no model budget when it only raises Needs you", async () => {
    const { dialog } = await openBlank();
    expect(within(dialog).getByLabelText("Command")).toHaveValue("");
    // An empty command is out of range, so the field carries the range sentence rather than its hint.
    expect(within(dialog).getByLabelText("Command")).toHaveAccessibleDescription("Use 1 to 500 characters");
    expect(within(dialog).getByLabelText("Every N hours")).toHaveValue(24);
    expect(within(dialog).getByLabelText("Every N hours")).toHaveAccessibleDescription("1 to 720. The shortest is every 1 hour, so at most 24 checks a day.");
    expect(within(dialog).getByRole("radio", { name: "Raise Needs you" })).toBeChecked();
    expect(within(dialog).getByText("One row in Needs you each time the result turns failing. Nothing starts.")).toBeVisible();
    // A watch that only raises a Needs you row starts no run: no template and no budget are asked.
    expect(within(dialog).queryByRole("radio", { name: "Fix failing CI" })).toBeNull();
    expect(within(dialog).queryByLabelText("Turns")).toBeNull();
    expect(within(dialog).queryByLabelText("Runs a day")).toBeNull();
    expect(within(dialog).getByRole("region", { name: "No model call, no model budget" })).toHaveTextContent("A check is a command, not a model call, so it spends no turns and none of an engine’s daily budget.");
    expect(dialog).toHaveTextContent("It is created off. Turn it on when you want it to start checking. It is kept on this device. A project can have at most 10 watches.");
    expect(createButton(dialog)).toBeDisabled();
    // The designer's ruling: the label no longer carries the Not built yet chip.
    expect(within(dialog).queryByText("Not built yet")).toBeNull();
  });

  it("shows the command and schedule ranges, and keeps Create off until both are in range", async () => {
    const { user, dialog } = await openBlank();
    const command = within(dialog).getByLabelText("Command");
    const hours = within(dialog).getByLabelText("Every N hours");
    await user.type(within(dialog).getByLabelText("Name"), "!");
    fireEvent.change(command, { target: { value: "" } });
    expect(within(dialog).getByText("Use 1 to 500 characters")).toBeVisible();
    expect(command).toHaveAttribute("aria-invalid", "true");
    expect(createButton(dialog)).toBeDisabled();
    fireEvent.change(command, { target: { value: "a".repeat(501) } });
    expect(within(dialog).getByText("Use 1 to 500 characters")).toBeVisible();
    fireEvent.change(command, { target: { value: "a".repeat(500) } });
    expect(within(dialog).queryByText("Use 1 to 500 characters")).toBeNull();
    expect(within(dialog).getByLabelText("Name")).toHaveValue("Fix failing CI!");
    // The name is still the template's, and the command is in range: the hours are what is left.
    fireEvent.change(hours, { target: { value: "0" } });
    expect(within(dialog).getByText("Use 1 to 720 hours")).toBeVisible();
    expect(hours).toHaveAttribute("aria-invalid", "true");
    expect(createButton(dialog)).toBeDisabled();
    fireEvent.change(hours, { target: { value: "1" } });
    expect(within(dialog).queryByText("Use 1 to 720 hours")).toBeNull();
    // An empty name is still off, as for every kind.
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "" } });
    expect(within(dialog).getByText("Use 1 to 80 characters")).toBeVisible();
    expect(createButton(dialog)).toBeDisabled();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Redirect tests" } });
    expect(createButton(dialog)).toBeEnabled();
  });

  it("asks for the template and its budgets when a failing result should start the crew's run", async () => {
    const { user, dialog } = await openBlank();
    await user.click(within(dialog).getByRole("radio", { name: "Start the crew template" }));
    expect(within(dialog).getByRole("radio", { name: "Fix failing CI" })).toBeChecked();
    expect(within(dialog).getByLabelText("Turns")).toHaveValue(40);
    expect(within(dialog).getByLabelText("Minutes")).toHaveValue(60);
    expect(within(dialog).getByLabelText("Runs a day")).toHaveValue(2);
    expect(within(dialog).getByLabelText("Turns a day")).toHaveValue(80);
    expect(within(dialog).getByText("Starts one run of the template each time the result turns failing, not while it stays failing.")).toBeVisible();
    // The watch's template and budgets come after the failing choice, as watch-setup-template draws them.
    const order = [...dialog.querySelectorAll("label")].map((one) => one.textContent);
    expect(order.indexOf("Every N hours")).toBeLessThan(order.indexOf("When it turns failing"));
    expect(order.indexOf("When it turns failing")).toBeLessThan(order.indexOf("Template"));
    expect(order.indexOf("Template")).toBeLessThan(order.indexOf("Turns"));
    expect(within(dialog).getByText(/^Budget for each run the watch starts\./)).toBeVisible();
    // Choosing the template a failing result starts must not turn the watch into that template's own trigger kind.
    await user.click(within(dialog).getByRole("radio", { name: "Dependency bump" }));
    expect(within(dialog).getByRole("radio", { name: "Watch" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: "Dependency bump" })).toBeChecked();
    expect(within(dialog).getByRole("region", { name: "The check has no model budget" })).toHaveTextContent("Only a run it starts spends the budget above.");
  });

  it("creates a watch that raises Needs you, with the defaults its body has to carry, and lists it not checked yet", async () => {
    mock.reset("crew-background");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "New background crew" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    await user.click(within(dialog).getByRole("radio", { name: "Watch" }));
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Redirect tests" } });
    fireEvent.change(within(dialog).getByLabelText("Command"), { target: { value: "npm test web/nav/redirect" } });
    fireEvent.change(within(dialog).getByLabelText("Every N hours"), { target: { value: "1" } });
    await user.click(createButton(dialog));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const row = rowOf("Redirect tests");
    expect(row).toHaveTextContent("Watch · no model · npm test web/nav/redirect");
    expect(row).toHaveTextContent("Every 1 hour · raises Needs you");
    expect(row).toHaveTextContent("Not checked yet");
    expect(within(row).getByRole("switch", { name: "Redirect tests on" })).toHaveTextContent("Off");
    const create = vi.mocked(globalThis.fetch).mock.calls.filter(([url]) => url === "/api/crew/background/create").at(-1);
    const sent = JSON.parse(String(create?.[1]?.body)) as { template: string; trigger: unknown; budget: unknown; daily: unknown };
    expect(sent.template).toBe("fix_failing_ci");
    expect(sent.trigger).toEqual({ kind: "watch", command: "npm test web/nav/redirect", every_hours: 1, on_failing: "needs_you" });
    expect(sent.budget).toEqual({ turns: 40, minutes: 60 });
    expect(sent.daily).toEqual({ runs: 2, turns: 80 });
  });

  it("shows the service's refusal of a denied command under Command, and drops it when the command is edited", async () => {
    const { user, dialog } = await openBlank();
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Redirect tests" } });
    fireEvent.change(within(dialog).getByLabelText("Command"), { target: { value: "git push --force origin main" } });
    fireEvent.change(within(dialog).getByLabelText("Every N hours"), { target: { value: "1" } });
    await user.click(createButton(dialog));
    const sentence = "That command is not allowed: force-push, history rewrite or any push to main.";
    expect(await within(dialog).findByText(sentence)).toBeVisible();
    expect(within(dialog).getByLabelText("Command")).toHaveAttribute("aria-invalid", "true");
    expect(within(dialog).getByLabelText("Command")).toHaveAccessibleDescription(sentence);
    fireEvent.change(within(dialog).getByLabelText("Command"), { target: { value: "npm test web/nav/redirect" } });
    expect(within(dialog).queryByText(sentence)).toBeNull();
    expect(createButton(dialog)).toBeEnabled();
  });

  it("sends the template and the budgets a watch that starts a run was given", async () => {
    const { user, dialog } = await openBlank();
    await user.click(within(dialog).getByRole("radio", { name: "Start the crew template" }));
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "Main build" } });
    fireEvent.change(within(dialog).getByLabelText("Command"), { target: { value: "npm run build" } });
    fireEvent.change(within(dialog).getByLabelText("Every N hours"), { target: { value: "6" } });
    await user.click(createButton(dialog));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const create = vi.mocked(globalThis.fetch).mock.calls.filter(([url]) => url === "/api/crew/background/create").at(-1);
    const sent = JSON.parse(String(create?.[1]?.body)) as { trigger: unknown; budget: unknown };
    expect(sent.trigger).toEqual({ kind: "watch", command: "npm run build", every_hours: 6, on_failing: "template" });
    expect(sent.budget).toEqual({ turns: 40, minutes: 60 });
  });
});

describe("A background start the disk guard refused (KC4, frame kc4-low-disk-background-run)", () => {
  const recentRuns = () => screen.getByRole("table", { name: "Recent runs" });
  const newestRun = () => within(recentRuns()).getAllByRole("row")[1] as HTMLElement;

  it("draws the refused start as Not started · Disk with the service's sentence, and no spend", async () => {
    mock.reset("crew-background-disk-low");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Recent runs" });
    const row = newestRun();
    expect(row).toHaveTextContent("Fix failing CITemplate: fix failing CICI failed on main");
    expect(within(row).getByText("Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again.")).toBeVisible();
    expect(within(row).getByText("No worker ran. Nothing was removed to make room.")).toBeVisible();
    const chip = within(row).getByText("Not started · Disk", { selector: ".ap-chip" });
    expect(chip).toHaveClass("ap-chip-am");
    expect(row).not.toHaveTextContent(/Stopped|Failed|turns|min\b/);
    // The run before it keeps drawing as it did.
    expect(within(recentRuns()).getByText("Stopped · Budget")).toBeVisible();
  });

  it("says Not started · Disk in the crew's last-run cell, not 0 turns, 0 min", async () => {
    mock.reset("crew-background-disk-low");
    render(<BackgroundTab />);
    const crews = await screen.findByRole("table", { name: "Background crews" });
    const row = within(crews).getAllByRole("row").find((one) => one.textContent?.includes("Fix failing CI")) as HTMLElement;
    expect(row).toHaveTextContent("Not started · Disk");
    expect(row).not.toHaveTextContent(/0 turns/);
  });

  it("draws the design's own sentence when the service sent none, above the muted line", async () => {
    mock.reset("crew-background-disk-low-bare");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Recent runs" });
    const row = newestRun();
    expect(within(row).getByText("Not started: Crew needs more free disk space. Free space and try again.")).toBeVisible();
    expect(within(row).getByText("No worker ran. Nothing was removed to make room.")).toBeVisible();
  });

  it("keeps a stopped run with a reason code it does not know drawn as a stopped run", async () => {
    mock.reset("crew-background-reason-unknown");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Recent runs" });
    const row = newestRun();
    expect(within(row).getByText("Stopped", { selector: ".ap-chip" })).toBeVisible();
    expect(row).toHaveTextContent("0 of 40 turns · 0 of 60 min");
    expect(row).not.toHaveTextContent(/Not started|Disk|No worker ran/);
  });

  it("keeps a run whose reason code is an inherited property name drawn as a stopped run", async () => {
    mock.reset("crew-background-reason-inherited");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Recent runs" });
    const row = newestRun();
    expect(within(row).getByText("Stopped", { selector: ".ap-chip" })).toBeVisible();
    expect(row).not.toHaveTextContent(/Not started|function|No worker ran/);
  });

  it("draws nothing of it until a run carries the code", async () => {
    mock.reset("crew-background");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Recent runs" });
    expect(recentRuns()).not.toHaveTextContent(/Not started|No worker ran/);
  });
});

describe("Background crew words", () => {
  const base = { template: "fix_failing_ci" as const };
  it("says each trigger the way the designs do", () => {
    expect(startsWhen({ ...base, trigger: { kind: "ci_failure", branch: "main" } })).toBe("When CI fails on main");
    expect(startsWhen({ ...base, trigger: { kind: "dependency_update" } })).toBe("When a dependency update opens");
    expect(startsWhen({ trigger: { kind: "dependency_update" } })).toBe("When a dependency update opens");
    expect(startsWhen({ ...base, trigger: { kind: "schedule", every_hours: 24 } })).toBe("Every 24 hours");
  });

  it("maps the route's refusals to the field they are about", () => {
    expect(fieldOf("this project already has a background crew with that name")).toBe("name");
    expect(fieldOf("name is 1 to 80 characters")).toBe("name");
    expect(fieldOf("name has a hidden character")).toBe("name");
    expect(fieldOf("a issue_label trigger needs label")).toBe("label");
    expect(fieldOf("label is 1 to 100 characters")).toBe("label");
    expect(fieldOf("every_hours is 1 to 720")).toBe("hours");
    expect(fieldOf("the budget is 1 to 1000 turns")).toBe("turns");
    expect(fieldOf("the budget is 1 to 1440 minutes")).toBe("minutes");
    expect(fieldOf("the daily budget is 1 to 1000 runs")).toBe("dailyRuns");
    expect(fieldOf("the daily budget is 1 to 10000 turns")).toBe("dailyTurns");
    expect(fieldOf("something else")).toBeNull();
  });

  it("lists what each trigger is set to across the crews", () => {
    const crew = (id: string, branch: string) => ({ id, project_id: "p", name: id, template: "fix_failing_ci" as const, trigger: { kind: "ci_failure" as const, workflow: "ci.yml", branch }, budget: { turns: 1, minutes: 1 }, daily: { runs: 2, turns: 80 }, enabled: true, state: "idle" as const, reviews_waiting: 0, created_at: "", updated_at: "" });
    const view = backgroundView({ crews: [crew("a", "main"), crew("b", "main"), crew("c", "dev")], total: 3, runs: {} }, new Date());
    expect(view.configured).toEqual({ ci_fails: "ci.yml on main, ci.yml on dev" });
  });

  it("says a dependency update trigger is configured as the bots' PRs, with the ecosystem when it has one", () => {
    expect(configuredAs({ kind: "dependency_update" })).toBe("Dependabot or Renovate PRs");
    expect(configuredAs({ kind: "dependency_update", ecosystem: "npm" })).toBe("Dependabot or Renovate PRs · npm");
  });
});
