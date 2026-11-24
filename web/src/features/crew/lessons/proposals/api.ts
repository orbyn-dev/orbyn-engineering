// The skill-proposals client (KC9): the read is Local and Preview-gated, so a page and its reload may send it. The two
// writes are Person-only and are never sent by a load, a refresh or a retry of a read: only a person's own click
// approves or dismisses. A service without the routes answers 404 not_found to the read, and the screens draw nothing.
import { ApiFailure, request } from "../../../../api/client";
import type { SkillProposal, SkillProposalList } from "./types";

/** The same shape the service's every crew id check accepts. */
const validProposalId = (id: string): boolean => id.length <= 96 && /^[A-Za-z0-9:_-][A-Za-z0-9:_.-]*$/.test(id);

const path = (id: string): string => {
  if (!validProposalId(id)) throw new ApiFailure("invalid", "That proposal’s id is not valid.", 0);
  return `/api/crew/skill-proposals/${encodeURIComponent(id)}`;
};

export const skillProposalsApi = {
  /** One repository's proposals. A read, never a write. */
  list: (repo: string, signal?: AbortSignal) =>
    request<SkillProposalList>("GET", `/api/crew/skill-proposals?repo=${encodeURIComponent(repo)}`, undefined, signal),
  /** A person adds the skill from the text they read. */
  approve: (id: string, body: { text: string; rev: number }) =>
    request<SkillProposal>("POST", `${path(id)}/approve`, body),
  /** A person dismisses the proposal; the lessons it came from stay as they are. */
  dismiss: (id: string, rev: number) => request<SkillProposal>("POST", `${path(id)}/dismiss`, { rev }),
};
