import { render, screen, waitFor } from "@testing-library/react";
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
let answers: { preview: Answer; capacity: Answer; coverage: Answer };
const sent = () => vi.mocked(fetch).mock.calls.map(([url, init]) => `${init?.method ?? "GET"} ${String(url)}`);

beforeEach(() => {
  answers = { preview: { status: 200, body: { enabled: true } }, capacity: { status: 200, body: state() }, coverage: { status: 200, body: coverage() } };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      const url = String(input);
      const answer = url.includes("/api/crew/preview") ? answers.preview : url.includes("/api/crew/capacity/coverage") ? answers.coverage : url.includes("/api/crew/capacity") ? answers.capacity : { status: 404 };
      if (answer === "never") return new Promise<Response>(() => undefined);
      return new Response(answer.body === undefined ? null : JSON.stringify(answer.body), { status: answer.status });
    }),
  );
});

describe("Review capacity on the real routes", () => {
  it("draws nothing and reads no capacity route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LiveCapacity view="settings" />);
    await waitFor(() => expect(sent()).toContain("GET /api/crew/preview"));
    await new Promise((resolve) => setTimeout(resolve, 20));
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

  it("never sends a write on load, and the screen has no control that could", async () => {
    const { container } = render(<LiveCapacity view="settings" />);
    await screen.findByText("8 changes");
    expect(sent().filter((call) => !call.startsWith("GET "))).toEqual([]);
    expect(container.querySelectorAll("button, input, select, textarea")).toHaveLength(0);
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
