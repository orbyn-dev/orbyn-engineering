import { Network, RefreshCw } from "lucide-react";
import type { Thread } from "../../api/types";
import { Button } from "../../ui/controls";
import { useFamily } from "./api";
import { FamilyTree } from "./FamilyTree";
import { familyRows, otherCount } from "./model";
import "./family.css";

/**
 * The panel's Linked threads tab: the family tree beside the thread. `onStart` starts the one linked thread the service can
 * make without a run to fork from, a side chat; the owner of the thread supplies it.
 */
export function FamilyPanel({ thread, onStart }: { thread: Thread; onStart(): void }) {
  const read = useFamily(thread.id);
  if (read.status === "loading") {
    return (
      <div className="fam-panel" aria-busy="true" aria-label="Loading linked threads">
        {[0, 1, 2].map((at) => (
          <div key={at} className="fam-skel-row">
            <span className="fam-skel" style={{ width: "70%" }} />
            <span className="fam-skel" style={{ width: "40%" }} />
          </div>
        ))}
      </div>
    );
  }
  if (read.status === "error") {
    return (
      <div className="fam-panel fam-note" role="alert">
        <b>Can’t load linked threads</b>
        <span>The thread is fine. Nothing is changed. Retry asks again.</span>
        <Button small icon={<RefreshCw size={13} aria-hidden />} onClick={read.reload}>
          Retry
        </Button>
      </div>
    );
  }
  if (otherCount(read.family) === 0) {
    return (
      <div className="fam-panel fam-note">
        <Network size={18} aria-hidden />
        <b>No linked threads</b>
        <span>Fork this thread to try another approach, or start a linked thread to ask something on the side.</span>
        <Button small onClick={onStart}>
          Start a linked thread
        </Button>
      </div>
    );
  }
  return (
    <div className="fam-panel">
      <FamilyTree rows={familyRows(read.family, thread.id)} />
    </div>
  );
}
