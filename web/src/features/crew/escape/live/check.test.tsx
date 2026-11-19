import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../../product/state/workspace";
import type { EscapeCheckRow } from "../../../../generated/EscapeCheckRow";
import { LiveEscapes } from "./LiveEscapes";
import { ESCAPE_LIVE_FRAMES, ESCAPE_LIVE_SCREENS } from "./screens";
import { ESCAPE_FRAMES } from "../names";
import { bodyOf, checkRow, detail, log, post, posts, project, row, stubFetch, type Answer, type Call } from "./support";

/** The check frames of Crew Designs v79: one escape's own page with its check in each state, on the real detail read. */
const CHECK_FRAMES = [
  "escape-check-proposed",
  "escape-check-writing",
  "escape-check-approve",
  "escape-check-proven",
  "escape-check-invalid",
  "escape-check-retry",
  "escape-check-withdraw",
  "escape-check-withdrawn",
  "escape-limit",
] as const;

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
  check?: Answer;
};

/** The escape every check frame is drawn over: confirmed, its gap a test gap, decided by the person. */
const confirmed = (check: EscapeCheckRow) => ({
  status: 200,
  body: detail({ escape: row({ state: "confirmed", gap: "test_gap", gap_by: "person", decided_unix: 1_762_501_000, decided_by: "person" }), check }),
});

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

describe("the check frames of Crew Designs v79", () => {
  it("routes every frame the designs list, and each opens a live screen", () => {
    const frames = ESCAPE_FRAMES as readonly string[];
    const live = ESCAPE_LIVE_FRAMES as readonly string[];
    const screens = ESCAPE_LIVE_SCREENS as Record<string, unknown>;
    for (const id of CHECK_FRAMES) {
      expect(frames, id).toContain(id);
      expect(live, id).toContain(id);
      expect(screens[id], id).toBeDefined();
    }
  });

  it("a proposed check waits for the person: Accept is the only start, and it is their own write", async () => {
    answers.detail = confirmed(checkRow({ state: "proposed" }));
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Check proposed")).toBeInTheDocument();
    expect(screen.getByText("tests/auth/return-to.test.ts")).toBeInTheDocument();
    expect(screen.getByText("It must fail on the escaped merge and pass on e41a9c0. If it passes on both, the check is marked failed and you decide")).toBeInTheDocument();
    expect(screen.getByText(/Only a confirmed escape with a test gap can have a check. A tooling gap has no proposal/)).toBeInTheDocument();
    const accept = screen.getByRole("button", { name: "Accept: start a worker" });
    expect(accept).toBeEnabled();
    answers.check = { status: 200, body: { escape: row({ state: "confirmed", gap: "test_gap" }), check: checkRow({ state: "writing", task_id: "task-check-2" }) } };
    await user.click(accept);
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toContain("/api/crew/escapes/esc-1/check/accept");
    expect(bodyOf(post(calls).init)).toEqual({ project: "p-app", seen: { state: "confirmed", gap: "test_gap", check: "proposed" } });
  });

  it("a writing check shows the worker and offers only Withdraw", async () => {
    answers.detail = confirmed(checkRow({ state: "writing", task_id: "task-check-2" }));
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Check being written")).toBeInTheDocument();
    expect(screen.getByText("Task task-check-2 · writing the test file")).toBeInTheDocument();
    expect(screen.getByText(/Runs when the worker’s change is read: once on the escaped merge, once on e41a9c0/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Withdraw" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: /Accept/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "Approve this change" })).toBeNull();
    expect(posts(calls)).toHaveLength(0);
  });

  it("where the proof cannot be confined, the person approves the exact commit and hash", async () => {
    answers.detail = confirmed(
      checkRow({
        state: "writing",
        task_id: "task-check-2",
        diff_commit: "4f1c9a07d3be5a6c218e0b97f5d2a43c6e81b09d",
        diff_hash: "b3a7f0c95e1d482a6f07c3d9e8b15a4720c6d8f3e91b5a07c4d2f86e30a1b97c",
        diff_paths: ["tests/auth/return-to.test.ts"],
        awaiting_approval: true,
      }),
    );
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Approve the proof")).toBeInTheDocument();
    expect(screen.getByText(/Needs your approval/)).toBeInTheDocument();
    expect(screen.getByText("Writing · waiting for your approval")).toBeInTheDocument();
    expect(screen.getByText("4f1c9a07d3be5a6c218e0b97f5d2a43c6e81b09d")).toBeInTheDocument();
    expect(screen.getByText("b3a7f0c95e1d482a6f07c3d9e8b15a4720c6d8f3e91b5a07c4d2f86e30a1b97c")).toBeInTheDocument();
    expect(screen.getByText("1 file: tests/auth/return-to.test.ts")).toBeInTheDocument();
    expect(screen.getByText(/runs only if you approve exactly this change/)).toBeInTheDocument();
    answers.check = { status: 200, body: { escape: row({ state: "confirmed", gap: "test_gap" }), check: checkRow({ state: "writing", task_id: "task-check-2", awaiting_approval: false, approved: true }) } };
    await user.click(screen.getByRole("button", { name: "Approve this change" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toContain("/api/crew/escapes/esc-1/check/approve");
    expect(bodyOf(post(calls).init)).toEqual({
      project: "p-app",
      seen: { state: "confirmed", gap: "test_gap", check: "writing" },
      commit: "4f1c9a07d3be5a6c218e0b97f5d2a43c6e81b09d",
      diff_hash: "b3a7f0c95e1d482a6f07c3d9e8b15a4720c6d8f3e91b5a07c4d2f86e30a1b97c",
    });
  });

  it("a proven check shows the proof and the queue path, and offers only Withdraw", async () => {
    answers.detail = confirmed(
      checkRow({
        state: "proven",
        task_id: "task-check-2",
        proof: { escaped_commit: "9be4410a1b2c3d4e5f60718293a4b5c6d7e8f90a", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", fails_on_escape: true, passes_on_fix: true },
      }),
    );
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Check proven")).toBeInTheDocument();
    expect(screen.getByText(/Fails on 9be4410: yes · Passes on e41a9c0: yes/)).toBeInTheDocument();
    expect(screen.getByText("Queued like any change, with its own risk tier, holds and required checks")).toBeInTheDocument();
    expect(screen.getByText(/A proof is evidence, not a verdict/)).toBeInTheDocument();
    const queue = screen.getByRole("button", { name: "Open the merge queue" });
    expect(queue).toBeDisabled();
    expect(screen.getByRole("button", { name: "Withdraw" })).toBeEnabled();
    expect(posts(calls)).toHaveLength(0);
  });

  it("a failed check says why in the proof’s own answers and offers Retry and Withdraw", async () => {
    answers.detail = confirmed(
      checkRow({
        state: "failed",
        path: "tests/auth/session.test.ts",
        run: "npm test -- tests/auth/session.test.ts",
        proof: { escaped_commit: "3be5a02a1b2c3d4e5f60718293a4b5c6d7e8f90a", fix_commit: "8e21d47a1b2c3d4e5f60718293a4b5c6d7e8f90a", fails_on_escape: false, passes_on_fix: true },
      }),
    );
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("The check did not prove itself")).toBeInTheDocument();
    expect(screen.getByText(/This one passes on the escaped merge too, so it is marked failed: adding it would give false comfort. Nothing was merged./)).toBeInTheDocument();
    expect(screen.getByText("On 3be5a02 (escaped merge)")).toBeInTheDocument();
    expect(screen.getByText("Passed · it had to fail")).toBeInTheDocument();
    expect(screen.getByText(/Fails on the escape: no · Passes on the fix: yes/)).toBeInTheDocument();
    expect(screen.getByText("The escape stays confirmed, against claude · web/auth, while you decide. A failed check changes no count.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Withdraw" })).toBeEnabled();
    expect(posts(calls)).toHaveLength(0);
  });

  it("Retry asks first, says what it does, and sends what was seen", async () => {
    answers.detail = confirmed(
      checkRow({
        state: "failed",
        path: "tests/auth/session.test.ts",
        run: "npm test -- tests/auth/session.test.ts",
        proof: { escaped_commit: "3be5a02a1b2c3d4e5f60718293a4b5c6d7e8f90a", fix_commit: "8e21d47a1b2c3d4e5f60718293a4b5c6d7e8f90a", fails_on_escape: false, passes_on_fix: true },
      }),
    );
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Retry" }));
    const dialog = screen.getByRole("dialog", { name: "Retry this check?" });
    expect(within(dialog).getByText(/Retry starts nothing: no worker runs until you accept again/)).toBeInTheDocument();
    expect(within(dialog).getByText("tests/auth/session.test.ts")).toBeInTheDocument();
    expect(within(dialog).getByText("Passed on the escaped merge too")).toBeInTheDocument();
    answers.check = { status: 200, body: { escape: row({ state: "confirmed", gap: "test_gap" }), check: checkRow({ state: "proposed", path: "tests/auth/session.test.ts" }) } };
    await user.click(within(dialog).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toContain("/api/crew/escapes/esc-1/check/retry");
    expect(bodyOf(post(calls).init)).toEqual({ project: "p-app", seen: { state: "confirmed", gap: "test_gap", check: "failed" } });
  });

  it("Withdraw asks first, says what it stops and when Crew refuses, and sends what was seen", async () => {
    answers.detail = confirmed(checkRow({ state: "proposed" }));
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Withdraw" }));
    const dialog = screen.getByRole("dialog", { name: "Withdraw this check?" });
    expect(within(dialog).getByText(/a worker still writing is cancelled, a proof still running is stopped and a queued merge of it is turned down/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Crew refuses: “The merge is landing right now. Withdraw the check after it ends.”/)).toBeInTheDocument();
    expect(within(dialog).getByText(/“Already merged: the merge could not be stopped”/)).toBeInTheDocument();
    answers.check = { status: 200, body: { escape: row({ state: "confirmed", gap: "test_gap" }), check: checkRow({ state: "withdrawn" }) } };
    await user.click(within(dialog).getByRole("button", { name: "Withdraw the check" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toContain("/api/crew/escapes/esc-1/check/withdraw");
    expect(bodyOf(post(calls).init)).toEqual({ project: "p-app", seen: { state: "confirmed", gap: "test_gap", check: "proposed" } });
  });

  it("a withdrawn check is final for the check, and proposing again is Planned, never sent", async () => {
    answers.detail = confirmed(checkRow({ state: "withdrawn" }));
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText(/The escape is still confirmed. A new proposal replaces this one./)).toBeInTheDocument();
    const again = screen.getByRole("button", { name: "Propose a check" });
    expect(again).toBeDisabled();
    expect(again).toHaveAttribute("aria-description", expect.stringContaining("create-check screen"));
    await user.click(again).catch(() => undefined);
    expect(posts(calls)).toHaveLength(0);
  });

  it("a stale act is refused in the service’s own words and the page is read again", async () => {
    answers.detail = confirmed(checkRow({ state: "proposed" }));
    answers.check = { status: 409, body: { code: "stale", message: "This check changed since you looked. Reload it and choose again." } };
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Accept: start a worker" }));
    expect(await screen.findByText(/This check changed since you looked. Reload it and choose again. Look again./)).toBeInTheDocument();
    await waitFor(() => expect(calls.filter(({ url }) => /\/api\/crew\/escapes\/esc-1$/.test(url))).toHaveLength(2));
  });
});

describe("the escape log at its limit", () => {
  it("draws the refusal banner and the newest-of heading when the project holds 5,000 escapes", async () => {
    answers.log = { status: 200, body: log({ total: 5000 }) };
    render(<LiveEscapes view="log" />);
    expect(await screen.findByText(/This project has reached the 5,000-escape limit\./)).toBeInTheDocument();
    expect(screen.getByText(/The same holds at 20,000 audit events\. Confirmed escapes are never dropped\./)).toBeInTheDocument();
    expect(screen.getByTitle("Planned: pruning the oldest dismissed and possible escapes is not built; the writer refuses instead")).toBeInTheDocument();
    expect(screen.getByText("Newest 3 of 5,000")).toBeInTheDocument();
    expect(posts(calls)).toHaveLength(0);
  });

  it("draws no banner below the limit", async () => {
    render(<LiveEscapes view="log" />);
    await screen.findByText("Escapes");
    expect(screen.queryByText(/5,000-escape limit/)).toBeNull();
    expect(screen.getByText(/6 in the last 60 days/)).toBeInTheDocument();
  });
});
