import { Link2, X } from "lucide-react";
import { useState } from "react";
import { compactAge } from "../../lib/format";
import { useLayoutWidth } from "../../lib/useLayout";
import { routeHref } from "../../product/state/route";
import { useWorkspace } from "../../product/state/workspace";
import { IconButton } from "../../ui/controls";
import { ToolbarPopover } from "../pulls";
import type { IssueDetail } from "./contract";
import "./issues.css";

/**
 * The issue page header's Linked threads chip: "Linked from N threads" for the thread ids the issue's own read carries
 * (`IssueDetail.linked_threads`), with the list as a popover, or a bottom sheet on a phone. It is read only: a row opens that
 * thread's page, and linking or starting lives in the page's Linked threads card. A linked thread the app has not loaded yet keeps
 * its id as the row's text, so the count and the list always agree.
 */
export function LinkedThreadsChip({ issue }: { issue: IssueDetail }) {
  const threads = useWorkspace((state) => state.threads);
  const [open, setOpen] = useState<HTMLElement | null>(null);
  const sheet = useLayoutWidth() === "phone";
  const ids = issue.linked_threads;
  if (ids.length === 0) return null;
  const label = ids.length === 1 ? "Linked from 1 thread" : `Linked from ${ids.length} threads`;
  const rows = ids.map((id) => {
    const thread = threads[id];
    return (
      <a key={id} className="iss-prow-open" href={routeHref({ name: "thread", id })}>
        <span className="iss-prow-title"><b>{thread?.title ?? id}</b></span>
        {thread ? (
          <span className="iss-prow-meta">
            <span>{thread.active_run ? "Running" : compactAge(thread.updated_at)}</span>
            <span>{[thread.engine, thread.worktree?.branch].filter(Boolean).join(" · ")}</span>
          </span>
        ) : null}
      </a>
    );
  });
  const list = (
    <div className="iss-pop">
      <div className="iss-pop-head">Linked threads</div>
      {rows}
    </div>
  );
  return (
    <>
      <button
        type="button"
        className="iss-chip-btn"
        aria-label="Linked threads"
        aria-haspopup="dialog"
        aria-expanded={open !== null}
        data-pressed={open !== null || undefined}
        onClick={(event) => setOpen(open ? null : event.currentTarget)}
      >
        <Link2 size={14} aria-hidden />
        {label}
      </button>
      {open ? (
        <ToolbarPopover anchor={open} label="Linked threads" layout={sheet ? "sheet" : "popover"} width={392} onClose={() => setOpen(null)}>
          {(dismiss) =>
            sheet ? (
              <>
                <div className="pulls-sheet-top" data-sheet-handle>
                  <span className="pulls-grab" aria-hidden />
                  <div className="pulls-sheet-head">
                    <b>Linked threads</b>
                    <IconButton label="Close" onClick={dismiss}><X size={14} aria-hidden /></IconButton>
                  </div>
                </div>
                <div className="pulls-sheet-body">{list}</div>
              </>
            ) : list
          }
        </ToolbarPopover>
      ) : null}
    </>
  );
}
