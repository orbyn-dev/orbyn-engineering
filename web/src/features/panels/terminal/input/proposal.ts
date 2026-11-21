import { create } from "zustand";
import { looksSecret } from "./classify";

/** The typed field the service reports on an engine event (M20): a command it suggests and, optionally, one line on what it does. */
export interface TerminalProposal {
  command: string;
  note?: string;
}

/** A proposal that passed the checks, ready to show. */
export interface Proposal {
  command: string;
  note: string | null;
  /** The command looks like it holds a secret: it is shown masked and never filled in. */
  secret: boolean;
}

export const MAX_COMMAND = 4096;
export const MAX_NOTE = 1024;
// NUL and every other C0 control except tab, plus DEL: nothing that could move the cursor, end the line or type something else.
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u0008\u000a-\u001f\u007f]/;

/**
 * Checks the typed field. Only a plain string command of one line within the limit is accepted; anything else is
 * dropped, so a proposal can never put more in the line than the person sees. Never reads model text.
 */
export function acceptTypedProposal(field: TerminalProposal): Proposal | null {
  if (typeof field !== "object" || field === null || typeof field.command !== "string") return null;
  const command = field.command.trim();
  if (command === "" || command.length > MAX_COMMAND || CONTROL.test(command)) return null;
  const note =
    typeof field.note === "string"
      ? // eslint-disable-next-line no-control-regex
        field.note.replace(/[\t\n\r]+/g, " ").replace(/[\u0000-\u001f\u007f]/g, "").replace(/\s+/g, " ").trim().slice(0, MAX_NOTE) || null
      : null;
  return { command, note, secret: looksSecret(command) };
}

interface ProposalSourceState {
  /** "none" is the production default: the service reports no field, so the card never draws. */
  source: "none" | "service";
  byRun: Record<string, Proposal>;
  setSource(source: "none" | "service"): void;
  offer(threadId: string, runId: string, field: TerminalProposal): void;
  reset(): void;
}

export const proposalKey = (threadId: string, runId: string) => `${threadId}:${runId}`;

export const useProposalSource = create<ProposalSourceState>((set, get) => ({
  source: "none",
  byRun: {},
  setSource: (source) => set({ source }),
  offer: (threadId, runId, field) => {
    if (get().source !== "service") return;
    const proposal = acceptTypedProposal(field);
    if (proposal) set((state) => ({ byRun: { ...state.byRun, [proposalKey(threadId, runId)]: proposal } }));
  },
  reset: () => set({ source: "none", byRun: {} }),
}));

/** Where the engine event field would land once the service sends it. */
export const offerProposal = (threadId: string, runId: string, field: TerminalProposal) => useProposalSource.getState().offer(threadId, runId, field);
