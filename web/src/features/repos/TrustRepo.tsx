import { useEffect, useRef, useState } from "react";
import { useUi } from "../../product/state/ui";
import { reposApi } from "./api";
import type { RepoRow } from "./contract";
import { TrustDialog } from "./Dialogs";
import { repoLabel } from "./choice";

/**
 * The trust prompt for one repository of a thread, opened from a place that knows only the repository's id (a refused hunk or line
 * write). It looks the row up in the project's list and shows the same dialog the repository's Git menu opens: one click on Trust is one
 * write. When the row cannot be found it says so in a notice. It never repeats the write that was refused; `onDone` is where the
 * caller lets the person try again.
 */
export function TrustRepo({ projectId, repoId, name, onClose, onDone }: { projectId: string; repoId: string; name: string; onClose(): void; onDone(): void }) {
  const pushToast = useUi((state) => state.pushToast);
  const [row, setRow] = useState<RepoRow | null>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    let live = true;
    const missing = () => { if (!live) return; pushToast(`${repoLabel(name)} is not trusted. Trust it in the project's settings first.`); close.current(); };
    reposApi().list(projectId).then((view) => {
      const found = view?.repositories.find((one) => one.id === repoId);
      if (!live) return;
      if (found) setRow(found); else missing();
    }, missing);
    return () => { live = false; };
  }, [projectId, repoId, name, pushToast]);
  return row ? <TrustDialog projectId={projectId} repo={row} onClose={onClose} onDone={onDone} /> : null;
}
