import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../../api/client";
import type { Project } from "../../../../api/types";
import type { CapacityState } from "../../../../generated/CapacityState";
import type { ReviewCoverage } from "../../../../generated/ReviewCoverage";
import { useWorkspace } from "../../../../product/state/workspace";
import { capacityApi } from "./api";
import { LiveCapacity } from "./LiveCapacity";

const project = { id: "p-app", name: "orbyn-engineering", trusted: true, is_git: true, path: "/fixture/app" } as Project;
const state = (over: Partial<CapacityState> = {}): CapacityState => ({
  project: "p-app",
  waiting: 3,
  limit: 8,
  sampling: "off",
  pause_enabled: false,
  settings_saved: true,
  picked: [],
  plan: [
    { from: 1, to: 4, sampled: 1 },
    { from: 5, to: 8, sampled: 2 },
  ],
  ...over,
});
const coverage = (over: Partial<ReviewCoverage> = {}): ReviewCoverage => ({
  project: "p-app",
  began_unix: 1_760_000_000,
  as_of_unix: 1_762_500_000,
  merges: 9,
  not_measured_merges: 0,
  agent_lines: 1420,
  reviewed_lines: 910,
  in_queue_lines: 310,
  areas: [{ area: "auth/", merges: 9, agent_lines: 1420, reviewed_lines: 910, in_queue_lines: 310, not_measured_merges: 0 }],
  engines: [{ engine: "claude", model: "sonnet", agent_lines: 1420, reviewed_lines: 910 }],
  ...over,
});

type Answer = { status: number; body?: unknown } | "never";
let hold: { project: string; answer: Promise<Response> } | null;
let answers: { preview: Answer; capacity: Answer; coverage: Answer; projects: Answer; coverageBy: Record<string, Answer> };
const sent = () => vi.mocked(fetch).mock.calls.map(([url, init]) => `${init?.method ?? "GET"} ${String(url)}`);

beforeEach(() => {
  hold = null;
  answers = { preview: { status: 200, body: { enabled: true } }, capacity: { status: 200, body: state() }, coverage: { status: 200, body: coverage() }, projects: { status: 200, body: [project] }, coverageBy: {} };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      const url = String(input);
      const answer = url.startsWith("/api/projects") ? answers.projects : url.includes("/api/crew/preview") ? answers.preview : url.includes("/api/crew/capacity/coverage") ? (answers.coverageBy[new URL(url, "http://x").searchParams.get("project") ?? ""] ?? answers.coverage) : url.includes("/api/crew/capacity") ? answers.capacity : { status: 404 };
      if (hold && url.includes("/api/crew/capacity/coverage") && new URL(url, "http://x").searchParams.get("project") === hold.project) return hold.answer;
      if (answer === "never") return new Promise<Response>(() => undefined);
      return new Response(answer.body === undefined ? null : JSON.stringify(answer.body), { status: answer.status });
    }),
  );
});

describe("Review capacity on the real routes", () => {
  it("loads the project list itself when the shell has not (a Preview hash opens before the workspace loads)", async () => {
    useWorkspace.setState({ projects: { data: null, loading: false, error: null } });
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("8 changes")).toBeInTheDocument();
    expect(sent()).toContain("GET /api/projects");
  });

  it("says so when the project list can't be read, and has no capacity read to make", async () => {
    useWorkspace.setState({ projects: { data: null, loading: false, error: null } });
    answers.projects = { status: 500, body: { code: "boom", message: "No list." } };
    render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("Can’t read the projects")).toBeInTheDocument();
    expect(sent().filter((call) => call.includes("/api/crew/capacity"))).toHaveLength(0);
  });

  it("draws nothing and reads no capacity route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LiveCapacity view="settings" />);
    await waitFor(() => expect(sent()).toContain("GET /api/crew/preview"));
    // The read has been asked; wait until its 404 has settled into the shared read before asserting nothing was drawn.
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(container).toBeEmptyDOMElement();
    expect(sent().filter((call) => call.includes("/capacity"))).toHaveLength(0);
  });

  it("says it is loading while the read is out", async () => {
    answers.capacity = "never";
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("Reading review capacity")).toBeInTheDocument();
  });

  it("says so when the read fails, and a retry reads again", async () => {
    answers.capacity = { status: 500, body: { code: "boom", message: "The store is busy." } };
    const user = userEvent.setup();
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("Can’t read review capacity")).toBeInTheDocument();
    answers.capacity = { status: 200, body: state() };
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("8 changes")).toBeInTheDocument();
    expect(sent().filter((call) => call.startsWith("GET /api/crew/capacity?"))).toHaveLength(2);
  });

  it("shows no limit while none was saved, never the default 8, and a sampling default", async () => {
    answers.capacity = { status: 200, body: state({ settings_saved: false, waiting: 3 }) };
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("No limit is set.")).toBeInTheDocument();
    expect(screen.getByText(/3 High and Medium changes are waiting for your review now/)).toBeInTheDocument();
    expect(screen.getByText("Not set")).toBeInTheDocument();
    expect(screen.getByText("Needs a limit")).toBeInTheDocument();
    expect(screen.getByText("Review every batch · default")).toBeInTheDocument();
    expect(screen.queryByText(/8 changes/)).toBeNull();
  });

  it("the pause switch reads Off until a person turns it on, then shows the pause", async () => {
    const off = render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("8 changes")).toBeInTheDocument();
    expect(screen.getByText("Off")).toBeInTheDocument();
    expect(screen.queryByText("Pause new Low-priority starts")).toBeNull();
    off.unmount();
    answers.capacity = { status: 200, body: state({ pause_enabled: true }) };
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("Pause new Low-priority starts")).toBeInTheDocument();
    expect(screen.queryByText(/New low-priority starts are paused/)).toBeNull();
  });

  it("says new starts are paused only while the service says so, with its counts", async () => {
    answers.capacity = { status: 200, body: state({ pause_enabled: true, waiting: 9, paused: { waiting: 9, limit: 8 } }) };
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("New low-priority starts are paused: your review queue is over its limit.")).toBeInTheDocument();
    expect(screen.getByText(/9 changes are waiting for your review and your limit is 8/)).toBeInTheDocument();
  });

  it("draws the sampling plan from the service when sampling is on, and no money", async () => {
    answers.capacity = { status: 200, body: state({ sampling: "one_in_four" }) };
    const { container } = render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("Review a sample")).toBeInTheDocument();
    expect(screen.getByText("The sampling plan")).toBeInTheDocument();
    expect(screen.getByText("5 to 8")).toBeInTheDocument();
    expect(screen.getByText("3 to 6")).toBeInTheDocument();
    expect(container.textContent).not.toMatch(/\$\d/);
  });

  it("never sends a write on load, and every control on the screen is a disabled, planned one", async () => {
    const { container } = render(<LiveCapacity view="settings" />);
    await screen.findByText("8 changes");
    expect(sent().filter((call) => !call.startsWith("GET "))).toEqual([]);
    expect(container.querySelectorAll("input, select, textarea")).toHaveLength(0);
    for (const control of container.querySelectorAll("button")) expect(control).toBeDisabled();
  });

  it("marks the editor Planned: the limit row has a disabled Change the limit beside a Planned pill, and clicking it sends nothing", async () => {
    const user = userEvent.setup();
    render(<LiveCapacity view="settings" />);
    await screen.findByText("8 changes");
    const button = screen.getByRole("button", { name: "Change the limit" });
    expect(button).toBeDisabled();
    expect(screen.getAllByText("Planned")).toHaveLength(1);
    await user.click(button);
    expect(sent().filter((call) => !call.startsWith("GET "))).toEqual([]);
  });

  it("with no limit saved the banner offers a disabled Set a limit (Planned), and the pause and low-risk rows use the unset wording", async () => {
    answers.capacity = { status: 200, body: state({ settings_saved: false, waiting: 3 }) };
    const { container } = render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("No limit is set.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Set a limit" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Change the limit" })).toBeNull();
    expect(screen.getByText("Planned")).toBeInTheDocument();
    expect(container.textContent).toContain("Review every low-risk batch, or review a sample sized to the batch.");
    expect(container.textContent).not.toContain("Paused starts begin by themselves");
    expect(screen.queryByText("What the limit does and doesn’t do")).toBeNull();
    expect(screen.getByRole("button", { name: "Set a limit" })).toHaveAttribute("title", expect.stringMatching(/^Planned: /));
    expect(container.textContent).not.toContain("Crew is not connected");
  });

  it("a saved limit with pausing off and every batch reviewed reads Off and Review every batch, with no default suffix, no plan and no banner", async () => {
    const { container } = render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("8 changes")).toBeInTheDocument();
    expect(screen.getByText("Off")).toBeInTheDocument();
    expect(screen.getByText("Review every batch")).toBeInTheDocument();
    expect(container.textContent).not.toContain("· default");
    expect(container.textContent).toContain("Paused starts begin by themselves when the queue is back at the limit or under it.");
    expect(screen.queryByText("The sampling plan")).toBeNull();
    expect(container.querySelector('[role="status"] b')).toBeNull();
  });

  it("puts the paused banner on the settings sheet, above the rows, with the pause row still reading Pause new Low-priority starts", async () => {
    answers.capacity = { status: 200, body: state({ pause_enabled: true, waiting: 9, paused: { waiting: 9, limit: 8 } }) };
    const { container } = render(<LiveCapacity view="settings" />);
    const banner = await screen.findByText("New low-priority starts are paused: your review queue is over its limit.");
    const row = screen.getByText("Pause new Low-priority starts");
    expect(banner.compareDocumentPosition(row) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(container.textContent).toContain("Running work carries on, and tasks you start and High-priority work are not paused.");
  });

  it("says there is no sampling plan to show when sampling is on and the service sends none, and the rest of the page is as usual", async () => {
    answers.capacity = { status: 200, body: state({ sampling: "one_in_four", pause_enabled: true, plan: [] }) };
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("No sampling plan to show")).toBeInTheDocument();
    expect(screen.getByText("Review a sample")).toBeInTheDocument();
    expect(screen.getByText("What the limit does and doesn’t do")).toBeInTheDocument();
  });

  it("keeps the title and the intro while it reads and when the read fails", async () => {
    answers.capacity = "never";
    const loading = render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("Reading review capacity")).toBeInTheDocument();
    expect(screen.getByText(/Agents can merge faster than a person can read\./)).toBeInTheDocument();
    loading.unmount();
    answers.capacity = { status: 500, body: { code: "boom", message: "The store is busy." } };
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("Can’t read review capacity")).toBeInTheDocument();
    expect(screen.getByText(/Agents can merge faster than a person can read\./)).toBeInTheDocument();
    expect(screen.getByText("Details")).toBeInTheDocument();
    expect(screen.queryByText("8 changes")).toBeNull();
  });
});

describe("the project list in front of both screens", () => {
  it("says it is reading the projects, and makes no capacity read until they arrive", async () => {
    useWorkspace.setState({ projects: { data: null, loading: false, error: null } });
    answers.projects = "never";
    render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("Reading projects")).toBeInTheDocument();
    expect(sent().filter((call) => call.includes("/api/crew/capacity"))).toHaveLength(0);
  });

  it.each(["settings", "coverage"] as const)("says only No project to show when the service lists none (%s), with no header, map or zero", async (view) => {
    answers.projects = { status: 200, body: [] };
    useWorkspace.setState({ projects: { data: [], loading: false, error: null } });
    const { container } = render(<LiveCapacity view={view} />);
    expect(await screen.findByText("No project to show")).toBeInTheDocument();
    expect(container.textContent).not.toContain("Review coverage");
    expect(container.textContent).not.toContain("Crew › Review capacity");
    expect(container.textContent).not.toMatch(/\b0\b/);
    expect(sent().filter((call) => call.includes("/api/crew/capacity"))).toHaveLength(0);
  });

  it("says so, with a Retry that reads the list again, when the project list can't be read on the settings page", async () => {
    useWorkspace.setState({ projects: { data: null, loading: false, error: null } });
    answers.projects = { status: 500, body: { code: "boom", message: "No list." } };
    const user = userEvent.setup();
    render(<LiveCapacity view="settings" />);
    expect(await screen.findByText("Can’t read the projects")).toBeInTheDocument();
    answers.projects = { status: 200, body: [project] };
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("8 changes")).toBeInTheDocument();
  });
});

describe("Review coverage on the real routes", () => {
  it("draws the map from the counts, at merge level, with the share of lines no person has reviewed", async () => {
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findAllByText("auth/")).not.toHaveLength(0);
    expect(container.textContent).toContain("510 of 1,420");
    expect(container.textContent).toContain("lines no person has reviewed · 36%");
    expect(container.textContent).toContain("Review is counted per merge, never per line");
    expect(container.textContent).toContain("claude · sonnet");
    expect(container.textContent).not.toMatch(/\$\d/);
    expect(container.textContent).not.toMatch(/line-level review/i);
    expect(sent().filter((call) => !call.startsWith("GET "))).toEqual([]);
  });

  it("says Not measured when provenance never began, and draws no zero", async () => {
    answers.coverage = { status: 200, body: coverage({ began_unix: null, merges: 0, agent_lines: 0, reviewed_lines: 0, in_queue_lines: 0, areas: [], engines: [], not_measured_merges: 4 }) };
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("No agent-written lines are recorded yet")).toBeInTheDocument();
    expect(container.textContent).toContain("every merge is Not measured");
    expect(container.textContent).toContain("4 merges are Not measured");
    expect(container.textContent).not.toContain("0 of 0");
  });

  it("is empty when recording began but no merge has lines", async () => {
    answers.coverage = { status: 200, body: coverage({ merges: 0, agent_lines: 0, reviewed_lines: 0, in_queue_lines: 0, areas: [], engines: [] }) };
    render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("No agent-written lines are recorded yet")).toBeInTheDocument();
  });

  it("names the merges that are not in the map, and an area with no lines or an engine with no model says Not measured", async () => {
    answers.coverage = {
      status: 200,
      body: coverage({
        not_measured_merges: 14,
        merges: 24,
        areas: [
          { area: "auth/", merges: 9, agent_lines: 1420, reviewed_lines: 910, in_queue_lines: 310, not_measured_merges: 0 },
          { area: "docs/", merges: 0, agent_lines: 0, reviewed_lines: 0, in_queue_lines: 0, not_measured_merges: 3 },
        ],
        engines: [{ engine: "codex", agent_lines: 10, reviewed_lines: 0 }],
      }),
    };
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("14 of 38 merges aren’t in the map.")).toBeInTheDocument();
    expect(container.textContent).toContain("Their lines are Not measured, not counted as read or unread.");
    expect(screen.getAllByText("Not measured").length).toBeGreaterThan(0);
    expect(container.textContent).toContain("codex · model not measured");
    expect(container.textContent).toContain("had no stored hunks");
    // The area with no lines is Not measured in every cell and names its own Not measured merges, never zeros.
    const docs = [...container.querySelectorAll(".cx-t15tile")].find((tile) => tile.textContent?.includes("docs/"))!;
    expect(docs.textContent).toContain("3 merges Not measured");
    expect(docs.textContent).not.toMatch(/\b0\b(?! merges with lines)/);
    expect(docs.textContent?.match(/Not measured/g)?.length).toBeGreaterThanOrEqual(4);
    const row = [...container.querySelectorAll(".cx-aptb > div")].find((cell) => cell.textContent?.startsWith("docs/"))!;
    expect(row.textContent).toContain("3 merges Not measured");
    expect(row.nextElementSibling?.textContent).toBe("Not measured");
  });

  it("shows the read error, and Try again reads once more", async () => {
    answers.coverage = { status: 500, body: { code: "notes_unreadable", message: "The notes can’t be read." } };
    const user = userEvent.setup();
    render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("Couldn’t read the provenance notes on this device. Nothing was changed, and no line was counted or removed.")).toBeInTheDocument();
    answers.coverage = { status: 200, body: coverage() };
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect((await screen.findAllByText("auth/")).length).toBeGreaterThan(0);
    expect(sent().filter((call) => call.startsWith("GET /api/crew/capacity/coverage"))).toHaveLength(2);
  });

  it("says it is loading while the read is out", async () => {
    answers.coverage = "never";
    render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("Reading review coverage")).toBeInTheDocument();
  });
});

describe("Review coverage states and the project picker", () => {
  const docs = { id: "p-docs", name: "orbyn-docs", trusted: true, is_git: true, path: "/fixture/docs" } as Project;

  it("draws the header with the project's name at once while the coverage is read", async () => {
    answers.coverage = "never";
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("Reading review coverage")).toBeInTheDocument();
    expect(container.querySelector(".cx-t10hd")?.textContent).toContain("orbyn-engineering");
    expect(container.textContent).toContain("This device only");
  });

  it("with one project the chip is plain text: no menu to open", async () => {
    render(<LiveCapacity view="coverage" />);
    await screen.findAllByText("auth/");
    expect(screen.queryByRole("button", { name: /orbyn-engineering/ })).toBeNull();
  });

  it("with several projects the chip opens a menu with the shown one checked, and choosing another reads that project's coverage", async () => {
    answers.projects = { status: 200, body: [project, docs] };
    useWorkspace.setState({ projects: { data: [project, docs], loading: false, error: null } });
    answers.coverageBy = { "p-docs": { status: 200, body: coverage({ project: "p-docs", agent_lines: 200, reviewed_lines: 50, in_queue_lines: 10, merges: 2, areas: [{ area: "guides/", merges: 2, agent_lines: 200, reviewed_lines: 50, in_queue_lines: 10, not_measured_merges: 0 }] }) } };
    const user = userEvent.setup();
    const { container } = render(<LiveCapacity view="coverage" />);
    await screen.findAllByText("auth/");
    const chip = screen.getByRole("button", { name: "orbyn-engineering" });
    expect(chip).toHaveAttribute("aria-haspopup", "menu");
    await user.click(chip);
    const menu = await screen.findByRole("menu", { name: "Project" });
    expect(within(menu).getByRole("menuitem", { name: "orbyn-engineering" })).toHaveAttribute("aria-current", "true");
    expect(sent().filter((call) => call.startsWith("GET /api/crew/capacity/coverage"))).toEqual(["GET /api/crew/capacity/coverage?project=p-app"]);
    await user.click(within(menu).getByRole("menuitem", { name: "orbyn-docs" }));
    expect((await screen.findAllByText("guides/")).length).toBeGreaterThan(0);
    expect(container.querySelector(".cx-t10hd")?.textContent).toContain("orbyn-docs");
    expect(sent().filter((call) => call.startsWith("GET /api/crew/capacity/coverage"))).toContain("GET /api/crew/capacity/coverage?project=p-docs");
    expect(sent().filter((call) => !call.startsWith("GET "))).toEqual([]);
  });

  it("closes the menu on Escape and leaves the project as it was", async () => {
    answers.projects = { status: 200, body: [project, docs] };
    useWorkspace.setState({ projects: { data: [project, docs], loading: false, error: null } });
    const user = userEvent.setup();
    render(<LiveCapacity view="coverage" />);
    await screen.findAllByText("auth/");
    await user.click(screen.getByRole("button", { name: "orbyn-engineering" }));
    await screen.findByRole("menu", { name: "Project" });
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(screen.getByRole("button", { name: "orbyn-engineering" })).toBeInTheDocument();
  });

  it("a late answer for the project that was left does not replace the chosen project's map", async () => {
    answers.projects = { status: 200, body: [project, docs] };
    useWorkspace.setState({ projects: { data: [project, docs], loading: false, error: null } });
    answers.coverageBy = { "p-docs": { status: 200, body: coverage({ project: "p-docs", areas: [{ area: "guides/", merges: 9, agent_lines: 1420, reviewed_lines: 910, in_queue_lines: 310, not_measured_merges: 0 }] }) } };
    let release: () => void = () => undefined;
    hold = { project: "p-app", answer: new Promise<Response>((resolve) => { release = () => resolve(new Response(JSON.stringify(coverage()), { status: 200 })); }) };
    const user = userEvent.setup();
    render(<LiveCapacity view="coverage" />);
    await user.click(await screen.findByRole("button", { name: "orbyn-engineering" }));
    await user.click(within(await screen.findByRole("menu", { name: "Project" })).getByRole("menuitem", { name: "orbyn-docs" }));
    expect((await screen.findAllByText("guides/")).length).toBeGreaterThan(0);
    await act(async () => {
      release();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getAllByText("guides/").length).toBeGreaterThan(0);
    expect(screen.queryByText("auth/")).toBeNull();
  });

  it("closes the menu on a click outside and leaves the project as it was", async () => {
    answers.projects = { status: 200, body: [project, docs] };
    useWorkspace.setState({ projects: { data: [project, docs], loading: false, error: null } });
    const user = userEvent.setup();
    render(<LiveCapacity view="coverage" />);
    await screen.findAllByText("auth/");
    await user.click(screen.getByRole("button", { name: "orbyn-engineering" }));
    await screen.findByRole("menu", { name: "Project" });
    await user.click(document.body);
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(screen.getAllByText("auth/").length).toBeGreaterThan(0);
  });

  it("heads a map with gaps by the banner alone: no button, no list, no Planned pill, no split by reason", async () => {
    answers.coverage = { status: 200, body: coverage({ merges: 24, not_measured_merges: 14 }) };
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("14 of 38 merges aren’t in the map.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Try/ })).toBeNull();
    expect(container.querySelectorAll(".cx-t15list, .cx-t15row")).toHaveLength(0);
    expect(container.textContent).not.toMatch(/Merges with no provenance note|Before provenance began|Try the/);
    // Planned: only the link from an area to its files.
    expect(screen.getAllByText("Planned")).toHaveLength(1);
    expect(sent().filter((call) => !call.startsWith("GET "))).toEqual([]);
  });

  it("a map of a few merges gets the same banner and exact counts as any map: no thin label and no cutoff", async () => {
    answers.coverage = { status: 200, body: coverage({ merges: 3, not_measured_merges: 2, agent_lines: 410, reviewed_lines: 150, in_queue_lines: 60, areas: [{ area: "web/app", merges: 3, agent_lines: 410, reviewed_lines: 150, in_queue_lines: 60, not_measured_merges: 2 }] }) };
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("2 of 5 merges aren’t in the map.")).toBeInTheDocument();
    expect(container.textContent).toContain("260 of 410");
    expect(container.textContent).toContain("lines no person has reviewed · 63%");
    expect(container.textContent).not.toMatch(/rests on|\bthin\b|too few|low sample/i);
  });

  it("with recording begun and no lines, says so once and draws the Open the Review queue button as Planned and disabled", async () => {
    answers.coverage = { status: 200, body: coverage({ merges: 0, agent_lines: 0, reviewed_lines: 0, in_queue_lines: 0, areas: [], engines: [], not_measured_merges: 0 }) };
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("No agent-written lines are recorded yet")).toBeInTheDocument();
    expect(container.textContent).toContain("No merge in orbyn-engineering since recording began has lines to count, so there is nothing to show.");
    expect(screen.getByRole("button", { name: "Open the Review queue" })).toBeDisabled();
    expect(screen.getByText("Planned")).toBeInTheDocument();
    expect(container.querySelectorAll(".cx-t15tile")).toHaveLength(0);
  });

  it("before recording began says every merge is Not measured with their count, and has no Review queue button", async () => {
    answers.coverage = { status: 200, body: coverage({ began_unix: null, merges: 0, agent_lines: 0, reviewed_lines: 0, in_queue_lines: 0, areas: [], engines: [], not_measured_merges: 9 }) };
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("No agent-written lines are recorded yet")).toBeInTheDocument();
    expect(container.textContent).toContain("Recording has not begun for orbyn-engineering on this device, so every merge is Not measured.");
    expect(container.textContent).toContain("9 merges are Not measured.");
    expect(container.querySelector("button")).toBeNull();
  });

  it("marks the link from an area to its files as Planned, drawn as text, not as a control", async () => {
    const { container } = render(<LiveCapacity view="coverage" />);
    await screen.findAllByText("auth/");
    expect(container.querySelector(".cx-t10r")?.textContent).toBe("Open an area for its filesPlanned");
    expect(container.querySelector(".cx-t10r button, .cx-t10r a")).toBeNull();
  });

  it("a failed read draws the banner, the raw error as the service gave it and one dash tile, never one per area", async () => {
    answers.coverage = { status: 500, body: { code: "notes_unreadable", message: "The notes can’t be read." } };
    const { container } = render(<LiveCapacity view="coverage" />);
    expect(await screen.findByText("notes_unreadable: The notes can’t be read.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy the error" })).toBeEnabled();
    const tiles = container.querySelectorAll(".cx-t15tile");
    expect(tiles).toHaveLength(1);
    expect(tiles[0]?.textContent).toBe("—Not shown while the notes can’t be read");
    expect(container.textContent).not.toContain("510 of");
  });
});

describe("the capacity writes", () => {
  it("send the person's settings to the Person route with the pause switch as given", async () => {
    answers.capacity = { status: 200, body: state({ pause_enabled: true }) };
    await capacityApi.saveSettings({ project: "p-app", limit: 6, sampling: "off", pause_enabled: true });
    const call = vi.mocked(fetch).mock.calls.find(([, init]) => init?.method === "POST")!;
    expect(String(call[0])).toBe("/api/crew/capacity/settings");
    expect(JSON.parse(String(call[1]?.body))).toEqual({ project: "p-app", limit: 6, sampling: "off", pause_enabled: true });
  });

  it("surface a refusal as the service's own failure, and refuse a bad task id before any request", async () => {
    answers.capacity = { status: 403, body: { code: "person_required", message: "Only a person can change this." } };
    await expect(capacityApi.saveSettings({ project: "p-app", limit: 6, sampling: "off", pause_enabled: true })).rejects.toMatchObject({ code: "person_required", status: 403 });
    const before = sent().length;
    expect(() => capacityApi.saveTask("../x", { priority: "low" })).toThrow(ApiFailure);
    expect(sent()).toHaveLength(before);
  });
});
