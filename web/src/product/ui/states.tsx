import { AlertTriangle, ArrowLeft } from "lucide-react";
import type { ReactNode } from "react";
import type { ApiFailure } from "../../api/client";
import { NETWORK_ERROR } from "../../api/client";
import { routeHref, type Route } from "../state/route";
import { useLayoutWidth } from "../../lib/useLayout";
import { useWaited } from "../../lib/motion";
import { Button } from "../../ui/controls";
import { Logo } from "./Logo";

/** A static skeleton, drawn only if the wait outlasts 400 ms. The label is read at once. */
export function Loading({ label, rows = 4 }: { label: string; rows?: number }) {
  const slow = useWaited(true);
  return (
    <div className="loading" role="status">
      <span className="sr-only">{label}</span>
      {slow
        ? Array.from({ length: rows }, (_, at) => <div key={at} className="skeleton" aria-hidden />)
        : null}
    </div>
  );
}

/** The splash while the app opens: the mark alone, drawn only if the wait outlasts 400 ms and gone with the real screen. */
export function Splash({ label }: { label: string }) {
  const slow = useWaited(true);
  return (
    <main className="logo-splash">
      <span className="sr-only" role="status">{label}</span>
      {slow ? <Logo size={64} /> : null}
    </main>
  );
}

export function ErrorState({
  title,
  failure,
  onRetry,
  compact,
}: {
  title: string;
  failure: ApiFailure;
  onRetry?: () => void;
  compact?: boolean;
}) {
  return (
    <div className={compact ? "error-state error-compact" : "error-state"} role="alert">
      <AlertTriangle size={16} aria-hidden />
      <div className="error-text">
        <strong>{title}</strong>
        <span>
          {failure.code === NETWORK_ERROR ? "Check that the Orbyn service is running." : failure.message}
        </span>
        {failure.code !== NETWORK_ERROR ? (
          <details>
            <summary>Details</summary>
            <code>{failure.code}</code>
          </details>
        ) : null}
      </div>
      {onRetry ? (
        <Button small onClick={onRetry}>
          Retry
        </Button>
      ) : null}
    </div>
  );
}

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <p className="empty-title">{title}</p>
      {children}
    </div>
  );
}

/** Screen identity, plus the explicit way back on one-surface layouts. */
/** In the phone layout a page has no sidebar beside it, so its topbar leads back to the thread list. */
export function NarrowBack({ to }: { to?: Route }) {
  const layout = useLayoutWidth();
  if (layout !== "phone") return null;
  // A page inside a section goes back to that section's list; the rest go to the threads.
  const label = to?.name === "pulls" ? "Back to pull requests" : "Back to threads";
  return (
    <a className="icon-btn" href={routeHref(to ?? { name: "home" })} aria-label={label} title={label}>
      <ArrowLeft size={16} aria-hidden />
    </a>
  );
}
