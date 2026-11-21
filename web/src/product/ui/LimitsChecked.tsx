import { RefreshCw, TriangleAlert } from "lucide-react";
import type { LimitsChecked } from "../../lib/limitsChecked";
import { Button, IconButton } from "../../ui/controls";

/** "Checked 5 min ago" or "Checking…", quiet ink, beside the refresh button. Nothing before the first answer. */
export function LimitsAge({ checked }: { checked: LimitsChecked }) {
  if (checked.label === null) return null;
  return (
    <span className="limits-age" role={checked.checking ? "status" : undefined}>
      {checked.label}
    </span>
  );
}

/** Refresh limits; after a failed check the same button is named Try again. Disabled with a spinner while checking. */
export function LimitsRefresh({ checked, busy = false, onRefresh }: { checked: LimitsChecked; busy?: boolean; onRefresh(): void }) {
  const working = checked.checking || busy;
  return (
    <IconButton
      label={checked.failed ? "Try again" : "Refresh limits"}
      className={checked.failed ? "limits-retry" : undefined}
      disabled={working}
      onClick={onRefresh}
    >
      <RefreshCw size={14} className={working ? "spin" : undefined} aria-hidden />
    </IconButton>
  );
}

/** The red line after a failed check; the last answer stays above it. */
export function LimitsFailure({ checked, onRetry }: { checked: LimitsChecked; onRetry(): void }) {
  if (!checked.failure) return null;
  return (
    <div className="limits-failure" role="alert">
      <TriangleAlert size={14} aria-hidden />
      <span>{checked.failure}</span>
      <Button small onClick={onRetry}>
        Try again
      </Button>
    </div>
  );
}
