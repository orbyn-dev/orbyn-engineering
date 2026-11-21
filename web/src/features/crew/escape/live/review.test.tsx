import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../../product/state/workspace";
import { LiveEscapes } from "./LiveEscapes";
import { detail, log, posts, project, row, stubFetch, type Answer, type Call } from "./support";

let calls: Call[] = [];
let answers: { preview: Answer; projects: Answer; log: Answer; logConfirmed: Answer; possible: Answer; detail: Answer; decide: Answer; gap: Answer };

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    log: { status: 200, body: log() },
    logConfirmed: { status: 200, body: log() },
    possible: { status: 200, body: { project: "p-app", as_of: 1, total: 0, rows: [] } },
    detail: { status: 200, body: detail() },
    decide: { status: 200, body: row({ state: "confirmed" }) },
    gap: { status: 200, body: row({ state: "confirmed", gap: "tooling_gap" }) },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

describe("the log's own words", () => {
  it("labels the total as all escapes and says how many rows are shown", async () => {
    answers.log = { status: 200, body: log({ total: 9 }) };
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText(/9 in all/)).toBeInTheDocument();
    expect(screen.queryByText(/in the last 60 days$/)).toBeNull();
    expect(screen.getByText(/Showing 3 of 9/)).toBeInTheDocument();
  });

  it("shows the low-sample banner with the read's own numbers", async () => {
    answers.log = { status: 200, body: log({ merges: 6, low_sample: true, min_merges: 10 }) };
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText(/Fewer than 10 merges in the last 60 days \(6\)/)).toBeInTheDocument();
  });

  it("shows no banner when the sample is enough", async () => {
    render(<LiveEscapes view="log" />);
    await screen.findByText("Escapes");
    expect(screen.queryByText(/Fewer than/)).toBeNull();
  });

  it("says suggested when Crew set the gap", async () => {
    answers.log = { status: 200, body: log({ rows: [row({ id: "e", state: "confirmed", gap: "test_gap", gap_by: "core", decided_by: "person", decided_unix: 1 })] }) };
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText(/Test gap · suggested/)).toBeInTheDocument();
  });
});

describe("the possible page", () => {
  it("heads the confirmed list with the all-escapes total", async () => {
    answers.logConfirmed = { status: 200, body: log({ confirmed: 1, total: 4, rows: [row({ id: "c", state: "confirmed" })] }) };
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByText(/4 in all/)).toBeInTheDocument();
  });

  it("keeps the section and offers Try again when the confirmed read fails", async () => {
    answers.logConfirmed = { status: 500, body: { code: "unreadable", message: "The escape log on this device can’t be read." } };
    const user = userEvent.setup();
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByText(/Can’t read the confirmed escapes/)).toBeInTheDocument();
    answers.logConfirmed = { status: 200, body: log({ total: 1, rows: [row({ id: "c", state: "confirmed" })] }) };
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText(/1 in all/)).toBeInTheDocument();
  });

  it("says a check is proposed from the confirmed escape's own page, and trails no dot on empty areas", async () => {
    answers.possible = { status: 200, body: { project: "p-app", as_of: 1, total: 1, rows: [row({ id: "p", areas: [] })] } };
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByText(/If you confirm/)).toBeInTheDocument();
    expect(screen.getByText(/You can then set its gap and ask Crew to propose a check/)).toBeInTheDocument();
    expect(screen.queryByText(/not on this page yet/)).toBeNull();
    expect(screen.queryByRole("button", { name: /Propose a check/ })).toBeNull();
    expect(screen.getByText(/^claude · found/)).toBeInTheDocument();
  });
});

describe("the detail's trail and gap", () => {
  it("says the trail is cut when the read says so", async () => {
    answers.detail = { status: 200, body: detail({ truncated: true }) };
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Older events are not listed.")).toBeInTheDocument();
  });

  it("says nothing of a cut trail when it is whole", async () => {
    render(<LiveEscapes view="detail" id="esc-1" />);
    await screen.findByText("Timeline");
    expect(screen.queryByText("Older events are not listed.")).toBeNull();
  });

  it("does not credit the Review queue, and never names the current gap on an earlier row", async () => {
    answers.detail = {
      status: 200,
      body: detail({
        escape: row({ state: "confirmed", gap: "tooling_gap", gap_by: "person", source: "person_marked" }),
        events: [
          { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "person", cause: "person:1", at: 1_762_500_000 },
          { id: 2, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:2", at: 1_762_500_100 },
          { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:3", at: 1_762_500_200 },
        ],
      }),
    };
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText(/You marked this merge\./)).toBeInTheDocument();
    expect(screen.queryByText(/Review queue/)).toBeNull();
    expect(screen.getAllByText("You changed the gap.")).toHaveLength(2);
    expect(screen.queryByText(/No cheap test could catch it\. $/)).toBeNull();
    expect(screen.getAllByText(/Gap set/)).toHaveLength(2);
    expect(screen.getAllByText(/^Gap set: Tooling gap$/)).toHaveLength(1);
  });

  it("offers Set the gap on a possible escape too", async () => {
    answers.detail = { status: 200, body: detail({ escape: row({ state: "possible" }) }) };
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByRole("button", { name: "Set the gap…" })).toBeInTheDocument();
  });

  it("marks a gap Crew suggested as suggested", async () => {
    answers.detail = { status: 200, body: detail({ escape: row({ state: "confirmed", gap: "test_gap", gap_by: "core" }) }) };
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText(/Test gap · suggested/)).toBeInTheDocument();
  });

  it("closes the gap dialog on Escape and keeps it a real modal dialog", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    const open = await screen.findByRole("button", { name: "Set the gap…" });
    await user.click(open);
    const dialog = await screen.findByRole("dialog", { hidden: true });
    expect(dialog.tagName).toBe("DIALOG");
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByText(/What kind of gap was it\?/)).toBeNull());
    expect(posts(calls)).toHaveLength(0);
  });

  it("sends one write on a double click of Set the gap", async () => {
    answers.gap = "never";
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Set the gap…" }));
    const dialog = await screen.findByRole("dialog", { hidden: true });
    const send = within(dialog).getByRole("button", { name: /^Set the gap$/, hidden: true });
    await user.dblClick(send);
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(send).toBeDisabled();
  });
});
