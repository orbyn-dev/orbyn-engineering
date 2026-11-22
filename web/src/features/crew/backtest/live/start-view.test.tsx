import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../../../../api/types";
import { useWorkspace } from "../../../../product/state/workspace";
import { LiveBacktest } from "./LiveBacktest";
import { LivePage } from "./LivePage";
import { estimate, posts, project, startLane, startRun, startView, stubFetch, type Answer, type AnswerBody, type Call } from "./support";

let calls: Call[] = [];
let answers: { preview: Answer; projects: Answer; estimate: Answer; startView: Answer; startViewById: Record<string, Answer>; report: Answer };

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    estimate: { status: 200, body: estimate() },
    startView: { status: 200, body: startView() },
    startViewById: {},
    report: { status: 200, body: runReport() },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

const runReport = () => ({ id: "run-4n", project: "p-app", state: "finished", budgets: [{ engine: "claude", tokens: 1_500_000 }], stop_reason: null, created_unix: 1, finished_unix: 2, lanes: [] });

const page = () => render(<LiveBacktest view="page" />);
const startViewCalls = () => calls.filter(({ url }) => url.includes("/api/crew/backtest/start-view")).map(({ url }) => url);
const row = (title: string) => screen.getByText(title).closest(".cx-setrow") as HTMLElement;

describe("the reviewer backtest page on the start-view read", () => {
  it("reads the last run's lanes, kinds and per-engine budgets", async () => {
    page();
    expect(await screen.findByText("correctness · claude · sonnet-5-5")).toBeVisible();
    expect(startViewCalls()).toEqual(["/api/crew/backtest/start-view?project=p-app"]);
    const lanes = row("Which lanes");
    expect(within(lanes).getAllByText("Review")).toHaveLength(3);
    expect(within(lanes).queryByText("Planned")).toBeNull();
    expect(within(lanes).queryByText("Not shown")).toBeNull();
    const budgets = row("Run budget");
    expect(within(budgets).getByText(/^claude:$/)).toBeVisible();
    expect(within(budgets).getByText("1.5M tokens")).toBeVisible();
    expect(within(budgets).getByText(/^codex:$/)).toBeVisible();
    expect(within(budgets).getByText("1M tokens")).toBeVisible();
    expect(within(budgets).getByText(/^opencode:$/)).toBeVisible();
    expect(within(budgets).getByText("500k tokens")).toBeVisible();
    expect(within(budgets).queryByText("Planned")).toBeNull();
    expect(within(budgets).queryByText("Not shown")).toBeNull();
  });

  it("shows Kind not recorded for a lane whose kind is null, never Review", async () => {
    answers.startView = {
      status: 200,
      body: startView({
        run: startRun({
          lanes: [startLane(), startLane({ engine: "codex", model: "gpt-5-5" }), startLane({ lane: "security", engine: "opencode", model: "deepseek-v4", kind: null })],
        }),
      }),
    };
    page();
    const line = await screen.findByText("security · opencode · deepseek-v4");
    expect(line.parentElement).toHaveTextContent("Kind not recorded");
    expect(line.parentElement).not.toHaveTextContent("Review");
    expect(within(row("Which lanes")).getAllByText("Review")).toHaveLength(2);
  });

  it("draws a check lane as Check", async () => {
    answers.startView = {
      status: 200,
      body: startView({ run: startRun({ lanes: [startLane({ lane: "checks", kind: "check" })] }) }),
    };
    page();
    expect(await screen.findByText("checks · claude · sonnet-5-5")).toBeVisible();
    expect(screen.getByText("Check")).toBeVisible();
  });

  it("lists one budget per engine and never sums them, and an engine without one reads Not set", async () => {
    answers.startView = {
      status: 200,
      body: startView({
        run: startRun({ budgets: [{ engine: "claude", tokens: 1_500_000 }, { engine: "codex", tokens: 1_000_000 }] }),
      }),
    };
    page();
    const budget = (await screen.findByText("Run budget")).closest(".cx-setrow") as HTMLElement;
    expect(budget).toHaveTextContent("claude: 1.5M tokens");
    expect(budget).toHaveTextContent("codex: 1M tokens");
    expect(budget).toHaveTextContent("opencode: Not set");
    expect(budget).not.toHaveTextContent(/total/i);
    expect(screen.queryByText("2.5M tokens")).toBeNull();
    expect(screen.queryByText("3M tokens")).toBeNull();
  });

  it("draws Not set and None yet without inventing a lane when the project never ran a backtest", async () => {
    answers.startView = { status: 200, body: startView({ run: null }) };
    page();
    expect(await screen.findByText("None yet")).toBeVisible();
    const lanes = row("Which lanes");
    expect(within(lanes).getByText("Not set")).toBeVisible();
    expect(within(lanes).getByText("Planned")).toBeVisible();
    const budgets = row("Run budget");
    expect(within(budgets).getByText("Not set")).toBeVisible();
    expect(within(budgets).getByText("Planned")).toBeVisible();
    const last = row("Last backtest");
    expect(within(last).queryByText("Planned")).toBeNull();
    expect(screen.queryByText(/correctness · claude/)).toBeNull();
    expect(screen.queryByRole("link", { name: /Open the report/ })).toBeNull();
  });

  it("fails closed in one quiet notice, keeps the estimate rows, and Try again reads only the start view", async () => {
    const user = userEvent.setup();
    answers.startView = { status: 500, body: { code: "unreadable", message: "the backtest start input can’t be read on this device" } };
    page();
    const notice = await screen.findByText(/Can’t read the lanes and budgets/);
    expect(notice).toBeVisible();
    expect(screen.getByText("What it replays")).toBeVisible();
    expect(screen.queryByText(/correctness · claude/)).toBeNull();
    const before = startViewCalls().length;
    answers.startView = { status: 200, body: startView() };
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("correctness · claude · sonnet-5-5")).toBeVisible();
    expect(startViewCalls()).toHaveLength(before + 1);
    expect(posts(calls)).toHaveLength(0);
  });

  it("names a failed last run and opens its report, with no list of past backtests", async () => {
    answers.startView = { status: 200, body: startView({ run: startRun({ state: "failed" }) }) };
    page();
    const last = (await screen.findByText("Last backtest")).closest(".cx-setrow") as HTMLElement;
    expect(within(last).getByText("Failed")).toBeVisible();
    expect(within(last).getByRole("link", { name: "Open the report" })).toHaveAttribute("href", "#/settings/crew/backtest/report?id=run-4n");
    expect(screen.queryByText(/past backtest/i)).toBeNull();
  });

  it("reads the lanes and budgets quietly while they load", async () => {
    answers.startView = "never";
    const { container } = page();
    expect(await screen.findByRole("status", { name: "Reading the lanes and budgets" })).toBeInTheDocument();
    expect(await screen.findByText("Reading the lanes of the last backtest.")).toBeVisible();
    await waitFor(() => expect(container.querySelectorAll(".cx-skelbar")).toHaveLength(6), { timeout: 1500 });
  });

  it("keeps a slow older answer from replacing a newer project's rows", async () => {
    const second: Project = { ...project, id: "p-second", name: "second-project" };
    let release!: (answer: AnswerBody) => void;
    const held = new Promise<AnswerBody>((resolve) => {
      release = resolve;
    });
    answers.projects = { status: 200, body: [project, second] };
    answers.startViewById = {
      [project.id]: held,
      [second.id]: { status: 200, body: startView({ project: second.id, run: startRun({ lanes: [startLane({ lane: "second", engine: "codex", model: "gpt-5-5" })] }) }) },
    };
    const view = render(<LivePage project={project} projects={[project, second]} onPick={() => undefined} />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("project=p-app"))).toBe(true));
    view.rerender(<LivePage project={second} projects={[project, second]} onPick={() => undefined} />);
    expect(await screen.findByText("second · codex · gpt-5-5")).toBeVisible();
    release({ status: 200, body: startView() });
    await waitFor(() => expect(screen.queryByText("correctness · claude · sonnet-5-5")).toBeNull());
    expect(screen.getByText("second · codex · gpt-5-5")).toBeVisible();
  });

  it("has no Start, Stop or run control on the page", async () => {
    page();
    await screen.findByText("correctness · claude · sonnet-5-5");
    expect(screen.queryByRole("button", { name: /start|stop|run a backtest/i })).toBeNull();
    expect(screen.queryByRole("link", { name: /stop/i })).toBeNull();
  });

  it("says a running backtest finishes by itself, without claiming a person stops it on its report", async () => {
    answers.estimate = { status: 200, body: estimate({ running: "run-live" }) };
    page();
    expect(await screen.findByText("A backtest is running.")).toBeVisible();
    expect(screen.getByText(/Only one runs at a time: it finishes by itself\./)).toBeVisible();
    expect(screen.queryByText(/stops it on its report/i)).toBeNull();
    expect(screen.queryByText(/a person stops/i)).toBeNull();
  });

  it("marks the sheet narrow at 640 px and keeps each lane and budget on one line", async () => {
    class Narrow {
      constructor(private readonly send: ResizeObserverCallback) {}
      observe(target: Element) {
        this.send([{ target, contentRect: { width: 600 } } as unknown as ResizeObserverEntry], this as unknown as ResizeObserver);
      }
      unobserve() {}
      disconnect() {}
    }
    vi.stubGlobal("ResizeObserver", Narrow);
    page();
    await screen.findByText("correctness · claude · sonnet-5-5");
    const sheet = screen.getByText("Crew › Reviewer backtest").closest(".cx-cwsheet") as HTMLElement;
    expect(sheet).toHaveClass("cx-narrow");
    expect(screen.getByText("correctness · claude · sonnet-5-5")).toBeVisible();
    expect(screen.getByText("1.5M tokens")).toBeVisible();
  });
});
