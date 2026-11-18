import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
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

describe("Background crews on the Crew routes", () => {
  it("lists the repo's crews by name with trigger, state and newest run, and says per-crew Autopilot is Planned", async () => {
    mock.reset("crew-background");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Background crews" });
    expect(screen.getByText("orbyn-engineering")).toBeVisible();
    expect(screen.queryByText(/^Repo:/)).toBeNull();
    expect(within(crewsTable()).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Crew", "Starts when", "Last run", "State", "Switch"]);
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
    const order = [...dialog.querySelectorAll("label")].map((one) => one.textContent);
    expect(order.indexOf("Name")).toBeLessThan(order.indexOf("Template"));
    expect(order.indexOf("Template")).toBeLessThan(order.findIndex((text) => text?.startsWith("Starts when")));

    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(rowOf("Dependency bump")).toHaveTextContent("When a dependency update opensNever runIdleOff");
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
    expect(label).toHaveAccessibleDescription("1 to 100 characters. An issue that gets this label starts a run.");
    await user.type(label, "crew-fix");
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
    expect(fieldOf("something else")).toBeNull();
  });

  it("lists what each trigger is set to across the crews", () => {
    const crew = (id: string, branch: string) => ({ id, project_id: "p", name: id, template: "fix_failing_ci" as const, trigger: { kind: "ci_failure" as const, workflow: "ci.yml", branch }, budget: { turns: 1, minutes: 1 }, enabled: true, state: "idle" as const, reviews_waiting: 0, created_at: "", updated_at: "" });
    const view = backgroundView({ crews: [crew("a", "main"), crew("b", "main"), crew("c", "dev")], total: 3, runs: {} }, new Date());
    expect(view.configured).toEqual({ ci_fails: "ci.yml on main, ci.yml on dev" });
  });

  it("says a dependency update trigger is configured as the bots' PRs, with the ecosystem when it has one", () => {
    expect(configuredAs({ kind: "dependency_update" })).toBe("Dependabot or Renovate PRs");
    expect(configuredAs({ kind: "dependency_update", ecosystem: "npm" })).toBe("Dependabot or Renovate PRs · npm");
  });
});
