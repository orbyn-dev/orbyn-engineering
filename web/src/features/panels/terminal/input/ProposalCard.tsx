import { Play, Pencil } from "lucide-react";
import { useMemo } from "react";
import { useThreadLogs } from "../../../../product/state/threadLogs";
import { maskSecrets } from "./history";
import { proposalFromEvents, proposalKey, useProposalSource, type Proposal } from "./proposal";

/** The proposal for the run the editor started, from the service's event or a test's typed offer. Nothing is read from model text. */
export function useProposal(threadId: string, runId: string | null): { key: string; proposal: Proposal } | null {
  const source = useProposalSource((state) => state.source);
  const offered = useProposalSource((state) => (runId ? state.byRun[proposalKey(threadId, runId)] : undefined));
  const events = useThreadLogs((state) => (runId ? state.logs[threadId]?.events : undefined));
  const recorded = useMemo(() => (runId && events ? proposalFromEvents(events, runId) : null), [events, runId]);
  if (source !== "service" || !runId) return null;
  const proposal = offered ?? recorded;
  return proposal ? { key: proposalKey(threadId, runId), proposal } : null;
}

/**
 * A command the engine proposed, as plain text. Nothing here runs by itself: Run is the only way it reaches the shell,
 * once per click. A command that looks like a secret is shown masked and can only be dismissed.
 */
export function ProposalCard({ engine, proposal, busy, onRun, onEdit, onCancel }: { engine: string; proposal: Proposal; busy: boolean; onRun(): void; onEdit(): void; onCancel(): void }) {
  return (
    <section className="term-proposal" aria-label={`${engine} proposes a command`}>
      <header>
        <b>{engine} proposes a command</b>
        <span className="term-proposal-state">Not run</span>
      </header>
      <pre className="term-proposal-code">
        <code>{proposal.secret ? maskSecrets(proposal.command) : proposal.command}</code>
      </pre>
      {proposal.secret ? (
        <p>This looks like a secret, so it is not filled in or run from here.</p>
      ) : proposal.note ? (
        <p>{proposal.note}</p>
      ) : null}
      <footer>
        {proposal.secret ? null : (
          <>
            <button type="button" className="term-proposal-run" disabled={busy} onClick={onRun}>
              <Play size={13} aria-hidden /> Run
            </button>
            <button type="button" className="text-action" onClick={onEdit}>
              <Pencil size={13} aria-hidden /> Edit
            </button>
          </>
        )}
        <button type="button" className="text-action" onClick={onCancel}>
          Cancel
        </button>
        {proposal.secret ? null : <span>It does not run until you press Run</span>}
      </footer>
    </section>
  );
}
