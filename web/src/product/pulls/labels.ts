import type { PullRequest } from "../../api/types";

/** The service says `pass` and `fail`; older stand-ins say `passing` and `failing`. */
export const checksOf = (state: string | null): "pass" | "fail" | "pending" | "none" =>
  state === "pass" || state === "passing" ? "pass" : state === "fail" || state === "failing" ? "fail" : state === "pending" ? "pending" : "none";

export type DotTone = "blue" | "red" | "ok" | "mute";

/** Precedence Failing, then Running, then Passing, as the Pull requests list reads checks. */
export function checksTone(pull: PullRequest): "fail" | "running" | "pass" | "skipped" | "none" | "unknown" {
  const counts = pull.checks;
  if (counts) {
    if (counts.failing > 0) return "fail";
    if (counts.running > 0) return "running";
    if (counts.passing > 0) return "pass";
    return counts.total > 0 ? "skipped" : "none";
  }
  const state = checksOf(pull.checks_state);
  return state === "fail" ? "fail" : state === "pending" ? "running" : state === "pass" ? "pass" : "none";
}

/** The sentence under a row: "4 of 4 checks passed". Null when the forge gave a state but no counts. */
export function checksSummary(pull: PullRequest): { tone: ReturnType<typeof checksTone>; text: string } | null {
  const tone = checksTone(pull);
  const counts = pull.checks;
  if (tone === "none") return { tone, text: "No checks" };
  if (!counts) return null;
  if (tone === "fail") return { tone, text: `${counts.failing} of ${counts.total} checks failed` };
  if (tone === "running") return { tone, text: `${counts.total - counts.running} of ${counts.total} checks done` };
  if (tone === "skipped") return { tone, text: "No checks ran" };
  return { tone, text: `${counts.passing} of ${counts.total} checks passed` };
}

/** Open, not a draft, checks passing and mergeable: the Pull requests list's own rule. */
export const readyToMerge = (pull: PullRequest, mergeable: string | null | undefined) =>
  pull.state === "open" && !pull.draft && checksOf(pull.checks_state) === "pass" && mergeable === "mergeable";

/** The state word the row leads with: Open, Merged or Closed. A draft leads with its own label instead. */
export const stateWord = (pull: PullRequest): "Open" | "Merged" | "Closed" | null =>
  pull.draft ? null : pull.state === "merged" ? "Merged" : pull.state === "closed" ? "Closed" : "Open";

/** A dot for the chip: blue while checks run, red when one failed, quiet otherwise. */
export function chipDot(pulls: readonly PullRequest[]): DotTone {
  const tones = pulls.map(checksTone);
  if (tones.includes("fail")) return "red";
  if (tones.includes("running")) return "blue";
  return tones.some((tone) => tone === "pass") ? "ok" : "mute";
}

/** "#3" on GitHub and the other forges, "!3" for a GitLab merge request. */
export const numberLabel = (pull: Pick<PullRequest, "number" | "url">) => `${forgeName(pull.url) === "GitLab" ? "!" : "#"}${pull.number}`;

export const chipLabel = (pulls: readonly PullRequest[]) => (pulls.length === 1 ? numberLabel(pulls[0]!) : `PRs ${pulls.length}`);

/** The forge's name from a pull request's address, for the line that says it did not answer. */
export function forgeName(url: string | undefined): string {
  const host = (() => {
    try {
      return new URL(url ?? "").hostname;
    } catch {
      return "";
    }
  })();
  if (host.includes("github")) return "GitHub";
  if (host.includes("gitlab")) return "GitLab";
  if (host.includes("dev.azure") || host.includes("visualstudio")) return "Azure DevOps";
  if (host.includes("forgejo") || host.includes("codeberg")) return "Forgejo";
  return "The host";
}

export const WATCH_TIP =
  "Tells you when a check fails, all checks pass, the branch conflicts with its base, or there are new comments or reviews. Yours are skipped.";
