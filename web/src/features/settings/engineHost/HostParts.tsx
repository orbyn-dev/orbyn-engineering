import { ChevronDown, Loader2 } from "lucide-react";
import { useState } from "react";
import { Button } from "../../../ui/controls";
import { type HostCause, type HostErrorKind, type HostState, type Chip, chipFor, plain } from "../../enginehost";
import "./engineHost.css";

const LOOP = "[Circular error cause]";

/** A chip drawn from its label and tone, for a state that is not a host state (a gap). */
export function HostPill({ chip }: { chip: Chip }) {
  return (
    <span className={chip.tone === "plain" ? "set-pill dot" : `set-pill ${chip.tone}${chip.spin ? "" : " dot"}`}>
      {chip.spin ? <Loader2 size={11} className="spin" aria-hidden /> : null}{chip.label}
    </span>
  );
}

/** The state chip: green only for Running, blue (with a spinner) for a wait, red for what failed, plain for the rest. */
export function HostChip({ state }: { state: HostState }) {
  const chip = chipFor(state);
  return (
    <span className={chip.tone === "plain" ? "set-pill dot" : `set-pill ${chip.tone}${chip.spin ? "" : " dot"}`}>
      {chip.spin ? <Loader2 size={11} className="spin" aria-hidden /> : null}{chip.label}
    </span>
  );
}

/**
 * The cause chain, outermost first and numbered, with the kind above it. Every cause shows; none is cut. A loop
 * marker is the last row in plain grey. There is no stack here on purpose: it belongs to Maintenance diagnostics.
 */
export function CauseChain({ kind, extra, causes }: { kind?: HostErrorKind | null; extra?: string; causes: HostCause[] }) {
  return (
    <div className="eh-chain">
      {kind || extra ? <div className="eh-chain-kind">{[kind ? `Kind: ${kind}` : null, extra].filter(Boolean).join(" · ")}</div> : null}
      <ol>
        {causes.map((cause, at) => (
          <li key={at}>
            <span>{at + 1}</span>
            {cause.message === LOOP
              ? <span className="eh-loop">{LOOP}</span>
              : <span><code>{cause.name}</code> {cause.message}{cause.code ? <> <span className="t3"><code>{cause.code}</code></span></> : null}</span>}
          </li>
        ))}
      </ol>
    </div>
  );
}

/** Show details / Hide details. The label is the button's whole name, so a screen reader hears what it opens. */
export function DetailsToggle({ open, onToggle }: { open: boolean; onToggle(): void }) {
  return (
    <Button small aria-expanded={open} icon={<ChevronDown size={13} aria-hidden style={open ? { transform: "rotate(180deg)" } : undefined} />} onClick={onToggle}>
      {open ? "Hide details" : "Show details"}
    </Button>
  );
}

export function useToggle(): [boolean, () => void] {
  const [open, setOpen] = useState(false);
  return [open, () => setOpen((value) => !value)];
}

/** The words for a failed write: the error's own message, never anything the person typed or the service stored. */
export const reasonOf = (failure: unknown): string => (failure instanceof Error && failure.message ? plain(failure.message, 300) : "Something went wrong.");
