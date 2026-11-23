import {
  addedProposals,
  openProposals,
  skillText,
  useAllProposals,
  useCrewPreviewRead,
  type SkillProposal,
} from "../crew";
import { useVisibleProjects } from "../../product/state/workspace";

/** A skill a person approved from a Crew proposal: the Skills page lists it as Personal. */
export interface AddedSkill {
  name: string;
  description: string;
  /** How many lessons the proposal was based on. */
  lessons: number;
  repo: { id: string; name: string };
}

/** A proposal that waits for a person, with the repository it came from. */
export interface ProposedSkill {
  proposal: SkillProposal;
  repo: { id: string; name: string };
}

/**
 * The skill proposals Crew made, across every visible project, for the Skills page (KC9). The page is not scoped to one
 * repository, so each visible project's proposals are read and merged. Crew Preview off reads nothing; a service without the
 * routes answers 404 to every read and the page then draws no group at all; another failure draws no group either, since the
 * design has no error state for it.
 */
export type CrewSkillsRead =
  | { status: "off" | "loading" | "missing" | "error"; proposed: ProposedSkill[]; added: AddedSkill[] }
  | { status: "ready"; proposed: ProposedSkill[]; added: AddedSkill[] };

export function useCrewSkills(): CrewSkillsRead {
  // The flag's own read tells "off" from "not answered yet": a skill added from lessons is not a stale address while it runs.
  const preview = useCrewPreviewRead();
  const on = preview.status === "ready" && preview.data.enabled;
  const projects = useVisibleProjects();
  const read = useAllProposals(on ? projects : null);
  if (preview.status === "loading") return { status: "loading", proposed: [], added: [] };
  if (!on) return { status: "off", proposed: [], added: [] };
  if (read.status !== "ready") return { status: read.status, proposed: [], added: [] };
  const proposed = read.groups.flatMap((group) =>
    openProposals(group.proposals).map((proposal) => ({ proposal, repo: group.repo })),
  );
  const added: AddedSkill[] = read.groups.flatMap((group) =>
    addedProposals(group.proposals).flatMap((proposal) => {
      const text = skillText(proposal.text);
      // A skill is listed by the name its own text gives; a text without one is not listed, rather than guessed.
      if (!text.name) return [];
      return [{ name: text.name, description: text.description ?? "", lessons: proposal.lessons.length, repo: group.repo }];
    }),
  );
  return { status: "ready", proposed, added };
}

