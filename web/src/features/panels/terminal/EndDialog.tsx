import type { TerminalInfo } from "../../../api/types";
import { ConfirmDialog } from "../../../ui/Dialog";

const listNames = (names: string[]) => (names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : (names[0] ?? ""));
const count = (n: number) => (n === 2 ? "both of them" : `all ${n} of them`);

/** The confirmation for ending a running terminal (one pane) or a split tab (every pane, "End all"). Cancel has focus. */
export function EndDialog({ panes, onConfirm, onClose }: { panes: TerminalInfo[]; onConfirm(): Promise<void>; onClose(): void }) {
  const running = panes.filter((pane) => pane.alive).map((pane) => pane.title);
  const tab = panes.length > 1;
  return (
    <ConfirmDialog title={tab ? "End this tab?" : "End this terminal?"} confirmLabel={tab ? "End all" : "End"} tone="danger" cancelFocus onConfirm={onConfirm} onClose={onClose}>
      {tab ? (
        <p>
          This tab has {panes.length} terminals and {listNames(running)} {running.length > 1 ? "are" : "is"} still running. Ending the tab ends {count(panes.length)} and stops {running.length > 1 ? "them" : listNames(running)}.
        </p>
      ) : (
        <p>{running[0]} is still running. Ending the terminal stops it.</p>
      )}
    </ConfirmDialog>
  );
}
