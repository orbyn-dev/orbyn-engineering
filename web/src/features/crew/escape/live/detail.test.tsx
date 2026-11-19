import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../../product/state/workspace";
import { LiveEscapes } from "./LiveEscapes";
import { bodyOf, detail, log, post, posts, project, row, stubFetch, type Answer, type Call } from "./support";

let calls: Call[] = [];
let answers: { preview: Answer; projects: Answer; log: Answer; logConfirmed: Answer; possible: Answer; detail: Answer; decide: Answer; gap: Answer; };

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

describe("one escape's own page", () => {
  it("draws the trail and the two boxes, and the check it has", async () => {
    answers.detail = {
      status: 200,
      body: detail({
        escape: row({ state: "confirmed", gap: "test_gap", source: "person_marked", reason: "The legacy login route lost its rate limit" }),
        events: [
          { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "person:40", at: 1_762_500_000 },
          { id: 2, kind: "confirmed", from_state: "possible", to_state: "confirmed", decided_by: "person", cause: "person:41", at: 1_762_501_000 },
          { id: 3, kind: "reclassified", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:42", at: 1_762_502_000 },
          { id: 4, kind: "check_proposed", from_state: "confirmed", to_state: "confirmed", decided_by: "person", cause: "person:43", at: 1_762_503_000 },
          { id: 5, kind: "check_proven", from_state: "confirmed", to_state: "confirmed", decided_by: "core", cause: "core:task-9:1", at: 1_762_504_000 },
          { id: 6, kind: "check_merged", from_state: "confirmed", to_state: "confirmed", decided_by: "core", cause: "core:task-9:1", at: 1_762_505_000 },
        ],
        check: { state: "merged", path: "tests/auth/return-to.test.ts", asserts: "An old returnTo link still redirects to its path", run: "npm test -- tests/auth/return-to.test.ts", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", task_id: "task-9", proof: { escaped_commit: "9be4410a1b2c3d4e5f60718293a4b5c6d7e8f90a", fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a", fails_on_escape: true, passes_on_fix: true }, note: null, attempts: 1, diff_commit: "d1c0b2a3", diff_hash: "a".repeat(64), diff_paths: ["tests/auth/return-to.test.ts"], awaiting_approval: false, approved: true, line: "The check is merged and runs before every merge.", proposed_unix: 1_762_503_000, updated_unix: 1_762_505_000 },
      }),
    };
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Detected: you marked it")).toBeInTheDocument();
    expect(screen.getByText("Confirmed by your mark")).toBeInTheDocument();
    expect(screen.getByText(/Gap set: Test gap/)).toBeInTheDocument();
    expect(screen.getByText("Check proposed")).toBeInTheDocument();
    expect(screen.getAllByText("Check merged").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("tests/auth/return-to.test.ts")).toBeInTheDocument();
    expect(screen.getByText(/Fails on 9be4410: yes · Passes on e41a9c0: yes/)).toBeInTheDocument();
    expect(screen.getByText("The check is merged and runs before every merge.")).toBeInTheDocument();
    expect(await screen.findByText(/2 confirmed · 1 possible · 1 dismissed, of /)).toBeInTheDocument();
  });

  it("a possible escape offers Confirm and Dismiss, and a Confirm is the person's own write", async () => {
    answers.detail = { status: 200, body: detail({ escape: row({ state: "possible" }) }) };
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Gap not set")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Confirm" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toContain("/api/crew/escapes/esc-1/confirm");
    expect(bodyOf(post(calls).init)).toEqual({ project: "p-app", seen: { state: "possible", gap: null } });
  });

  it("a confirmed escape offers no decision, and Propose a check is Planned, never sent", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText(/Set the gap…/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Confirm" })).toBeNull();
    expect(screen.getByText(/The escape is confirmed and its gap is a test gap, or not set yet/)).toBeInTheDocument();
    const propose = screen.getByRole("button", { name: "Propose a check" });
    expect(propose).toBeDisabled();
    expect(propose).toHaveAttribute("aria-description", expect.stringContaining("separate pull request"));
    await user.click(propose).catch(() => undefined);
    expect(posts(calls)).toHaveLength(0);
  });

  it("the gap dialog sends the person's own choice with what was seen", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Set the gap…" }));
    expect(await screen.findByText(/What kind of gap was it\?/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Tooling gap/ }));
    answers.detail = { status: 200, body: detail({ escape: row({ state: "confirmed", gap: "tooling_gap", gap_by: "person" }) }) };
    await user.click(screen.getByRole("button", { name: /^Set the gap$/ }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toContain("/api/crew/escapes/esc-1/gap");
    expect(bodyOf(post(calls).init)).toEqual({ project: "p-app", seen: { state: "confirmed", gap: null }, gap: "tooling_gap" });
    expect(await screen.findByText(/Why a test can’t help/)).toBeInTheDocument();
  });

  it("a stale gap refusal is said in the service's own words and the page is read again", async () => {
    answers.gap = { status: 409, body: { code: "stale", message: "This escape changed since you opened it." } };
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Set the gap…" }));
    await user.click(screen.getByRole("button", { name: /^Set the gap$/ }));
    expect(await screen.findByText(/This escape changed since you opened it\. Look again\./)).toBeInTheDocument();
    await waitFor(() => expect(calls.filter(({ url }) => /\/api\/crew\/escapes\/esc-1$/.test(url))).toHaveLength(2));
  });

  it("a tooling gap draws why a test can’t help, and no proposal", async () => {
    answers.detail = { status: 200, body: detail({ escape: row({ state: "confirmed", gap: "tooling_gap", gap_by: "person" }) }) };
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText(/Why a test can’t help/)).toBeInTheDocument();
    expect(screen.getByText(/no check is proposed for a tooling gap/)).toBeInTheDocument();
    expect(screen.getByText(/Add the rule yourself/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Propose a check" })).toBeNull();
  });

  it("a dismissed escape is final: no act is offered, and the boxes say so", async () => {
    answers.detail = {
      status: 200,
      body: detail({
        escape: row({ state: "dismissed", source: "post_merge_check", decided_unix: 1_762_501_000, decided_by: "person", reason: "A repository check failed on this device after the merge, and the merge was not reverted." }),
        events: [
          { id: 1, kind: "detected", from_state: null, to_state: "possible", decided_by: "core", cause: "post_merge_check:5a90e1b", at: 1_762_500_000 },
          { id: 2, kind: "dismissed", from_state: "possible", to_state: "dismissed", decided_by: "person", cause: "person:44", at: 1_762_501_000 },
        ],
      }),
    };
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Dismissed by you")).toBeInTheDocument();
    expect(screen.getByText(/Why Crew thought so/)).toBeInTheDocument();
    expect(screen.getByText("No: a dismissed escape is not an escape")).toBeInTheDocument();
    expect(screen.getByText("Can’t be set on a dismissed escape")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Set the gap/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "Confirm" })).toBeNull();
  });

  it("names no escape when the frame is opened without one", async () => {
    render(<LiveEscapes view="detail" id="" />);
    expect(await screen.findByText("No escape is named")).toBeInTheDocument();
    expect(calls.some(({ url }) => url.includes("/api/crew/escapes/"))).toBe(false);
  });

  it("fails closed when the escape cannot be read", async () => {
    answers.detail = { status: 500, body: { code: "unreadable", message: "The escape log on this device can’t be read." } };
    render(<LiveEscapes view="detail" id="esc-1" />);
    expect(await screen.findByText("Can’t read the escape")).toBeInTheDocument();
  });
});
