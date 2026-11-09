import { describe, expect, it } from "vitest";
import type { PullListEntry } from "../../../api/types";
import { activeChips, bodyOpensWith, detailsKey, emptyColumns, checksOf, checksView, mergeGate, noFilters, pageState, takeLabels, statesToAsk, visiblePulls, yourReview, type PullSort } from "./pullModel";

const newest: PullSort = { by: "created", dir: "newest" };
const oldest: PullSort = { by: "created", dir: "oldest" };

const entry = (number: number, over: Partial<PullListEntry["pull"]> = {}, project = "p"): PullListEntry => ({
  project_id: project,
  pull: {
    number,
    title: `PR ${number}`,
    url: "",
    state: "open",
    head: "h",
    base: "main",
    author: null,
    draft: false,
    updated_at: null,
    labels: [],
    review_state: null,
    checks_state: null,
    ...over,
  },
});

describe("pull list", () => {
  it("takes finished label: words out of the search", () => {
    expect(takeLabels("fix label:Bug login", true)).toEqual({ labels: ["bug"], rest: "fix login" });
    expect(takeLabels("fix label:Bug", false)).toEqual({ labels: [], rest: "fix label:Bug" });
    expect(takeLabels("label:Bug ", false)).toEqual({ labels: ["bug"], rest: "" });
  });

  it("filters and sorts on what the forge reports", () => {
    const list = [entry(1, { draft: true }), entry(2, { checks_state: "failing" }), entry(3, { labels: ["Bug"] }, "q")];
    const numbers = (filters = noFilters, labels: string[] = [], sort = newest) => visiblePulls(list, filters, labels, sort).map((e) => e.pull.number);
    expect(numbers({ ...noFilters, draft: "hide" })).toEqual([3, 2]);
    expect(numbers({ ...noFilters, checks: ["failing"] })).toEqual([2]);
    expect(numbers({ ...noFilters, labels: ["bug"] })).toEqual([3]);
    expect(numbers(noFilters, ["bug"], oldest)).toEqual([3]);
    expect(numbers({ ...noFilters, project: "q" }, [], oldest)).toEqual([3]);
    expect(numbers(noFilters, [], oldest)).toEqual([1, 2, 3]);
  });

  it("keeps a pull request that matches any chosen review or check", () => {
    const list = [entry(1, { review_state: "approved" }), entry(2, { review_state: "changes_requested", checks_state: "pending" }), entry(3)];
    const numbers = (filters: Parameters<typeof visiblePulls>[1]) => visiblePulls(list, filters, [], newest).map((e) => e.pull.number);
    expect(numbers({ ...noFilters, review: ["approved", "changes_requested"] })).toEqual([2, 1]);
    expect(numbers({ ...noFilters, checks: ["running"] })).toEqual([2]);
  });

  it("limits Me to the viewer's login", () => {
    const list = [entry(1, { author: "ada" }), entry(2, { author: "grace" })];
    expect(visiblePulls(list, { ...noFilters, me: true }, [], newest, ["grace"]).map((e) => e.pull.number)).toEqual([2]);
  });

  it("asks the service for each chosen state, or all of them at once", () => {
    expect(statesToAsk(["open", "merged"])).toEqual(["open", "merged"]);
    expect(statesToAsk(["open", "closed", "merged"])).toEqual(["all"]);
  });

  it("never counts the defaults as active filters", () => {
    expect(activeChips(noFilters)).toEqual([]);
    expect(activeChips({ ...noFilters, review: ["approved"], checks: ["failing"] }).map((chip) => chip.label)).toEqual([
      "Review: approved",
      "Checks: failing",
    ]);
  });

  it.each([
    ["pass", "pass"],
    ["passing", "pass"],
    ["fail", "fail"],
    ["failing", "fail"],
    ["pending", "pending"],
    ["none", "none"],
    [null, "none"],
    ["something new", "none"],
  ] as const)("reads check state %j as %s", (state, word) => {
    expect(checksOf(state)).toBe(word);
  });
});

describe("what the cells say", () => {
  const counts = (over: Partial<NonNullable<PullListEntry["pull"]["checks"]>>) => ({
    ...entry(1, { checks: { total: 4, passing: 0, failing: 0, running: 0, failing_names: [], ...over } }).pull,
  });
  it.each([
    [{ failing: 1, passing: 3, failing_names: ["e2e (macOS)"] }, "Failing · e2e (macOS)"],
    [{ failing: 3, passing: 1, failing_names: ["e2e (macOS)", "e2e (Linux)", "docs"] }, "Failing · e2e (macOS) +2"],
    [{ failing: 1, running: 2, passing: 1, failing_names: ["lint"] }, "Failing · lint"],
    [{ running: 2, passing: 1 }, "Running · 2 of 4"],
    [{ passing: 4 }, "Passing"],
    [{ passing: 2 }, "Passing · 2 skipped"],
    [{ passing: 0 }, "No checks ran"],
    [{ total: 0 }, "No checks"],
  ])("words the checks %j as %s", (over, label) => {
    expect(checksView(counts(over))?.label).toBe(label);
  });

  it("lists failing names in the tooltip, then how many more", () => {
    const names = Array.from({ length: 20 }, (_, at) => `c${at}`);
    expect(checksView(counts({ failing: 25, failing_names: names }))?.tip?.lines.at(-1)).toBe("and 5 more");
  });

  it("says nothing when the list carries no counts", () => {
    expect(checksView({ ...entry(1).pull, checks: undefined })).toBeNull();
  });

  it.each([
    ["approved", "You approved"],
    ["changes_requested", "You requested changes"],
    ["commented", "You commented"],
    ["dismissed", "Your review was dismissed"],
    ["pending", "Finish your review"],
  ])("reads the viewer's review %s as %s", (state, label) => {
    expect(yourReview({ project_id: "p", number: 1, viewer_review_state: state }, [])?.label).toBe(label);
  });

  it("offers Review requested only once the signed-in login is known", () => {
    const asked = { project_id: "p", number: 1, viewer_review_state: "none", requested_reviewers: [{ name: "Me", kind: "user" }] };
    expect(yourReview(asked, [])).toBeNull();
    expect(yourReview(asked, ["me"])?.label).toBe("Review requested");
  });
});

describe("filters that need details", () => {
  const withDetails = new Map([
    ["p:1", { project_id: "p", number: 1, requested_reviewers: [{ name: "me", kind: "user" }], comment_count: 2 }],
    ["p:2", { project_id: "p", number: 2, requested_reviewers: [{ name: "sam", kind: "user" }], comment_count: 9 }],
  ]) as Parameters<typeof visiblePulls>[5];
  const list = [entry(1), entry(2), entry(3)];
  const numbers = (filters: Parameters<typeof visiblePulls>[1], sort = newest) => visiblePulls(list, filters, [], sort, ["me"], withDetails).map((e) => e.pull.number);

  it("keeps what asks the viewer for review, and keeps rows with no details", () => {
    expect(numbers({ ...noFilters, review: ["review_required"] })).toEqual([3, 1]);
  });
  it("keeps what asks any chosen person or team, and rows with no details", () => {
    expect(numbers({ ...noFilters, requestedFrom: ["user:sam"] })).toEqual([3, 2]);
  });
  it("sorts by comments with the unknown last, whichever way it runs", () => {
    expect(numbers(noFilters, { by: "comments", dir: "newest" })).toEqual([2, 1, 3]);
    expect(numbers(noFilters, { by: "comments", dir: "oldest" })).toEqual([1, 2, 3]);
  });
  it("sorts by the creation time the details bring, the number breaking a tie, and rows without one last", () => {
    const created = new Map([
      ["p:1", { project_id: "p", number: 1, created_at: "2026-10-01T00:00:00Z" }],
      ["p:2", { project_id: "p", number: 2, created_at: "2026-10-01T00:00:00Z" }],
    ]) as Parameters<typeof visiblePulls>[5];
    const order = (sort: PullSort) => visiblePulls(list, noFilters, [], sort, [], created).map((e) => e.pull.number);
    expect(order(newest)).toEqual([2, 1, 3]);
    expect(order(oldest)).toEqual([1, 2, 3]);
  });
  it("names its chips", () => {
    expect(activeChips({ ...noFilters, review: ["review_required"], requestedFrom: ["team:orbyn-dev/web"] }).map((chip) => chip.label)).toEqual([
      "Review: needs my review",
      "Review requested from: @orbyn-dev/web",
    ]);
  });
});

describe("the pull request page", () => {
  const pull = (over: Partial<PullListEntry["pull"]> = {}) => entry(1, over).pull;
  it.each([
    [{ state: "merged" }, "mergeable", false, "merged"],
    [{ state: "closed" }, "mergeable", false, "closed"],
    [{ draft: true }, "mergeable", false, "draft"],
    [{ review_state: "approved" }, "conflicting", false, "conflict"],
    [{ checks_state: "fail" }, "mergeable", false, "failed"],
    [{ review_state: "changes_requested" }, "mergeable", false, "changes"],
    [{ review_state: "review_required" }, "mergeable", true, "review"],
    [{ review_state: "approved", checks_state: "pass" }, "mergeable", false, "approved"],
    [{ checks_state: "pending" }, "mergeable", false, "running"],
  ] as const)("opens %j in the %s state", (over, mergeable, asked, state) => {
    expect(pageState(pull(over as Partial<PullListEntry["pull"]>), mergeable, asked)).toBe(state);
  });

  it.each([
    [{ state: "merged" }, "mergeable", false, false, "Nothing left to do on GitHub."],
    [{ state: "closed" }, "mergeable", false, false, "Reopen to merge it."],
    [{ draft: true }, "mergeable", false, false, "Mark it ready for review to merge."],
    [{}, "mergeable", true, false, "Merge needs a connection."],
    [{}, "conflicting", false, false, "Resolve the conflict to merge."],
    [{ checks_state: "fail" }, "mergeable", false, false, "Merge is blocked by a failing check."],
    [{ checks_state: "pending" }, "mergeable", false, false, "Merge unlocks when checks pass."],
    [{ review_state: "review_required" }, "mergeable", false, false, "Merge needs an approval."],
    [{ review_state: "approved" }, "mergeable", false, true, "No conflicts"],
  ] as const)("gates Merge for %j: enabled %s, saying %s", (over, mergeable, offline, enabled, hint) => {
    expect(mergeGate(pull(over as Partial<PullListEntry["pull"]>), mergeable, { offline, auto: false, forge: "GitHub" })).toMatchObject({ enabled, hint });
  });

  it("lets Merge wait for checks once auto-merge is chosen", () => {
    expect(mergeGate(pull({ checks_state: "pending" }), "mergeable", { offline: false, auto: true, forge: "GitHub" }).enabled).toBe(true);
  });
});

describe("bodyOpensWith", () => {
  it("is true for a first heading of the same text at any level and in any case", () => {
    expect(bodyOpensWith("## Summary\n\n- a", "Summary")).toBe(true);
    expect(bodyOpensWith("\n\n# summary #\ntext", "Summary")).toBe(true);
  });
  it("is false for another heading, a heading further down, plain text, or no body", () => {
    expect(bodyOpensWith("## Why\n## Summary", "Summary")).toBe(false);
    expect(bodyOpensWith("Summary of the change", "Summary")).toBe(false);
    expect(bodyOpensWith("## Summaries", "Summary")).toBe(false);
    expect(bodyOpensWith("", "Summary")).toBe(false);
    expect(bodyOpensWith(null, "Summary")).toBe(false);
  });
});

describe("emptyColumns", () => {
  const list = [entry(1), entry(2)];
  const row = (number: number, over: Record<string, unknown> = {}) => [detailsKey("p", number), { project_id: "p", number, comment_count: 0, requested_reviewers: [], viewer_review_state: "none", ...over }] as const;
  const map = (...rows: (readonly [string, unknown])[]) => new Map(rows) as unknown as Parameters<typeof emptyColumns>[1];

  it("leaves out Reviewers and Your review when every row's details are in and none has any", () => {
    expect(emptyColumns(list, map(row(1), row(2)), ["me"])).toEqual({ reviewers: true, review: true });
  });
  it("keeps a column in which any row has data", () => {
    const reviewers = [{ kind: "user", name: "sam" }];
    expect(emptyColumns(list, map(row(1, { requested_reviewers: reviewers }), row(2)), ["me"])).toEqual({ reviewers: false, review: true });
    expect(emptyColumns(list, map(row(1), row(2, { viewer_review_state: "approved" })), ["me"])).toEqual({ reviewers: true, review: false });
  });
  it("keeps both while any row's details are still coming, and with no rows", () => {
    expect(emptyColumns(list, map(row(1)), ["me"])).toEqual({ reviewers: false, review: false });
    expect(emptyColumns([], map(), [])).toEqual({ reviewers: false, review: false });
  });
});
