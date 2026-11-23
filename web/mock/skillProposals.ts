import type { SkillProposal, SkillProposalList } from "../src/features/crew/lessons/proposals/types.ts";
import type { MockResponse, Scenario } from "./api.ts";
import { PROPOSAL_LESSONS } from "./lessons.ts";

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

1. Keep the target inside the app. A redirect that followed an outside target failed the web/nav checks before.
2. Drop a returnTo that points to another host. A login link that kept one failed the web/nav checks before.
3. Before renaming or removing a helper that builds returnTo links, look for old links that still use it. A helper rename once stopped old returnTo links from redirecting.

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
  ...over,
});

export function createSkillProposalsMock(options: { onApproved?: (name: string, lessons: readonly string[]) => void } = {}) {
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
            ? [proposal({ state: "approved", skill: { name: "redirect-checks" }, rev: 2 })]
            : scenario === "crew-proposals-secret" || scenario === "crew-proposals-approve-fails"
              ? [proposal()]
              : [];
  }

  const bad = (code: string, message: string, status = 400): MockResponse => ({ status, body: { code, message } });
  const nameOf = (one: SkillProposal): string => /^name:\s*(.+)$/m.exec(one.text)?.[1]?.trim() ?? "proposed-skill";

  function handle(method: string, pathname: string, search: URLSearchParams, body: unknown): MockResponse | undefined {
    if (!pathname.startsWith("/api/crew/skill-proposals")) return undefined;
    const write = (body ?? {}) as Record<string, unknown>;
    const id = /^\/api\/crew\/skill-proposals\/([A-Za-z0-9_.:-]+)\/(approve|dismiss)$/.exec(pathname);
    if (method === "GET" && pathname === "/api/crew/skill-proposals") {
      const repo = search.get("repo");
      if (!repo) return bad("invalid", "A repository is required.");
      return { status: 200, body: { proposals: rows.filter((one) => one.repo === repo) } satisfies SkillProposalList };
    }
    if (!id) return undefined;
    const one = rows.find((entry) => entry.id === id[1]);
    if (!one) return bad("not_found", "No such proposal on this device.", 404);
    if (one.state !== "open") return bad("not_open", "This proposal is answered already.", 409);
    if (write.rev !== one.rev) return bad("stale", "This proposal changed since you opened it.", 409);
    if (id[2] === "dismiss") {
      one.state = "dismissed";
      one.rev += 1;
      return { status: 200, body: one };
    }
    if (!one.can_approve) return bad("invalid", "This proposal can no longer be approved.");
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
    options.onApproved?.(one.skill.name, one.lessons);
    return { status: 200, body: one };
  }

  return { reset, handle };
}
