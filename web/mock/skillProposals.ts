import type { SkillProposal, SkillProposalList } from "../src/features/crew/lessons/proposals/types.ts";
import type { MockResponse, Scenario } from "./api.ts";
import { PROPOSAL_LESSONS } from "./lessons.ts";
import type { Lesson } from "../src/features/crew/lessons/types.ts";

/**
 * The skill-proposals mock (KC9): the read and the two Person writes over the design's sample, the same three repeated
 * failures and the same SKILL.md the frames draw. It stands in for the service and ships nowhere: `mock/plugin.ts` loads it
 * in the dev server only, and the production build assertions check that no mock reached the output. Scenarios:
 * "crew-proposals" (one open proposal), "crew-proposals-none" (nothing proposed yet), "crew-proposals-stale" (one of its
 * lessons was turned off, so Approve is off), "crew-proposals-approved" (the skill exists and its lessons carry it),
 * "crew-proposals-secret" (the scan refuses a text holding a key) and "crew-proposals-approve-fails" (the write is
 * refused, as a skills-local permission problem is). Every other scenario answers an empty list.
 */

const SKILL = `---
name: redirect-checks
description: "Check where a redirect or returnTo link goes before changing code that follows it."
---

# Redirect checks

Use when a task changes how the app follows a redirect or a returnTo link.

1. Keep the target inside the app. An outside target failed before.
2. Drop a returnTo that points to another host. A login link that kept one failed before.
3. Before renaming a helper that builds returnTo links, look for old links that use it. A rename once broke them.

Filled in from a fixed template, using 3 lessons from orbyn-engineering.`;

const proposal = (over: Partial<SkillProposal> = {}): SkillProposal => ({
  id: "prop-redirect-checks",
  repo: "p-app",
  state: "open",
  text: SKILL,
  lessons: [...PROPOSAL_LESSONS],
  can_approve: true,
  skill: null,
  rev: 1,
  created_at: "2026-11-07T16:31:00Z",
  approved_at: null,
  ...over,
});

export function createSkillProposalsMock(
  options: {
    onApproved?: (name: string, lessons: readonly string[]) => void;
    /** The lesson as it is now, so `can_approve` follows a lesson turned off or deleted after the proposal was made. */
    lessonNow?: (id: string) => Lesson | undefined;
    /** Whether the project exists; an unknown repository is a 404, as the contract says. */
    knownRepo?: (repo: string) => boolean;
  } = {},
) {
  let rows: SkillProposal[] = [];
  let secret = false;
  let unwritable = false;

  function reset(scenario: Scenario) {
    secret = scenario === "crew-proposals-secret";
    unwritable = scenario === "crew-proposals-approve-fails";
    rows =
      scenario === "crew-proposals"
        ? [proposal()]
        : scenario === "crew-proposals-stale"
          ? [proposal({ can_approve: false })]
          : scenario === "crew-proposals-approved"
            ? [proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2, approved_at: "2026-11-07T16:33:00Z" })]
            : scenario === "crew-proposals-secret" || scenario === "crew-proposals-approve-fails"
              ? [proposal()]
              : [];
  }

  const bad = (code: string, message: string, status = 400): MockResponse => ({ status, body: { code, message } });
  const canApprove = (one: SkillProposal): boolean =>
    options.lessonNow ? one.lessons.every((entry) => options.lessonNow?.(entry.id)?.state === "active") : one.can_approve;
  const served = (one: SkillProposal): SkillProposal => ({ ...one, can_approve: canApprove(one) });
  const nameOf = (one: SkillProposal): string => /^name:\s*(.+)$/m.exec(one.text)?.[1]?.trim() ?? "proposed-skill";

  function handle(method: string, pathname: string, search: URLSearchParams, body: unknown): MockResponse | undefined {
    if (!pathname.startsWith("/api/crew/skill-proposals")) return undefined;
    const write = (body ?? {}) as Record<string, unknown>;
    const id = /^\/api\/crew\/skill-proposals\/([A-Za-z0-9_.:-]+)\/(approve|dismiss)$/.exec(pathname);
    if (method === "GET" && pathname === "/api/crew/skill-proposals") {
      const repo = search.get("repo");
      if (!repo) return bad("invalid", "A repository is required.");
      if (options.knownRepo && !options.knownRepo(repo)) return bad("not_found", "No such repository on this device.", 404);
      return {
        status: 200,
        body: { proposals: rows.filter((one) => one.repo === repo).map(served) } satisfies SkillProposalList,
      };
    }
    if (!id) return undefined;
    if (method !== "POST") return bad("method_not_allowed", "Approve and dismiss take POST.", 405);
    const one = rows.find((entry) => entry.id === id[1]);
    if (!one) return bad("not_found", "No such proposal on this device.", 404);
    if (one.state !== "open") return bad("not_open", "This proposal is answered already.", 409);
    if (write.rev !== one.rev) return bad("stale", "This proposal changed since you opened it.", 409);
    if (id[2] === "dismiss") {
      one.state = "dismissed";
      one.rev += 1;
      return { status: 200, body: served(one) };
    }
    if (!canApprove(one)) return bad("invalid", "This proposal can no longer be approved.");
    const text = typeof write.text === "string" ? write.text : "";
    if (!text.trim() || text.length > 600) return bad("invalid", "The skill text is 1 to 600 characters.");
    // The contract fixes the scan sentence exactly (docs/crew-lessons.md, "Errors"): the screen relays it, never its own.
    if (secret && text.includes("sk-"))
      return bad("secret_refused", "Not saved: this looks like it holds a secret. Edit it and save again.");
    // Sample-only failure kind: the contract defines 409 stale/not_open, 400 invalid/secret_refused and 403 desktop_required,
    // and a skills-local write can also fail for reasons of its own, which this stands in for.
    if (unwritable)
      return bad("unwritable", `skills-local refused the write: ${nameOf(one)} could not be created (permission denied).`, 500);
    one.text = text;
    one.state = "approved";
    one.skill = { name: nameOf(one) };
    one.rev += 1;
    one.approved_at = "2026-11-07T16:33:00Z";
    options.onApproved?.(one.skill.name, one.lessons.map((entry) => entry.id));
    return { status: 200, body: served(one) };
  }

  return { reset, handle };
}
