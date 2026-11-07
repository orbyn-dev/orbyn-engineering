import { CloudOff, Copy, GitPullRequest, Lock, RefreshCw, TriangleAlert, X } from "lucide-react";
import type { ReactNode } from "react";
import { Button } from "../../ui/controls";

/** A quiet bar above the list: what is wrong, and the step that helps. */
export function PullsBanner({ icon, children, actions, tone, live }: { icon: ReactNode; children: ReactNode; actions: ReactNode; tone?: "warn" | "err"; live?: boolean }) {
  return (
    <div className="pulls-banner" data-tone={tone} role={live ? "status" : undefined}>
      {icon}
      <span>{children}</span>
      <span className="pulls-banner-end">{actions}</span>
    </div>
  );
}

export const OfflineBanner = ({ ago, onRetry }: { ago: string; onRetry(): void }) => (
  <PullsBanner
    live
    icon={<CloudOff size={14} aria-hidden />}
    actions={
      <Button small tone="secondary" icon={<RefreshCw size={14} aria-hidden />} onClick={onRetry}>
        Retry
      </Button>
    }
  >
    You{"’"}re offline. Showing what was loaded {ago}.
  </PullsBanner>
);

export const DetailsBanner = ({ onRetry, onRemove }: { onRetry(): void; onRemove(): void }) => (
  <PullsBanner
    live
    icon={<TriangleAlert size={14} aria-hidden />}
    actions={
      <>
        <Button small tone="secondary" icon={<RefreshCw size={14} aria-hidden />} onClick={onRetry}>
          Retry
        </Button>
        <Button small tone="secondary" onClick={onRemove}>
          Remove filter
        </Button>
      </>
    }
  >
    Couldn{"’"}t load details, so this filter can{"’"}t apply yet.
  </PullsBanner>
);

/** The one-block states of the list: nothing to show, or no way to reach the forge. */
function Block({ icon, title, children, actions }: { icon: ReactNode; title: string; children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="pulls-block">
      {icon}
      <h2>{title}</h2>
      <p>{children}</p>
      {actions ? <div className="pulls-block-actions">{actions}</div> : null}
    </div>
  );
}

export const NoPulls = () => (
  <Block icon={<GitPullRequest size={16} aria-hidden />} title="No pull requests yet.">
    Open one from a thread{"\u2019"}s Git menu.
  </Block>
);

export const Unreachable = ({ forge, onRetry, onCopy }: { forge: string; onRetry(): void; onCopy(): void }) => (
  <Block
    icon={<X size={16} aria-hidden />}
    title={`Can’t reach ${forge}`}
    actions={
      <>
        <Button tone="primary" icon={<RefreshCw size={14} aria-hidden />} onClick={onRetry}>
          Retry
        </Button>
        <Button tone="secondary" icon={<Copy size={14} aria-hidden />} onClick={onCopy}>
          Copy details
        </Button>
      </>
    }
  >
    {forge} did not answer. Your pull requests are not changed. Orbyn tries again when you press Retry.
  </Block>
);

export const SignedOut = ({ forge, onSignIn, onLater }: { forge: string; onSignIn(): void; onLater(): void }) => (
  <Block
    icon={<Lock size={16} aria-hidden />}
    title={`Sign in to ${forge}`}
    actions={
      <>
        <Button tone="primary" onClick={onSignIn}>
          Sign in to {forge}
        </Button>
        <Button tone="secondary" onClick={onLater}>
          Not now
        </Button>
      </>
    }
  >
    Orbyn needs your {forge} account to list pull requests, checks and reviews. Nothing is posted without you.
  </Block>
);
