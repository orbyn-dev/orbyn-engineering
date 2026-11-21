import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../../product/state/workspace";
import { LiveEscapes } from "./LiveEscapes";
import { bodyOf, log, post, posts, possible, project, row, stubFetch, type Answer, type Call } from "./support";

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
  const confirmed = log({ total: 3, possible: 0, rows: log().rows.filter((one) => one.state === "confirmed") });
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    log: { status: 200, body: confirmed },
    logConfirmed: { status: 200, body: confirmed },
    possible: { status: 200, body: possible() },
    detail: { status: 200, body: { escape: log().rows[0], events: [], truncated: false, check: null } },
    decide: { status: 200, body: row({ state: "confirmed" }) },
    gap: { status: 200, body: row({ state: "confirmed" }) },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

describe("possible escapes, and the two decisions that are a person’s alone", () => {
  it("draws each card's why in one sentence and keeps the counts below", async () => {
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByText(/2 possible escapes\./)).toBeInTheDocument();
    expect(screen.getByText(/A later fix commit changed lines this merge added/)).toBeInTheDocument();
    expect(screen.getAllByText(/It becomes a confirmed escape and counts against/)).toHaveLength(2);
    expect(screen.getByText(/Confirmed escapes/)).toBeInTheDocument();
    expect(screen.getByText(/3 in all/)).toBeInTheDocument();
  });

  it("a Confirm click sends the person's own write with what was seen, and reads again", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="possible" />);
    const card = (await screen.findByText(/It may be ordinary follow-up work/)).closest("div.cx-fbx") as HTMLElement;
    await user.click(within(card).getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toContain("/api/crew/escapes/esc-4/confirm");
    expect(bodyOf(post(calls).init)).toEqual({ project: "p-app", seen: { state: "possible", gap: null } });
    await waitFor(() => expect(calls.filter(({ url: at }) => at.includes("/api/crew/escapes/possible"))).toHaveLength(2));
  });

  it("a Dismiss click is the person's own write too", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="possible" />);
    const card = (await screen.findByText(/It may be ordinary follow-up work/)).closest("div.cx-fbx") as HTMLElement;
    await user.click(within(card).getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toContain("/api/crew/escapes/esc-4/dismiss");
  });

  it("a stale refusal is said in the service's own words, and the lists are read again", async () => {
    answers.decide = { status: 409, body: { code: "stale", message: "This escape changed since you opened it." } };
    const user = userEvent.setup();
    render(<LiveEscapes view="possible" />);
    const card = (await screen.findByText(/It may be ordinary follow-up work/)).closest("div.cx-fbx") as HTMLElement;
    await user.click(within(card).getByRole("button", { name: "Confirm" }));
    expect(await screen.findByText(/This escape changed since you opened it\. Look again\./)).toBeInTheDocument();
    await waitFor(() => expect(calls.filter(({ url }) => url.includes("/api/crew/escapes/possible"))).toHaveLength(2));
  });

  it("says so when the possible read itself fails, and a retry reads again", async () => {
    answers.possible = { status: 500, body: { code: "unreadable", message: "The escape log on this device can’t be read." } };
    const user = userEvent.setup();
    render(<LiveEscapes view="possible" />);
    expect(await screen.findByText(/Can’t read the possible escapes on this device/)).toBeInTheDocument();
    answers.possible = { status: 200, body: possible() };
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText(/2 possible escapes\./)).toBeInTheDocument();
  });

  it("sends no write to draw: only the reads ran", async () => {
    render(<LiveEscapes view="possible" />);
    await screen.findByText(/2 possible escapes\./);
    expect(posts(calls)).toHaveLength(0);
  });
});
