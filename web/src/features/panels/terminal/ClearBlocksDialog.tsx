import { ConfirmDialog } from "../../../ui/Dialog";

/**
 * The confirmation before a terminal tab's blocks are cleared (parity G17; frame block-clear-confirm). It names what is dropped and
 * what stays. Cancel has focus and Escape closes it, so a clear is never one keystroke away.
 */
export function ClearBlocksDialog({ onConfirm, onClose }: { onConfirm(): Promise<void>; onClose(): void }) {
  return (
    <ConfirmDialog title="Clear all blocks?" confirmLabel="Clear all blocks" tone="danger" cancelFocus onConfirm={onConfirm} onClose={onClose}>
      <p>The finished commands and their output are dropped from this terminal. A command that is still running stays, and its output keeps growing. The terminal and its scrollback stay.</p>
    </ConfirmDialog>
  );
}
