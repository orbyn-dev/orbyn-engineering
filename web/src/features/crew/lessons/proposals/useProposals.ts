import type { ApiFailure } from "../../../../api/client";
import { useAsync } from "../../../../lib/useAsync";
import { skillProposalsApi } from "./api";
import type { SkillProposal, SkillProposalList } from "./types";

/**
 * One repository's proposals. A 404 `not_found` is not an error: it is the answer of a service that has no skill-proposal
 * routes (Crew Preview off, or an older service), and the Lessons page draws no proposals section and the Skills page no
 * group. Any other failure is reported by the section as the lessons page reports its own: a red banner with Try again.
 */
export type ProposalsRead =
  | { status: "loading"; list: null; failure: null }
  | { status: "missing"; list: null; failure: null }
  | { status: "error"; list: null; failure: ApiFailure }
  | { status: "ready"; list: SkillProposalList; failure: null };

export type ProposalsReader = ProposalsRead & { reload(): Promise<void> };

export function useProposals(repo: string): ProposalsReader {
  const read = useAsync(`crew-proposals:${repo}`, (signal) => skillProposalsApi.list(repo, signal));
  const state: ProposalsRead =
    read.status === "ready"
      ? { status: "ready", list: read.data, failure: null }
      : read.status === "error"
        ? read.error.status === 404
          ? { status: "missing", list: null, failure: null }
          : { status: "error", list: null, failure: read.error }
        : { status: "loading", list: null, failure: null };
  return Object.assign(state, { reload: read.reload });
}

/** The open proposals of a read list, in the order the service answered them. */
export const openProposals = (proposals: readonly SkillProposal[]): SkillProposal[] =>
  proposals.filter((proposal) => proposal.state === "open");

/** The approved proposals whose skill exists. */
export const addedProposals = (proposals: readonly SkillProposal[]): SkillProposal[] =>
  proposals.filter((proposal) => proposal.state === "approved" && proposal.skill !== null);
