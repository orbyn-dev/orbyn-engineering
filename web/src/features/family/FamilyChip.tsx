import { Network, X } from "lucide-react";
import { useState } from "react";
import type { Thread } from "../../api/types";
import { useLayoutWidth } from "../../lib/useLayout";
import { useWorkspace } from "../../product/state/workspace";
import { IconButton } from "../../ui/controls";
import { ToolbarPopover } from "../pulls";
import { hasKnownRelatives, useFamily } from "./api";
import { FamilyTree } from "./FamilyTree";
import { familyRows, otherCount } from "./model";
import "./family.css";

/**
 * The header's Linked chip, with the number of other threads in the family, opening the family as one tree.
 * A thread with no relatives has no chip; while the family is read the chip shows only for a thread known to have some.
 */
export function FamilyChip({ thread }: { thread: Thread }) {
  const read = useFamily(thread.id);
  const known = useWorkspace((state) => hasKnownRelatives(thread, state.threads));
  const [open, setOpen] = useState<HTMLElement | null>(null);
  const sheet = useLayoutWidth() === "phone";
  const count = read.family ? otherCount(read.family) : null;
  if (count === 0 || (read.status === "loading" && !known) || read.status === "error") return null;
  const rows = read.family ? familyRows(read.family, thread.id) : null;
  const heading = rows ? `Linked threads · ${rows.length} in this family` : "Linked threads";
  return (
    <>
      <button
        type="button"
        className="fam-chip"
        aria-label="Linked threads"
        aria-haspopup="dialog"
        aria-expanded={open !== null}
        data-pressed={open !== null || undefined}
        onClick={(event) => setOpen(open ? null : event.currentTarget)}
      >
        <Network size={14} aria-hidden />
        {count === null ? "Linked" : `Linked ${count}`}
      </button>
      {open && rows ? (
        <ToolbarPopover anchor={open} label="Linked threads" layout={sheet ? "sheet" : "popover"} width={420} onClose={() => setOpen(null)}>
          {(dismiss) =>
            sheet ? (
              <>
                <div className="pulls-sheet-top" data-sheet-handle>
                  <span className="pulls-grab" aria-hidden />
                  <div className="pulls-sheet-head">
                    <b>{heading}</b>
                    <IconButton label="Close" onClick={dismiss}>
                      <X size={14} aria-hidden />
                    </IconButton>
                  </div>
                </div>
                <div className="pulls-sheet-body">
                  <FamilyTree rows={rows} onOpen={dismiss} />
                </div>
              </>
            ) : (
              <div className="fam-pop">
                <div className="fam-head">{heading}</div>
                <FamilyTree rows={rows} onOpen={dismiss} />
              </div>
            )
          }
        </ToolbarPopover>
      ) : null}
    </>
  );
}
