import { ChevronLeft } from "lucide-react";
import { useEffect } from "react";
import { ApiFailure } from "../../../../api/client";
import { ErrorState, Loading } from "../../../../product/ui/states";
import { runDetailView } from "./backgroundModel";
import { Chip, Facts } from "./parts";
import { useBackgroundRun } from "./useBackground";

export interface RunPanelProps {
  /** The run "Open the run" was pressed for. */
  runId: string;
  /** Back to Recent runs, the list the run page belongs to; a run the read does not have also returns there. */
  onBack(): void;
}

/**
 * One background run's own page (Crew designs: background-run-detail), reached from "Open the run" on a watch's banner.
 * It draws the crew the run belongs to and the run's own state, times, spend, wait and id — the eight rows of the one run
 * read — and nothing else: no log, no tail, no action and no cost. A run the read does not know answers 404: nothing of it
 * is drawn and the page goes back to Recent runs, as the button did while the read did not exist.
 */
export function RunPanel({ runId, onBack }: RunPanelProps) {
  const read = useBackgroundRun(runId);
  const missing = read.error instanceof ApiFailure && read.error.status === 404;
  useEffect(() => {
    if (missing) onBack();
  }, [missing, onBack]);
  if (missing) return null;
  if (read.status === "loading") return <Loading label="Reading the run" rows={2} />;
  const back = (
    <header className="ap-set-head ap-watch-head">
      <button type="button" className="ap-watch-back" onClick={onBack}>
        <ChevronLeft size={14} aria-hidden />
        Recent runs
      </button>
    </header>
  );
  if (read.status === "error") {
    return (
      <section className="ap-page ap-watch" aria-label="Background run">
        {back}
        <ErrorState title="Can’t read this run" failure={read.error} onRetry={() => void read.reload()} />
      </section>
    );
  }
  const detail = read.data;
  const view = runDetailView(detail);
  return (
    <section className="ap-page ap-watch" aria-label={detail.name + " run"}>
      <header className="ap-set-head ap-watch-head">
        <button type="button" className="ap-watch-back" onClick={onBack}>
          <ChevronLeft size={14} aria-hidden />
          Recent runs
        </button>
        <b>{detail.name}</b>
        <Chip tone="plain">Preview</Chip>
        <span className="ap-spacer" />
        <Chip tone={view.chip.tone}>{view.chip.text}</Chip>
      </header>
      <div className="ap-box ap-watch-facts">
        <Facts rows={view.facts} />
      </div>
    </section>
  );
}
