import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { LiveEscapeSettings } from "./LiveEscapeSettings";
import { detail, log, possible, posts, project, stubFetch, type Answer, type Call } from "./support";

const defaults = { report_days: 60, follow_up_days: 14, min_merges: 10, rate_multiple: 2, stored: false };

let calls: Call[] = [];
let answers: { preview: Answer; projects: Answer; log: Answer; logConfirmed: Answer; possible: Answer; detail: Answer; decide: Answer; gap: Answer; thresholds: Answer };

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    log: { status: 200, body: log() },
    logConfirmed: { status: 200, body: log() },
    possible: { status: 200, body: possible() },
    detail: { status: 200, body: detail() },
    decide: { status: 200, body: log().rows[0] },
    gap: { status: 200, body: log().rows[0] },
    thresholds: { status: 200, body: defaults },
  };
  stubFetch(answers, calls);
});

describe("the escape constants on the real thresholds read", () => {
  it("reads the four constants and marks each one default when nobody stored values", async () => {
    render(<LiveEscapeSettings />);
    expect(await screen.findByText("60 days · default")).toBeInTheDocument();
    expect(screen.getByText("14 days · default")).toBeInTheDocument();
    expect(screen.getByText("10 merges · default")).toBeInTheDocument();
    expect(screen.getByText("2× · default")).toBeInTheDocument();
    expect(screen.getByText("Ask me each time · default")).toBeInTheDocument();
    expect(screen.getByText("Not used in v1 · Proposed")).toBeInTheDocument();
    expect(screen.getAllByText("Planned")).not.toHaveLength(0);
  });

  it("shows a person's stored values with no default marker", async () => {
    answers.thresholds = { status: 200, body: { report_days: 30, follow_up_days: 21, min_merges: 20, rate_multiple: 3, stored: true } };
    render(<LiveEscapeSettings />);
    expect(await screen.findByText("30 days")).toBeInTheDocument();
    expect(screen.getByText("21 days")).toBeInTheDocument();
    expect(screen.getByText("20 merges")).toBeInTheDocument();
    expect(screen.getByText("3×")).toBeInTheDocument();
    expect(screen.queryByText("60 days · default")).toBeNull();
    expect(screen.queryByText("14 days · default")).toBeNull();
    expect(screen.queryByText("10 merges · default")).toBeNull();
    expect(screen.queryByText("2× · default")).toBeNull();
  });

  it("sends no write to draw: only the read ran", async () => {
    render(<LiveEscapeSettings />);
    await screen.findByText("Crew › Escapes");
    expect(posts(calls)).toHaveLength(0);
    expect(calls.filter(({ url }) => url === "/api/crew/escapes/thresholds")).toHaveLength(1);
  });

  it("fails closed on an unreadable document: no constants, and a retry reads again", async () => {
    answers.thresholds = { status: 500, body: { code: "unreadable", message: "The escape log on this device can’t be read." } };
    const user = userEvent.setup();
    render(<LiveEscapeSettings />);
    expect(await screen.findByText("Can’t read the escape constants")).toBeInTheDocument();
    expect(screen.queryByText("60 days · default")).toBeNull();
    answers.thresholds = { status: 200, body: defaults };
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("60 days · default")).toBeInTheDocument();
    expect(calls.filter(({ url }) => url === "/api/crew/escapes/thresholds")).toHaveLength(2);
  });

  it("carries the Designer's rules: ask each time, the proof, no undo, the Proposed tie-break, no cost", async () => {
    render(<LiveEscapeSettings />);
    await screen.findByText("60 days · default");
    expect(screen.getByText(/Nothing runs until you accept it, under every mode including Full/)).toBeInTheDocument();
    expect(screen.getAllByText("Planned").length).toBeGreaterThan(0);
    expect(screen.getByText(/fails on the escaped merge and passes on the fix or revert/)).toBeInTheDocument();
    expect(screen.getByText(/You confirmed it, or you marked the merge\. Final: no undo in v1/)).toBeInTheDocument();
    expect(screen.getByText("Not used in v1 · Proposed")).toBeInTheDocument();
    expect(screen.getByText("Not part of the record")).toBeInTheDocument();
    expect(document.body.textContent ?? "").not.toMatch(/\$\d/);
  });

  it("draws nothing and reads no escape route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: { code: "not_found", message: "Not found" } };
    const { container } = render(<LiveEscapeSettings />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/escapes"))).toBe(false);
  });
});
