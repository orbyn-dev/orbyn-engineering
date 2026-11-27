import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../../product/state/workspace";
import { LiveBacktest } from "./LiveBacktest";
import { tokens, WHY } from "./model";
import { estimate, lane, notMeasuredLane, posts, project, run, stubFetch, type Answer, type Call } from "./support";

let calls: Call[] = [];
let answers: { preview: Answer; projects: Answer; estimate: Answer; report: Answer };

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    estimate: { status: 200, body: estimate() },
    report: { status: 200, body: run() },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

/** A report frame names its run in the hash; the app resolves it and passes it down, as the escape detail does. */
const openReport = (id = "run-4n") => render(<LiveBacktest view="report" id={id} />);

describe("the reviewer backtest on the real 4n routes", () => {
  it("reads the estimate and shows the real size of the backtest", async () => {
    render(<LiveBacktest view="estimate" />);
    expect(await screen.findByText("64 merged pull requests are 30 days old or more.")).toBeInTheDocument();
    expect(screen.getByText("The last 180 days")).toBeInTheDocument();
    expect(screen.getByText(/Outcomes: Measured from the escape log/)).toBeInTheDocument();
    expect(calls.some(({ url }) => url.startsWith("/api/crew/backtest/estimate") && url.includes("project=p-app"))).toBe(true);
  });

  it("says Not measured, never a zero, when the escape log cannot say", async () => {
    answers.estimate = { status: 200, body: estimate({ outcomes_measured: false }) };
    render(<LiveBacktest view="estimate" />);
    expect(await screen.findByText(/Outcomes: Not measured/)).toBeInTheDocument();
    expect(screen.getByText(/the report shows finding counts only, never a zero/)).toBeInTheDocument();
  });

  it("opens the run dialog once, even on a double-click, and Escape closes it", async () => {
    const user = userEvent.setup();
    render(<LiveBacktest view="estimate" />);
    const dialog = await screen.findByRole("dialog", { name: "Run a reviewer backtest" });
    // jsdom has no Escape-to-cancel for a native modal; the browser's own step is this event.
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await user.dblClick(screen.getByRole("button", { name: "Run a backtest" }));
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });

  it("closes the run dialog on Cancel, even on a double-click, and keeps Start Planned inside it", async () => {
    const user = userEvent.setup();
    render(<LiveBacktest view="estimate" />);
    const dialog = await screen.findByRole("dialog", { name: "Run a reviewer backtest" });
    expect(dialog).toHaveClass("dialog");
    expect(within(dialog).getByRole("button", { name: "Start the backtest" })).toBeDisabled();
    await user.dblClick(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText(/No backtest is running here/)).toBeInTheDocument();
  });

  it("writes token counts with no stray point", () => {
    expect(tokens(1_500_000)).toBe("1.5M tokens");
    expect(tokens(12_500_000)).toBe("12.5M tokens");
    expect(tokens(2_000_000)).toBe("2M tokens");
    expect(tokens(1_280_000)).toBe("1.28M tokens");
    expect(tokens(396_000)).toBe("396k tokens");
    expect(tokens(1_500)).toBe("1.5k tokens");
  });

  it("says so when nothing is old enough to replay", async () => {
    answers.estimate = { status: 200, body: estimate({ eligible_prs: 0 }) };
    render(<LiveBacktest view="estimate" />);
    expect(await screen.findByText("No merged pull requests are 30 days old or more yet.")).toBeInTheDocument();
  });

  it("offers the run in progress from the estimate", async () => {
    answers.estimate = { status: 200, body: estimate({ running: "run-live" }) };
    render(<LiveBacktest view="estimate" />);
    expect(await screen.findByText(/A backtest is running/)).toBeInTheDocument();
    expect(screen.getByText(/Only one runs at a time: it finishes by itself\./)).toBeVisible();
    expect(screen.queryByText(/stops it on its report/i)).toBeNull();
    expect(screen.getByRole("button", { name: "Open the running backtest" })).toBeEnabled();
  });

  it("keeps Start Planned and names the read a start needs", async () => {
    render(<LiveBacktest view="estimate" />);
    const start = await screen.findByRole("button", { name: "Start the backtest" });
    expect(start).toBeDisabled();
    expect(start).toHaveAttribute("aria-description", `Planned: ${WHY.start}`);
    expect(WHY.start).toContain("lane list (lane, engine, model, review or check)");
    expect(WHY.start).toContain("token budget per engine");
  });

  it("reads a run by the id in the hash and shows each lane", async () => {
    openReport();
    expect(await screen.findByText("correctness · claude · sonnet-5-5")).toBeInTheDocument();
    expect(screen.getByText("6 of 9")).toBeInTheDocument();
    expect(screen.getByText("recall 66%")).toBeInTheDocument();
    expect(screen.getByText("6 of 21")).toBeInTheDocument();
    expect(screen.getByText("precision 28%")).toBeInTheDocument();
    expect(screen.getByText("21")).toBeInTheDocument();
    expect(screen.getByText("2 unresolved")).toBeInTheDocument();
    expect(screen.getByText("1.28M tokens")).toBeInTheDocument();
    expect(calls.some(({ url }) => url === "/api/crew/backtest/run-4n/report")).toBe(true);
  });

  it("labels the stacked narrow report: the run card first, then a card per lane", async () => {
    openReport();
    expect(await screen.findByText("Outcomes")).toBeInTheDocument();
    expect(screen.getByText("Lanes that scored")).toBeInTheDocument();
    expect(screen.getByText("What it used")).toBeInTheDocument();
    expect(screen.getByText("Escapes caught: 6 of 9 · recall 66%")).toBeInTheDocument();
    expect(screen.getByText("Findings raised: 21 · 2 unresolved")).toBeInTheDocument();
    expect(screen.getByText("Matched an escape: 6 of 21 · precision 28%")).toBeInTheDocument();
  });

  it("keeps every stacked lane cell's column heading in the narrowed table", async () => {
    openReport();
    await screen.findByText("correctness · claude · sonnet-5-5");
    const table = document.querySelector<HTMLElement>(".cx-bt-report .cx-aptb");
    expect(table).not.toBeNull();
    const headings = [...table!.querySelectorAll(":scope > .cx-h")].map((cell) => cell.textContent);
    expect(headings).toEqual(["Lane", "Escapes it would have caught", "Findings raised", "Findings that matched an escape"]);
    // Each score cell repeats its header's words in a label the narrow layout shows and the wide one hides.
    const labels = [...table!.querySelectorAll(".cx-bt-narrow")].map((label) => label.textContent ?? "");
    expect(labels).toEqual([
      "Escapes caught: 6 of 9 · recall 66%",
      "Findings raised: 21 · 2 unresolved",
      "Matched an escape: 6 of 21 · precision 28%",
    ]);
    // The wide layout's own figures stay for the wide table, and are what the narrow breakpoint hides.
    expect(table!.querySelectorAll(".cx-bt-wide")).toHaveLength(6);
  });

  it("hides the header row and shows those labels only in the narrow container", () => {
    // jsdom runs no container query: the rule that keeps each stacked cell's heading is read from its source.
    const css = readFileSync(join(process.cwd(), "src", "features", "crew", "backtest", "live", "backtest-live.css"), "utf8");
    const [wide = "", narrow = ""] = css.split("@container (max-width: 640px)");
    expect(wide).toMatch(/\.cx-t14pg \.cx-aptb \.cx-bt-narrow \{\s*display: none;/);
    expect(narrow).toMatch(/\.cx-bt-report \.cx-aptb > div\.cx-h \{\s*display: none;/);
    expect(narrow).toMatch(/\.cx-bt-report \.cx-aptb \.cx-bt-wide \{\s*display: none;/);
    expect(narrow).toMatch(/\.cx-bt-report \.cx-aptb \.cx-bt-narrow \{\s*display: block;/);
  });

  it("keeps the replay count Planned, never a number", async () => {
    openReport();
    expect(await screen.findByText("The report route does not serve how many pull requests each lane replayed.")).toBeInTheDocument();
    expect(screen.queryByText(/pull requests replayed/i)).toBeNull();
  });

  it("shows Not measured where the escape log cannot score a lane, never a zero rate", async () => {
    answers.report = { status: 200, body: run({ lanes: [lane(), notMeasuredLane()] }) };
    openReport();
    expect(await screen.findByText("security · opencode · deepseek-v4")).toBeInTheDocument();
    expect(screen.getAllByText("Not measured").length).toBeGreaterThan(0);
    expect(screen.getAllByText("the escape log can’t say").length).toBeGreaterThan(0);
    expect(screen.queryByText("0%")).toBeNull();
  });

  it("shows a stopped run as Partial, in the service's own reason", async () => {
    answers.report = { status: 200, body: run({ state: "stopped", stop_reason: "stopped by the person" }) };
    openReport();
    expect(await screen.findByText("You stopped this backtest.")).toBeInTheDocument();
    expect(screen.getByText("Partial")).toBeInTheDocument();
    expect(screen.getByText(/The counts below are for the pull requests it replayed and no others/)).toBeInTheDocument();
  });

  it("names a budget stop in the service's own reason", async () => {
    answers.report = { status: 200, body: run({ state: "stopped", stop_reason: "token budget spent: codex" }) };
    openReport();
    expect(await screen.findByText(/The token budget was spent for codex/)).toBeInTheDocument();
  });

  it("keeps Run again Planned and names the field a start cannot rebuild", async () => {
    openReport();
    const again = await screen.findByRole("button", { name: "Run again" });
    expect(again).toBeDisabled();
    expect(again).toHaveAttribute("aria-description", `Planned: ${WHY.runAgain}`);
    expect(WHY.runAgain).toContain("a review or a check");
  });

  it("shows the design's rate rule under the table", async () => {
    openReport();
    expect(await screen.findByText(/A rate shows from 5 escapes/)).toBeInTheDocument();
    expect(screen.getByText(/A rate shows from 10 findings/)).toBeInTheDocument();
  });

  it("says so when the frame names no run", async () => {
    render(<LiveBacktest view="report" />);
    expect(await screen.findByText("No backtest to show")).toBeInTheDocument();
    expect(calls.some(({ url }) => url.includes("/report"))).toBe(false);
  });

  it("fails closed when the report cannot be read, and a retry reads again", async () => {
    answers.report = { status: 500, body: { code: "unreadable", message: "The backtest store can’t be read." } };
    const user = userEvent.setup();
    openReport();
    expect(await screen.findByText(/Can’t read the backtest/)).toBeInTheDocument();
    expect(screen.getByText(/unreadable: The backtest store can’t be read/)).toBeInTheDocument();
    answers.report = { status: 200, body: run() };
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("correctness · claude · sonnet-5-5")).toBeInTheDocument();
  });

  it("draws nothing and reads no backtest route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LiveBacktest view="estimate" />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/backtest"))).toBe(false);
  });

  it("loads the project list itself when the shell has not", async () => {
    useWorkspace.setState({ projects: { data: null, loading: false, error: null } });
    render(<LiveBacktest view="estimate" />);
    expect(await screen.findByText("64 merged pull requests are 30 days old or more.")).toBeInTheDocument();
    expect(calls.some(({ url }) => url.startsWith("/api/projects"))).toBe(true);
  });

  it("sends no write to draw: only reads ran", async () => {
    render(<LiveBacktest view="estimate" />);
    await screen.findByText("64 merged pull requests are 30 days old or more.");
    openReport();
    await screen.findByText("correctness · claude · sonnet-5-5");
    expect(posts(calls)).toHaveLength(0);
  });
});
