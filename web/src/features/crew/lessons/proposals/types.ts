/**
 * The skill proposal wire types, written by hand from docs/crew-lessons.md ("Skill proposals (KC9)"). There is no
 * generated type for them yet: they move to web/src/generated when the service slice lands, and this file goes away then.
 */

/** A proposal is made by code and waits for a person: open, or answered by Approve or Dismiss. */
export type SkillProposalState = "open" | "approved" | "dismissed";

/** A proposal the crew made from three or more active lessons of one repository; the person reads it, edits it, approves or dismisses it. */
export interface SkillProposal {
  id: string;
  repo: string;
  state: SkillProposalState;
  /** The SKILL.md text the fixed template filled in; the person edits it before Approve. */
  text: string;
  /** The source lessons, ids only. */
  lessons: string[];
  /** False when a source lesson was deleted or turned off: the proposal is kept and Approve is off. */
  can_approve: boolean;
  /** The skill that exists once the proposal is approved; null while it is not. */
  skill: { name: string } | null;
  /** Integer, +1 on every change; every write sends it back. */
  rev: number;
  created_at: string;
}

/** GET /api/crew/skill-proposals?repo=: every proposal of one repository. */
export interface SkillProposalList {
  proposals: SkillProposal[];
}
