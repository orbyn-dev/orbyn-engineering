import { toFailure, useAsync } from "../../../../lib/useAsync";
import type { Project } from "../../../../api/types";
import { skillProposalsApi } from "./api";
import type { SkillProposal } from "./types";

/** One repository and the proposals its read answered. */
export interface ProposalGroup {
  repo: Pick<Project, "id" | "name">;
  proposals: SkillProposal[];
}

/**
 * The proposals of every visible project, for the Skills page, which is not scoped to one repository. Each repository's
 * read is Local and Preview-gated; a service without the routes answers 404 to every one of them, and the page then draws
 * no groups at all. One repository that answers while others 404 is still answered (an older service rolls forward).
 */
export type AllProposalsRead =
  | { status: "loading"; groups: null; failure: null }
  | { status: "missing"; groups: null; failure: null }
  | { status: "error"; groups: null; failure: ReturnType<typeof toFailure> }
  | { status: "ready"; groups: ProposalGroup[]; failure: null };

export function useAllProposals(projects: readonly Pick<Project, "id" | "name">[] | null): AllProposalsRead {
  const repos = projects ?? [];
  const read = useAsync(
    `crew-proposals:all:${repos.map((repo) => repo.id).join(",")}`,
    async (signal) => {
      const answers = await Promise.all(
        repos.map((repo) =>
          skillProposalsApi.list(repo.id, signal).then(
            (list) => ({ repo: { id: repo.id, name: repo.name }, proposals: list.proposals, missing: false }),
            (error: unknown) => {
              if (toFailure(error).status === 404) return { repo: { id: repo.id, name: repo.name }, proposals: [], missing: true };
              throw error;
            },
          ),
        ),
      );
      return answers;
    },
  );
  // No project list yet is a read that has not answered, not a repository with nothing proposed.
  if (projects === null || read.status === "loading") return { status: "loading", groups: null, failure: null };
  if (read.status === "error") {
    if (read.error.status === 404) return { status: "missing", groups: null, failure: null };
    return { status: "error", groups: null, failure: read.error };
  }
  const groups = read.data.map(({ repo, proposals }) => ({ repo, proposals }));
  // A read made for another repository list (a page that had no projects yet) is not this one's answer.
  if (read.data.map(({ repo }) => repo.id).join(",") !== repos.map((repo) => repo.id).join(","))
    return { status: "loading", groups: null, failure: null };
  return answersMissing(read.data) ? { status: "missing", groups: null, failure: null } : { status: "ready", groups, failure: null };
}

/** True when every repository answered 404: the service has no skill-proposal routes. */
function answersMissing(answers: readonly { missing: boolean }[]): boolean {
  return answers.length > 0 && answers.every((answer) => answer.missing);
}
