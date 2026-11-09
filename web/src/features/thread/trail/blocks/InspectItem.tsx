import { api } from "../../../../api/client";
import type { ToolItem } from "../../../../product/timeline/types";
import { useAsync } from "../../../../lib/useAsync";
import { Dialog } from "../../../../ui/Dialog";
import { useOpenState } from "../../../../ui/openState";
import { useThreadOfTimeline } from "../../timelineThread";

function ItemDetails({ threadId, id }: { threadId: string; id: string }) {
  const raw = useAsync(`item:${threadId}:${id}`, (signal) => api.threadItem(threadId, id, signal));
  if (raw.error) return <p className="tool-empty">Not recorded for this item.</p>;
  return (
    <pre tabIndex={0} className="tool-output">
      {raw.data ? JSON.stringify(raw.data, null, 2) : "Loading..."}
    </pre>
  );
}

/** A quiet footer button that opens the call's stored record. */
export function InspectItem({ item, label = "Raw result" }: { item: ToolItem; label?: string }) {
  // Kept with the thread's other open state, so the dialog survives its row leaving and returning.
  const [open, setOpen] = useOpenState(`inspect:${item.key}`, false);
  const threadId = useThreadOfTimeline();
  if (!threadId) return null;
  return (
    <>
      <button type="button" className="bt tool-inspect" onClick={() => setOpen(true)}>
        {label}
      </button>
      {open ? (
        <Dialog title="Item details" onClose={() => setOpen(false)}>
          <ItemDetails threadId={threadId} id={item.callId} />
        </Dialog>
      ) : null}
    </>
  );
}
