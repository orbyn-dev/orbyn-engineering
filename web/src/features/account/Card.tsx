import { AlertCircle } from "lucide-react";
import type { ReactNode } from "react";

/** The card every account step sits in. */
export function Card({ title, lead, wide, children, label }: { title: string; lead?: ReactNode; wide?: boolean; children: ReactNode; label?: string }) {
  return (
    <section className="ac-card" data-wide={wide || undefined} aria-label={label ?? title}>
      <h2>{title}</h2>
      {lead ? <p>{lead}</p> : null}
      {children}
    </section>
  );
}

/** A failure in words. Red is only the failure itself. */
export function Failure({ children }: { children: ReactNode }) {
  return (
    <p className="ac-err" role="alert">
      <AlertCircle size={14} aria-hidden />
      <span>{children}</span>
    </p>
  );
}

/** A link in the card's quiet row. */
export function TextLink({ onClick, children, disabled }: { onClick(): void; children: ReactNode; disabled?: boolean }) {
  return (
    <button type="button" className="ac-lnk" onClick={onClick} disabled={disabled}>
      {children}
    </button>
  );
}
