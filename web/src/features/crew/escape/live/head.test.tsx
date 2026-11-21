import { render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useWorkspace } from "../../../../product/state/workspace";
import { LiveEscapes } from "./LiveEscapes";
import { log, posts, project, row, stubFetch, type Answer, type Call } from "./support";

/** Crew Designs v87: the Settings link and the Possible tab strip in the header of the escape log and of the possible list, at every width. */

let calls: Call[] = [];
let answers: {
  preview: Answer;
  projects: Answer;
  log: Answer;
  logConfirmed: Answer;
  possible: Answer;
  detail: Answer;
  decide: Answer;
  gap: Answer;
};

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    log: { status: 200, body: log() },
    logConfirmed: { status: 200, body: log() },
    possible: { status: 200, body: { project: "p-app", as_of: 1, total: 2, rows: [row({ id: "esc-4" }), row({ id: "esc-5" })] } },
    detail: { status: 200, body: { escape: log().rows[0], events: [], truncated: false, check: null } },
    decide: { status: 200, body: log().rows[0] },
    gap: { status: 200, body: log().rows[0] },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});
afterEach(() => {
  vi.unstubAllGlobals();
});

/** A ResizeObserver that reports one width as soon as an element is watched, as a laid-out browser does. */
function stubWidth(width: number) {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private readonly callback: (entries: unknown[]) => void) {}
      observe() {
        this.callback([{ contentRect: { width } }]);
      }
      disconnect() {}
      unobserve() {}
    },
  );
}

const tab = (name: RegExp | string) => screen.getByRole("tab", { name });

describe("the log's Settings link", () => {
  it("opens the escape settings, and draws at the full width, in a 640 px window and in the 320 px panel", async () => {
    for (const draw of [() => undefined, () => stubWidth(600)]) {
      draw();
      const view = render(<LiveEscapes view="log" />);
      expect(await screen.findByRole("link", { name: "Settings" })).toHaveAttribute("href", "#/settings/crew/escapes/settings");
      view.unmount();
    }
    render(<LiveEscapes view="log" panel />);
    expect(await screen.findByRole("link", { name: "Settings" })).toHaveAttribute("href", "#/settings/crew/escapes/settings");
    expect(screen.getByRole("link", { name: "Coverage" })).toBeInTheDocument();
  });

  it("sits after Coverage and before the project chip, and the possible list has it too", async () => {
    const view = render(<LiveEscapes view="log" />);
    const links = await screen.findAllByRole("link", { name: /Coverage|Settings/ });
    expect(links.map((link) => link.textContent)).toEqual(["Coverage", "Settings"]);
    view.unmount();
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByRole("link", { name: "Settings" })).toHaveAttribute("href", "#/settings/crew/escapes/settings");
    expect(screen.queryByRole("link", { name: "Coverage" })).toBeNull();
  });

  it("only links: nothing is written by drawing the header", async () => {
    render(<LiveEscapes view="log" />);
    await screen.findByRole("tablist", { name: "Escape log views" });
    expect(posts(calls)).toEqual([]);
  });
});

describe("the Possible tab strip: All escapes and Possible N", () => {
  it("shows the count the log reads, with All escapes selected on the log and each tab opening its page for the project", async () => {
    render(<LiveEscapes view="log" />);
    const strip = await screen.findByRole("tablist", { name: "Escape log views" });
    await screen.findByRole("tab", { name: "Possible 2" });
    expect(within(strip).getAllByRole("tab").map((entry) => entry.textContent)).toEqual(["All escapes", "Possible 2"]);
    expect(tab("All escapes")).toHaveAttribute("aria-selected", "true");
    expect(tab(/^Possible/)).toHaveAttribute("aria-selected", "false");
    expect(tab("All escapes")).toHaveAttribute("href", "#/settings/crew/escapes?repo=p-app");
    expect(tab(/^Possible/)).toHaveAttribute("href", "#/settings/crew/escapes/possible?repo=p-app");
    expect(tab(/^Possible/)).toHaveAccessibleName("Possible 2");
  });

  it("selects Possible on the possible list, whose count is the log's, never the sum with the confirmed ones", async () => {
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByText(/2 possible escapes\./)).toBeInTheDocument();
    expect(tab(/^Possible/)).toHaveAttribute("aria-selected", "true");
    expect(tab(/^Possible/)).toHaveTextContent("Possible 2");
    expect(tab("All escapes")).toHaveAttribute("aria-selected", "false");
  });

  it("keeps the tab with 0 when none waits, and draws the count in full with its separator when many do", async () => {
    answers.log = { status: 200, body: log({ possible: 0 }) };
    const view = render(<LiveEscapes view="log" />);
    expect(await screen.findByRole("tab", { name: "Possible 0" })).toBeInTheDocument();
    view.unmount();
    answers.log = { status: 200, body: log({ possible: 3310 }) };
    render(<LiveEscapes view="log" />);
    expect(await screen.findByRole("tab", { name: "Possible 3,310" })).toBeInTheDocument();
  });

  it("reads out an ellipsis as a status while the log is read, and the list still opens", async () => {
    answers.log = "never";
    stubFetch(answers, calls);
    render(<LiveEscapes view="possible" />);
    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent("reading");
    expect(tab(/^Possible/)).toHaveAccessibleName("Possible reading");
    expect(tab(/^Possible/)).toHaveTextContent("Possible …reading");
    expect(await screen.findByText(/2 possible escapes\./)).toBeInTheDocument();
  });

  it("says can’t read, with a dash and never a zero, when the log can’t be read", async () => {
    answers.log = { status: 500, body: { code: "unreadable", message: "The escape log on this device can’t be read." } };
    render(<LiveEscapes view="log" />);
    const possible = await screen.findByRole("tab", { name: "Possible can’t read" });
    expect(possible).toHaveTextContent("Possible –can’t read");
    expect(possible).not.toHaveTextContent("0");
    expect(possible).toHaveAttribute("href", "#/settings/crew/escapes/possible?repo=p-app");
  });

  it("draws the same strip in a 640 px window and in the 320 px panel", async () => {
    stubWidth(600);
    const view = render(<LiveEscapes view="possible" />);
    expect(await screen.findByRole("tab", { name: "Possible 2" })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByText("Dashboard")).toBeNull();
    view.unmount();
    vi.unstubAllGlobals();
    stubFetch(answers, calls);
    render(<LiveEscapes view="possible" panel />);
    expect(await screen.findByRole("tab", { name: "Possible 2" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("button", { name: "Open in Crew" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getAllByText(/Confirm$/).length).toBeGreaterThan(0);
  });
});

describe("the possible list when nothing waits", () => {
  it("says so as a neutral state with what would put an escape there, and draws no card, no Confirm and no confirmed table", async () => {
    answers.possible = { status: 200, body: { project: "p-app", as_of: 1, total: 0, rows: [] } };
    answers.log = { status: 200, body: log({ possible: 0 }) };
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByText("Nothing waits for you")).toBeInTheDocument();
    expect(screen.getByText(/No escape is waiting for a decision in the last 60 days\. A revert appears here at once as a possible escape; so can a later fix or a failed check Crew notices\. Each stays here until you confirm or dismiss it\./)).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryByRole("button", { name: "Confirm" })).toBeNull();
    expect(screen.queryByText(/Confirmed escapes/)).toBeNull();
    expect(tab(/^Possible/)).toHaveAccessibleName("Possible 0");
  });

  it("leaves the window out of the sentence when the log can’t say it, rather than guessing one", async () => {
    answers.possible = { status: 200, body: { project: "p-app", as_of: 1, total: 0, rows: [] } };
    answers.log = { status: 500, body: { code: "unreadable", message: "x" } };
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByText(/^No escape is waiting for a decision\. A revert appears here/)).toBeInTheDocument();
  });

  it("a decision reads the count again, so the tab never keeps a number that was acted on", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const user = userEvent.setup();
    render(<LiveEscapes view="possible" />);
    const card = (await screen.findAllByText("Confirm"))[0]!;
    await user.click(card);
    await vi.waitFor(() => expect(calls.filter(({ url }) => /\/api\/crew\/escapes\?project=p-app$/.test(url)).length).toBe(2));
    expect(posts(calls)).toHaveLength(1);
  });
});
