import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useWorkspace } from "../../../../product/state/workspace";
import { PreviewNav } from "../../shared/preview-nav";
import { LiveEscapes } from "./LiveEscapes";
import { log, posts, project, row, stubFetch, type Answer, type Call } from "./support";

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
    possible: { status: 200, body: { project: "p-app", as_of: 1, total: 0, rows: [] } },
    detail: { status: 200, body: { escape: log().rows[0], events: [], truncated: false, check: null } },
    decide: { status: 200, body: log().rows[0] },
    gap: { status: 200, body: log().rows[0] },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

describe("the escape log on the real routes", () => {
  it("counts the three states apart, never adds them, and shows no rate", async () => {
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText("Escapes")).toBeInTheDocument();
    expect(screen.getByText(/2 possible · 1 dismissed, counted apart and never added/)).toBeInTheDocument();
    expect(screen.getByText(/of 38 merges · last 60 days · Rate: Not measured/)).toBeInTheDocument();
    expect(screen.getByText(/2 · 1 · 1 of 21/)).toBeInTheDocument();
    expect(screen.getByText(/0 · 0 · 0 of 5 · low sample/)).toBeInTheDocument();
    expect(screen.getByText("Last 60 days")).toBeInTheDocument();
  });

  it("names a row by what the read carries, and never invents a merge title", async () => {
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText("Old returnTo links stopped redirecting")).toBeInTheDocument();
    expect(screen.getByText("Reverted")).toBeInTheDocument();
    expect(screen.getAllByText("Confirmed escape")).toHaveLength(2);
    expect(screen.getAllByText(/Confirmed by you,/)).toHaveLength(2);
    expect(screen.getByText("Test gap")).toBeInTheDocument();
    expect(screen.queryByText(/Merged to main/)).toBeNull();
  });

  it("says so in one line when the window holds no escape", async () => {
    answers.log = { status: 200, body: log({ total: 0, possible: 0, confirmed: 0, dismissed: 0, merges: 14, rows: [], engines: [{ engine: "claude", merges: 14, possible: 0, confirmed: 0, dismissed: 0, low_sample: false }] }) };
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText("No escapes in the last 60 days")).toBeInTheDocument();
    expect(screen.getByText(/14 merges, and no possible, confirmed or dismissed escape/)).toBeInTheDocument();
    expect(screen.queryByText("Why it got through")).toBeNull();
  });

  it("flags the whole log low sample when the window holds fewer merges than a rate needs", async () => {
    answers.log = { status: 200, body: log({ merges: 6, low_sample: true, total: 1, possible: 0, confirmed: 1, dismissed: 0, engines: [{ engine: "claude", merges: 4, possible: 0, confirmed: 1, dismissed: 0, low_sample: true }], rows: [row({ state: "confirmed", decided_unix: 1_762_501_000, decided_by: "person" })] }) };
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText(/of 6 merges · last 60 days · low sample · Rate: Not measured/)).toBeInTheDocument();
  });

  it("fails closed on an unreadable log: dashes, no numbers, and a retry reads again", async () => {
    answers.log = { status: 500, body: { code: "unreadable", message: "The escape log on this device can’t be read." } };
    const user = userEvent.setup();
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText(/Can’t read escapes/)).toBeInTheDocument();
    expect(screen.getAllByText("—")).toHaveLength(4);
    expect(screen.getAllByText(/Not shown while the log can’t be read/)).toHaveLength(4);
    expect(screen.getByText(/unreadable: The escape log on this device can’t be read/)).toBeInTheDocument();
    answers.log = { status: 200, body: log() };
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Escapes")).toBeInTheDocument();
    expect(calls.filter(({ url }) => url.startsWith("/api/crew/escapes?"))).toHaveLength(2);
  });

  it("draws nothing and reads no escape route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LiveEscapes view="log" />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/escapes"))).toBe(false);
  });

  it("loads the project list itself when the shell has not (a Preview hash opens before the workspace loads)", async () => {
    useWorkspace.setState({ projects: { data: null, loading: false, error: null } });
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText("Escapes")).toBeInTheDocument();
    expect(calls.some(({ url }) => url.startsWith("/api/projects"))).toBe(true);
  });

  it("sends no write to draw: only the reads ran", async () => {
    render(<LiveEscapes view="log" />);
    await screen.findByText("Escapes");
    expect(posts(calls)).toHaveLength(0);
  });
});

/** Six escapes, as the design's sample page holds: enough for the panel's four-card limit to show. */
const sixRows = () =>
  Array.from({ length: 6 }, (_, index) =>
    row({
      id: `esc-${index}`,
      state: index < 3 ? "confirmed" : "possible",
      reason: `Escape number ${index}`,
      decided_unix: index < 3 ? 1_762_501_000 : null,
      decided_by: index < 3 ? "person" : null,
    }),
  );

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

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("the narrow log (Crew Designs v79 escape-narrow-window and escape-narrow-panel)", () => {
  it("draws the 640 px window as two tiles and a card per escape, never the table", async () => {
    stubWidth(600);
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText("Escapes")).toBeInTheDocument();
    expect(screen.getByText("Rate")).toBeInTheDocument();
    expect(screen.getByText("Not measured")).toBeInTheDocument();
    expect(screen.getByText(/None in v1/)).toBeInTheDocument();
    expect(screen.getByText(/2 possible · 1 dismissed, apart/)).toBeInTheDocument();
    expect(screen.getByText(/Of 38 merges · last 60 days/)).toBeInTheDocument();
    expect(screen.getByText("orbyn-engineering")).toBeInTheDocument();
    expect(screen.queryByText("When found")).toBeNull();
    expect(screen.getByText("Old returnTo links stopped redirecting")).toBeInTheDocument();
    expect(screen.getByText(/Showing 3 of 6 · newest found first/)).toBeInTheDocument();
  });

  it("draws the 320 px panel as one counts card and the first four escapes, cards only", async () => {
    answers.log = { status: 200, body: log({ rows: sixRows() }) };
    render(<LiveEscapes view="log" panel />);
    expect(await screen.findByText("3 confirmed")).toBeInTheDocument();
    expect(screen.getByText(/2 possible · 1 dismissed, counted apart/)).toBeInTheDocument();
    expect(screen.getByText(/No rate: Not measured/)).toBeInTheDocument();
    expect(screen.getByText("Escape number 3")).toBeInTheDocument();
    expect(screen.queryByText("Escape number 4")).toBeNull();
    expect(screen.getByText(/Showing 4 of 6. The table, the timeline of an escape and its check are behind Open in Crew./)).toBeInTheDocument();
    expect(screen.queryByText("Escapes")).toBeNull();
    expect(screen.queryByText("When found")).toBeNull();
    expect(screen.queryByText("orbyn-engineering")).toBeNull();
  });

  it("goes narrow by its own measured width, and stays the full page at 640 px or more", async () => {
    stubWidth(640);
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText("When found")).toBeInTheDocument();
    expect(screen.queryByText("Rate")).toBeNull();
  });

  it("draws Open in Crew on the panel only, and it only navigates", async () => {
    // The panel is a Preview surface: the Preview route's links open the Preview frames.
    render(<PreviewNav><LiveEscapes view="log" panel /></PreviewNav>);
    await screen.findByText("3 confirmed");
    await userEvent.click(screen.getByRole("button", { name: "Open in Crew" }));
    expect(window.location.hash).toBe("#/preview/crew/escape-log");
    expect(posts(calls)).toHaveLength(0);
  });

  it("carries the Coverage link at every width, before the project chip: in the header, with the chip in a window, on its own line in the panel", async () => {
    const { unmount } = render(<LiveEscapes view="log" />);
    await screen.findByText("What counts as an escape");
    const chip = document.querySelector(".cx-t10sel") as HTMLElement;
    const full = screen.getByRole("link", { name: "Coverage" });
    expect(full).toHaveAttribute("href", "#/settings/crew/capacity/coverage?repo=p-app");
    expect(full.compareDocumentPosition(chip) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    unmount();
    stubWidth(600);
    const window600 = render(<LiveEscapes view="log" />);
    await screen.findByText(/newest found first/);
    expect(screen.getByRole("link", { name: "Coverage" }).parentElement).toContainElement(document.querySelector(".cx-t10sel") as HTMLElement);
    window600.unmount();
    render(<PreviewNav><LiveEscapes view="log" panel /></PreviewNav>);
    await screen.findByText("3 confirmed");
    expect(screen.getByRole("link", { name: "Coverage" })).toHaveAttribute("href", "#/preview/crew/capacity-live-coverage");
    expect(posts(calls)).toHaveLength(0);
  });

  it("keeps a narrow unreadable log to one dash card, never four tiles", async () => {
    stubWidth(600);
    answers.log = { status: 500, body: { code: "unreadable", message: "The escape log on this device can’t be read." } };
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText(/Can’t read escapes/)).toBeInTheDocument();
    expect(screen.getAllByText("—")).toHaveLength(1);
    expect(screen.getAllByText(/Not shown while the log can’t be read/)).toHaveLength(1);
  });

  it("reads the log once and sends no write, at either narrow width", async () => {
    stubWidth(600);
    render(<LiveEscapes view="log" panel />);
    await screen.findByText("3 confirmed");
    render(<LiveEscapes view="log" />);
    await screen.findByText("Rate");
    expect(calls.filter(({ url }) => url.startsWith("/api/crew/escapes?"))).toHaveLength(2);
    expect(posts(calls)).toHaveLength(0);
  });
});


