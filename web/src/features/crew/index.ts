// Front door of the Crew feature: the only module outside crew imports from it.
export { CrewPreview, crewPreviewRoute } from "./previews";
export { askCloseLead, type CloseOutcome } from "./lead/closeLead";
export { CloseLeadDialog } from "./lead/CloseLeadDialog";
export * from "./autopilot/approvals";
export { CrewNavRow } from "./flow/live/CrewNavRow";
export { useCrewFlag } from "./flow/live/useCrewFlag";
export { RunTab } from "./runpage/RunPage";
export { runLinkClient, setRunLinkClient, type RunLink, type RunLinkClient } from "./runpage/link";
export { crewStartDiskLow, crewStartOutcome, diskLowOf, diskSentence, floorText, freeText, isCrewStartTool, NO_WORKER_RAN, type DiskLow } from "./tool";
export { CrewToolBlock } from "./thread/CrewToolBlock";
// Skill proposals and the skills added from lessons (KC9): the words the Skills page draws and the read it makes.
export {
  ADDED_EMPTY_LINE,
  ADDED_GROUP_NOTE,
  ALL_PROJECTS,
  AVAILABLE,
  PROPOSED_GROUP_NOTE,
  lessonsWord,
  skillText,
} from "./lessons/proposals/model";
export { useAllProposals, type AllProposalsRead } from "./lessons/proposals/useAllProposals";
export { addedProposals, openProposals } from "./lessons/proposals/useProposals";
export type { SkillProposal } from "./lessons/proposals/types";
