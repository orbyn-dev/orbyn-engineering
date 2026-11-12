import { X } from "lucide-react";
import type { ReactNode } from "react";
import { panelOverlays, useLayoutWidth } from "../../lib/useLayout";
import { fileName, isFileTab, parseFileTab, useUi, type FileTabId } from "../../product/state/ui";
import { IconButton } from "../../ui/controls";
import { useFileTabNames } from "../workspace";
import { EditorTitleExtras } from "./editor/EditorTitleExtras";
import { FileTab } from "./files/FileTab";
import { PanelResizer } from "./PanelHost";
import { PANEL_DEFAULT } from "./strip/panelWidth";
import { tabOf } from "./strip/tabs";

/** The title line of the open file: its name and the worktree it is in. */
function Title({ tab }: { tab: FileTabId }) {
  const target = parseFileTab(tab);
  const names = useFileTabNames(target && target.kind !== "thread" ? target.projectId : "", target && target.kind !== "thread" ? target.worktreeId : "", null);
  const subtitle = !names.worktree ? null : names.project ? `${names.project} · ${names.worktree}` : names.worktree;
  return (
    <div className="ctx-title">
      <b>{target?.kind === "explorer" ? fileName(target.path) : tabOf(tab).label}</b>
      {subtitle ? <span className="ctx-sub">{subtitle}</span> : null}
      <EditorTitleExtras tabId={tab} onSave={() => undefined} />
    </div>
  );
}

/**
 * A page of no thread (home, a new thread, a project) with the side panel beside it, when files are open. With no
 * thread there is nothing for the panel to show but those files, read-only: no Changes, Browser or Activity tab, and
 * no composer. Without an open file the page fills the window as it always did.
 */
export function WithFilePanel({ children }: { children: ReactNode }) {
  const open = useUi((state) => state.panelOpen);
  const order = useUi((state) => state.panelTabOrder);
  const chosen = useUi((state) => state.panelTab);
  const width = useUi((state) => state.panelWidth);
  const closePanel = useUi((state) => state.closePanel);
  const closeTab = useUi((state) => state.closePanelTab);
  const openFileTab = useUi((state) => state.openFileTab);
  const layout = useLayoutWidth();
  const files = order.filter(isFileTab);
  // The chosen surface may be one of a thread's the panel cannot show here; the first file stands in.
  const tab = files.find((id) => id === chosen) ?? files[0] ?? null;
  if (!open || tab === null) return <>{children}</>;
  const sheet = layout === "phone";
  const mode = sheet ? "sheet" : panelOverlays(layout) ? "overlay" : "docked";
  return (
    <div className="thread-layout">
      <div className="thread-main" hidden={sheet}>
        {children}
      </div>
      <aside className="context" aria-label="Files" data-mode={mode} style={!sheet ? { width: width ?? PANEL_DEFAULT } : undefined}>
        {sheet ? null : <PanelResizer />}
        <header className="ctx-strip">
          <div className="ctx-tabs" role="tablist" aria-label="Panel tabs">
            {files.map((id) => {
              const { label, icon: Icon } = tabOf(id);
              return (
                <button
                  key={id}
                  type="button"
                  role="tab"
                  className="ctx-slot"
                  data-tab={id}
                  data-tip={label}
                  data-selected={tab === id || undefined}
                  aria-label={label}
                  aria-selected={tab === id}
                  tabIndex={tab === id ? 0 : -1}
                  onClick={() => {
                    const target = parseFileTab(id);
                    if (tab !== id && target) openFileTab(target);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Delete" || event.key === "Backspace") closeTab(id);
                  }}
                >
                  <Icon size={14} aria-hidden />
                </button>
              );
            })}
          </div>
          <div className="ctx-end">
            <IconButton label="Close side panel" onClick={closePanel}>
              <X size={14} aria-hidden />
            </IconButton>
          </div>
        </header>
        <Title tab={tab} />
        <div className="context-body">
          <div role="tabpanel" className="tabpanel">
            <FileTab key={tab} id={tab} threadId={null} />
          </div>
        </div>
      </aside>
    </div>
  );
}
