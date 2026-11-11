import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import { installMockFetch } from "../../../../test-support";
import { SettingsPage } from "../../../settings/SettingsPage";
import { AutopilotSettings } from "./AutopilotSettings";
import { BackgroundCards, BackgroundTab } from "./Background";
import { CrewSurface } from "./CrewSurface";
import { CrewSettingsSection } from "./CrewSettings";
import { FirstRunBanner } from "./FirstRunBanner";
import { StoppedBudgetItem } from "./StoppedItem";
import { TrustTab } from "./Trust";
import { crewApi } from "./api";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

type Scenario = "crew" | `crew-${string}`;
const on = (scenario: Scenario) => mock.reset(scenario);

describe("Crew Preview off", () => {
  it("draws nothing of Crew: no Settings category, no screen", async () => {
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("link", { name: "Crew" })).toBeNull();
  });

  it("renders no Crew screen while the service has no Crew routes", async () => {
    const { container } = render(<CrewSurface tab="approvals" />);
    await waitFor(() => expect(mock.scenario).toBe("default"));
    expect(container).toBeEmptyDOMElement();
  });

  it("lists Crew in Settings, and its screen, while Preview is on", async () => {
    on("crew");
    render(<SettingsPage section="crew" onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "Crew" })).toHaveAttribute("aria-current", "page");
    expect(await screen.findByRole("heading", { name: "Autopilot" })).toBeVisible();
  });

  it("shows its tabs and moves between them with the arrow keys", async () => {
    on("crew");
    const user = userEvent.setup();
    render(<CrewSurface tab="approvals" />);
    const tabs = await screen.findByRole("tablist", { name: "Crew" });
    expect(within(tabs).getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Approvals", "Trust", "Background"]);
    within(tabs).getByRole("tab", { name: "Approvals" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(window.location.hash).toBe("#/crew/trust");
  });
});

describe("Settings › Crew › Autopilot", () => {
  const radio = (name: RegExp) => screen.getByRole("radio", { name });

  it("shows the three settings, Full selected and tagged Default, with the trip wires", async () => {
    on("crew");
    render(<AutopilotSettings />);
    expect(await screen.findByRole("radiogroup", { name: "Autopilot" })).toBeVisible();
    expect(radio(/^Full\s*Default/)).toBeChecked();
    expect(radio(/^Off/)).toHaveTextContent("Crew proves the work and stops. Agents’ requests come to you, and every change waits for you before it merges.");
    expect(radio(/^On/)).toHaveTextContent("Low and Medium risk merge by themselves; High risk waits for you before merging.");
    const wires = screen.getByRole("region", { name: "Trip wires that stop a task" });
    expect(wires).toHaveTextContent("3 denials in a row stop the task with the reason “too many denials”.");
    expect(wires).toHaveTextContent("20 denials in one run stop it with the same reason.");
    expect(wires).toHaveTextContent("possible injected instructions");
    expect(wires).toHaveTextContent("Stopping is amber, not red.");
  });

  it("lists what is always denied, read-only, and leaves out the row that is not enforced yet", async () => {
    on("crew");
    render(<AutopilotSettings />);
    const list = await screen.findByRole("region", { name: "Always denied, in every mode" });
    const items = within(list).getAllByRole("listitem").map((item) => item.textContent);
    expect(items).toEqual([
      "Network to hosts the repo hasn’t allowed",
      "Writing or deleting outside the worktree",
      "Force-push or history rewrite on main",
      "Changing branch protection, permissions or settings",
      "Editing answer keys, check files or .orbyn/crew.json",
      "Production deploys",
      "Publishing or messaging outside the repo",
      "Changing account, device or sign-in state",
      "Payments",
    ]);
    expect(list).toHaveTextContent("Read-only: a person can’t switch one off here");
    expect(within(list).queryByRole("switch")).toBeNull();
    expect(within(list).queryByRole("checkbox")).toBeNull();
  });

  it("switches to On with a click and says what High risk means", async () => {
    on("crew");
    const user = userEvent.setup();
    render(<AutopilotSettings />);
    await user.click(await screen.findByRole("radio", { name: /^On/ }));
    expect(radio(/^On/)).toBeChecked();
    expect(screen.getByText("What High risk means here")).toBeVisible();
    expect(screen.getByText(/Auth, keys, crypto, migrations, CI, dependency changes, large diffs/)).toBeVisible();
    expect(screen.queryByRole("region", { name: "Trip wires that stop a task" })).toBeNull();
    expect((await crewApi.settings()).mode).toBe("on");
  });

  it("moves the choice with the arrow keys, and Off says agents will ask you", async () => {
    on("crew");
    const user = userEvent.setup();
    render(<AutopilotSettings />);
    (await screen.findByRole("radio", { name: /^Full/ })).focus();
    await user.keyboard("{ArrowRight}");
    expect(radio(/^Off/)).toBeChecked();
    expect(screen.getByText("Off: agents will ask you")).toBeVisible();
    expect(screen.getByText(/Every request an agent makes, and every merge, waits for you/)).toBeVisible();
    await waitFor(() => expect(radio(/^Off/)).toHaveFocus());
  });

  it("tells which crews differ from the repo and why, and folds the list away", async () => {
    on("crew-per-crew");
    render(<AutopilotSettings />);
    const table = await screen.findByRole("table", { name: "Autopilot per crew" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Crew", "Autopilot", "Why"]);
    expect(table).toHaveTextContent("Login redirect crewLead: Fix login redirectOnYou set this crew to On on Sat 7 Nov.");
    expect(table).toHaveTextContent("Dependency bump (background).orbyn/crew.jsonFullFollows the repo.");
    expect(screen.getByText("9 actions")).toBeVisible();
    expect(screen.getByText("Show the list")).toBeVisible();
  });

  it("says Full is the default on a new repo, with Change and Keep Full", async () => {
    on("crew-first-run");
    const user = userEvent.setup();
    render(<FirstRunBanner />);
    const banner = await screen.findByRole("region", { name: "Autopilot" });
    expect(banner).toHaveTextContent("Autopilot is Full for orbyn-engineering. The lead answers agents’ requests, verified work merges by itself, and you review what reached main in the Review tab.");
    expect(within(banner).getByRole("button", { name: "Change" })).toBeVisible();
    await user.click(within(banner).getByRole("button", { name: "Keep Full" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Autopilot" })).toBeNull());
  });

  it("describes the production-linked targets the guard watches, read-only", async () => {
    on("crew");
    render(<CrewSettingsSection />);
    expect(await screen.findByRole("heading", { name: "Production-linked targets" })).toBeVisible();
    expect(screen.getByText("Always on, in every mode")).toBeVisible();
    const guarded = screen.getByRole("table", { name: "Always guarded" });
    expect(guarded).toHaveTextContent("prod-db-1Remote database$PROD_DATABASE_URL profile prodprod_targets.orbyn/crew.json");
    expect(screen.getByRole("table", { name: "Exempt (safe)" })).toHaveTextContent("staging-dbA throwaway staging databasestaging-db.internalsafe_targets");
    expect(screen.getByRole("table", { name: "What counts as destructive" })).toHaveTextContent("DataDROP, TRUNCATE, DELETE without a WHERE");
    expect(screen.getByText(/Signed in now: OpenCode · DeepSeek V4 and Codex · GPT-5\.5\./)).toBeVisible();
    // Editing .orbyn/crew.json is not built, so no control offers it.
    expect(screen.queryByRole("button", { name: /Mark a target|Open \.orbyn\/crew\.json/ })).toBeNull();
  });
});

describe("Trust", () => {
  it("shows revert and rework per engine and area with the sample, and a rate only from 10 merges", async () => {
    on("crew");
    render(<TrustTab />);
    const engines = await screen.findByRole("table", { name: "By engine" });
    expect(engines).toHaveTextContent("Claude Code · Sonnet 5.5Anthropic2 of 31reverted · 6%4 of 31reworked · 13%31 merges");
    expect(engines).toHaveTextContent("Codex · GPT-5.5OpenAI1 of 18reverted · 6%3 of 18reworked · 17%18 merges");
    expect(engines).toHaveTextContent("OpenCode · DeepSeek V4DeepSeek0 of 4too few for a rate1 of 4too few for a rate4 merges");
    expect(screen.getByRole("table", { name: "By area" })).toHaveTextContent("keys/3 of 11reverted · 27%2 of 11reworked · 18%11 merges");
    expect(screen.getByText(/It is per repo, on this device, over the last 60 days\./)).toBeVisible();
  });

  it("shows each reviewer's precision with its sample", async () => {
    on("crew");
    render(<TrustTab />);
    const table = await screen.findByRole("table", { name: "Reviewers" });
    expect(table).toHaveTextContent("Security · Codex9 of 12confirmed findings were real · 75%3 of 12dropped by the Finding check12 findings");
    expect(table).toHaveTextContent("Correctness · Claude Code4 of 5confirmed were real · too few for a rate1 of 5");
    expect(screen.getByText("A reviewer whose findings are often dropped is asked for fewer low-severity findings. Nothing changes without showing it here before it happens.")).toBeVisible();
  });

  it("puts Crew beside solo runs, and says so when there are too few solo runs to say", async () => {
    on("crew");
    render(<TrustTab />);
    const table = await screen.findByRole("table", { name: "Crew vs solo" });
    expect(table).toHaveTextContent("Docs changes12 merges0 reverted · 1 reworked · median 22 min · $0.849 runs1 reverted · 2 reworked · median 31 min · $0.71Crew: fewer reverts, 9 min faster, $0.13 more");
    expect(table).toHaveTextContent("Auth changes14 merges2 reverted · 3 reworked · median 41 min · $1.623 runstoo few to compareNot enough solo runs to say");
  });

  it("with few merges says the router uses defaults, and shows no percentage", async () => {
    on("crew-young");
    render(<TrustTab />);
    const banner = await screen.findByText(/This repo has 5 merges/);
    expect(banner).toHaveTextContent("This repo has 5 merges. A rate shows from 10 merges, over the last 60 days, per repo on this device; until then Crew shows the individual results and the router uses defaults.");
    const table = screen.getByRole("table", { name: "By engine" });
    expect(table).toHaveTextContent("Claude Code · Sonnet 5.50 of 3too few for a rate1 of 3too few for a rate3 merges");
    expect(table).not.toHaveTextContent("%");
    expect(screen.queryByRole("table", { name: "By area" })).toBeNull();
  });
});

describe("Background crews", () => {
  it("lists each crew with its trigger, last run, state and Autopilot setting", async () => {
    on("crew");
    render(<BackgroundTab />);
    const table = await screen.findByRole("table", { name: "Background crews" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Crew", "Starts when", "Last run", "State", "Autopilot"]);
    expect(table).toHaveTextContent("Fix failing CITemplate: fix failing CIWhen CI fails on mainSat 7 Nov, 15:11 · landed in ReviewOnFull");
    expect(table).toHaveTextContent("Security advisoriesTemplate: security advisoryWhen a security advisory is publishedNever runOffOff");
    expect(screen.getByText(".orbyn/crew.json")).toBeVisible();
  });

  it("turns a crew on or off from the list", async () => {
    on("crew");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    const sw = await screen.findByRole("switch", { name: "Security advisories state" });
    expect(sw).not.toBeChecked();
    await user.click(sw);
    expect(sw).toBeChecked();
    expect((await crewApi.background()).crews.find((one) => one.id === "sec")?.enabled).toBe(true);
  });

  it("lists recent runs, a budget stop as Stopped and not Failed", async () => {
    on("crew");
    render(<BackgroundTab />);
    const runs = await screen.findByRole("table", { name: "Recent runs" });
    expect(runs).toHaveTextContent("Fix failing CI · f1a0c63Template: fix failing CICI failed on main · 15:11Landed in Review · 15:49 · MediumMergedFull");
    expect(runs).toHaveTextContent("Fix failing CI · e44b01dTemplate: fix failing CICI failed on main · Fri 6 NovStopped · budgetStoppedFull");
    expect(within(runs).queryByText("Failed")).toBeNull();
  });

  it("offers the four templates and the four triggers with how repeats are handled", async () => {
    on("crew");
    render(<BackgroundTab />);
    const templates = await screen.findByRole("region", { name: "Templates" });
    for (const name of ["Fix failing CI", "Flaky test", "Dependency bump", "Security advisory"]) expect(within(templates).getByRole("article", { name })).toBeVisible();
    expect(templates).toHaveTextContent("It is High risk by default.");
    const triggers = screen.getByRole("table", { name: "Triggers" });
    expect(triggers).toHaveTextContent("One run per failing commit; a new failure during a run is added to it.");
    expect(triggers).toHaveTextContent("The issue’s text is the brief. It is data, never instructions, and goes through the same checks.");
    expect(triggers).toHaveTextContent("Skipped if the previous run is still going.");
    expect(triggers).toHaveTextContent("One run per update; updates to the same package are grouped.");
  });

  it("creates a background crew from a template in a short form", async () => {
    on("crew");
    const user = userEvent.setup();
    render(<BackgroundTab />);
    await user.click(await screen.findByRole("button", { name: "Use the Dependency bump template" }));
    const dialog = await screen.findByRole("dialog", { name: "Create a background crew" });
    expect(dialog).toHaveTextContent("It is saved in .orbyn/crew.json so it can be reviewed like any other change.");
    expect(within(dialog).getByRole("radio", { name: "Dependency bump" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: "A dependency update opens" })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Create background crew" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect((await crewApi.background()).crews).toHaveLength(5);
  });

  it("says what a background crew is, and offers the templates, when there are none", async () => {
    on("crew-no-background");
    render(<BackgroundTab />);
    expect(await screen.findByText("No background crews yet")).toBeVisible();
    expect(screen.getByText(/A background crew starts when something happens in the repo/)).toBeVisible();
    expect(screen.getByRole("button", { name: "See the templates" })).toBeVisible();
    expect(screen.queryByRole("table", { name: "Background crews" })).toBeNull();
  });

  it("is a small card per crew in the 320 px panel", async () => {
    on("crew");
    const view = await crewApi.background();
    render(<BackgroundCards crews={view.crews} />);
    const cards = screen.getAllByRole("listitem");
    expect(cards).toHaveLength(4);
    expect(cards[0]).toHaveTextContent("Fix failing CIOnWhen CI fails on main");
    expect(cards[3]).toHaveTextContent("Security advisoriesOffWhen a security advisory is published");
  });

  it("shows a run that reached its budget as Stopped, with what it finished", () => {
    const open = vi.fn();
    render(<StoppedBudgetItem onOpen={open} onRaise={() => undefined} run={{ title: "Fix failing CI · e44b01d", risk: "medium", meta: "Background crew · Fri 6 Nov · Claude Code · Sonnet 5.5 · crew/ci-fix · 2 of 3 changes finished", checks: "2 of 3", notVerified: 3, budget: "300k of 300k tokens" }} />);
    const item = screen.getByRole("article", { name: "Fix failing CI · e44b01d · Stopped · budget" });
    expect(item).toHaveClass("ap-box-am");
    expect(item).toHaveTextContent("It reached its budget. What it finished is merged or ready; the rest is listed.");
    expect(item).toHaveTextContent("Checks 2 of 3 · Not verified: 3 · 300k of 300k tokens");
    expect(within(item).getByRole("button", { name: "Raise the budget and continue" })).toBeVisible();
    screen.getByRole("button", { name: "Open what it finished" }).click();
    expect(open).toHaveBeenCalled();
  });
});
