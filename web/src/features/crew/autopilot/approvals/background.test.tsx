import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../generated/Project";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { BackgroundTab } from "./Background";
import { ciTrigger } from "./BackgroundCreate";
import { backgroundView, startsWhen } from "./backgroundModel";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project], loading: false, error: null } });
});

const crewsTable = () => screen.getByRole("table", { name: "Background crews" });
const rowOf = (name: string) => within(crewsTable()).getByText(name, { selector: "b" }).closest('[role="row"]') as HTMLElement;

describe("Background crews on the Crew routes", () => {
  it("lists the crews the route holds with their trigger and newest run, and no Autopilot column it cannot fill", async () => {
    mock.reset("crew-background");
    render(<BackgroundTab />);
    await screen.findByRole("table", { name: "Background crews" });
    expect(within(crewsTable()).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Crew", "Starts when", "Last run", "State"]);
    expect(rowOf("Fix failing CI")).toHaveTextContent(/Template: fix failing CIWhen CI fails on main\w{3} \d{1,2} Nov, \d\d:\d\d · landed in ReviewOn/);
    expect(rowOf("Security advisories")).toHaveTextContent("Security advisoriesTemplate: security advisoryWhen a security advisory is publishedNever runOff");
    expect(screen.queryByRole("note")).toBeNull();
    const runs = screen.getByRole("table", { name: "Recent runs" });
    expect(runs).toHaveTextContent(/Stopped · budget/);
    expect(within(runs).queryByText("Failed")).toBeNull();
  });

  it("turns a crew on through the route, and says why when the route refuses", async () => {
    mock.reset("crew-background");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    const switchOf = async () => await screen.findByRole("switch", { name: "Security advisories state" });
    expect(await switchOf()).toHaveAttribute("aria-checked", "false");
    await user.click(await switchOf());
    await waitFor(async () => expect(await switchOf()).toHaveAttribute("aria-checked", "true"));
    mock.fail("/api/crew/background/bg-sec/disable", 1, 409, "conflict", "A run is in progress.");
    await user.click(await switchOf());
    expect(await screen.findByRole("alert")).toHaveTextContent("A run is in progress.");
    expect(await switchOf()).toHaveAttribute("aria-checked", "true");
  });

  it("creates a crew from a template, off, and lists it", async () => {
    mock.reset("crew-background-empty");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    expect(await screen.findByText("No background crews yet")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Use the Dependency bump template" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    expect(within(dialog).getByRole("radio", { name: "Dependency bump" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: "A dependency update opens" })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(rowOf("Dependency bump")).toHaveTextContent("When a dependency update opensNever runOff");
  });

  it("sends the CI workflow and branch, and keeps the two triggers whose fields are not drawn out of reach", async () => {
    mock.reset("crew-background");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "New background crew" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    expect(within(dialog).getByRole("radio", { name: "An issue gets a label" })).toBeDisabled();
    expect(within(dialog).getByRole("radio", { name: "On a schedule" })).toBeDisabled();
    await user.type(within(dialog).getByLabelText("CI workflow and branch"), "build.yml on release");
    // The name is the template's; the project already has a crew of that name.
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("already exists");
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("opens the Flaky test template with Create held back until its schedule can be set", async () => {
    mock.reset("crew-background-empty");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "Use the Flaky test template" }));
    const dialog = await screen.findByRole("dialog", { name: /New background crew/ });
    expect(within(dialog).getByRole("button", { name: "Create background crew" })).toBeDisabled();
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
    expect(startsWhen({ template: "security_advisory", trigger: { kind: "dependency_update" } })).toBe("When a security advisory is published");
    expect(startsWhen({ ...base, trigger: { kind: "schedule", every_hours: 24 } })).toBe("Every 24 hours");
  });

  it("reads the CI field as workflow and branch, and lists what each trigger is set to", () => {
    expect(ciTrigger("ci.yml on main")).toEqual({ kind: "ci_failure", workflow: "ci.yml", branch: "main" });
    expect(ciTrigger("ci.yml")).toEqual({ kind: "ci_failure", workflow: "ci.yml", branch: null });
    expect(ciTrigger("")).toEqual({ kind: "ci_failure", workflow: null, branch: null });
    const crew = (id: string, branch: string) => ({ id, project_id: "p", name: id, template: "fix_failing_ci" as const, trigger: { kind: "ci_failure" as const, workflow: "ci.yml", branch }, budget: { turns: 1, minutes: 1 }, enabled: true, state: "idle" as const, reviews_waiting: 0, created_at: "", updated_at: "" });
    const view = backgroundView({ crews: [crew("a", "main"), crew("b", "main"), crew("c", "dev")], runs: {} }, new Date());
    expect(view.configured).toEqual({ ci_fails: "ci.yml on main, ci.yml on dev" });
  });
});
