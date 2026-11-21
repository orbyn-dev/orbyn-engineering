import { Check } from "lucide-react";
import { maskSecrets } from "./history";
import { proposalKey, useProposalSource, type Proposal } from "./proposal";

/** The typed proposal the service reported for the run the editor started. Nothing is read from model text; with no source, always null. */
export function useProposal(threadId: string, runId: string | null): { key: string; proposal: Proposal } | null {
  const proposal = useProposalSource((state) => (state.source === "service" && runId ? state.byRun[proposalKey(threadId, runId)] : undefined));
  return proposal && runId ? { key: proposalKey(threadId, runId), proposal } : null;
}

/**
 * A command the service suggested, as plain text. Nothing here runs: Accept only puts it in the input line, once per click,
 * and the person runs it from there. A command that looks like a secret is shown masked and can only be dismissed.
 */
export function ProposalCard({ engine, proposal, onAccept, onDismiss }: { engine: string; proposal: Proposal; onAccept(): void; onDismiss(): void }) {
  return (
    <section className="term-proposal" aria-label={`${engine} suggests a command`}>
      <header>
        <b>{engine} suggests a command</b>
        <span className="term-proposal-state">Not run</span>
      </header>
      <pre className="term-proposal-code">
        <code>{proposal.secret ? maskSecrets(proposal.command) : proposal.command}</code>
      </pre>
      {proposal.secret ? (
        <p>This looks like a secret, so it is not filled in from here.</p>
      ) : proposal.note ? (
        <p>{proposal.note}</p>
      ) : null}
      <footer>
        {proposal.secret ? null : (
          <button type="button" className="term-proposal-run" onClick={onAccept}>
            <Check size={13} aria-hidden /> Accept
          </button>
        )}
        <button type="button" className="text-action" onClick={onDismiss}>
          Dismiss
        </button>
        {proposal.secret ? null : <span>Nothing runs from here. Accept puts it in the line.</span>}
      </footer>
    </section>
  );
}
