import { AlertCircle, ChevronLeft } from "lucide-react";
import { useEffect, useRef, type ReactNode } from "react";

/** The card every account step sits in. */
export function Card({ title, lead, wide, children, label, alert, focus }: { title: string; lead?: ReactNode; wide?: boolean; children: ReactNode; label?: string; alert?: boolean; focus?: boolean }) {
  const heading = useRef<HTMLHeadingElement>(null);
  // A card that replaces another takes the focus, so it does not fall to the page.
  useEffect(() => {
    if (focus) heading.current?.focus();
  }, [focus, title]);
  const head = (
    <>
      <h2 ref={heading} tabIndex={focus ? -1 : undefined}>{title}</h2>
      {lead ? <p>{lead}</p> : null}
    </>
  );
  return (
    <section className="ac-card" data-wide={wide || undefined} aria-label={label ?? title}>
      {alert ? <div role="alert">{head}</div> : head}
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

/** "‹ Account": back to the Account page. Its name is just "Account"; the chevron is only a picture. */
export function BackLink({ onClick }: { onClick(): void }) {
  return (
    <button type="button" className="ac-back" onClick={onClick}>
      <ChevronLeft size={14} aria-hidden />
      Account
    </button>
  );
}
