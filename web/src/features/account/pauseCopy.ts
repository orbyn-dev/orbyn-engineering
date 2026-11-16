import type { AccountSyncView } from "../../generated/AccountSyncView";

/** What the person can do about a pause, by its cause. Paused refusals (`zone_failed`, `too_large`) are "Couldn’t sync" instead. */
export type PauseAction =
  | { kind: "resume"; enabled: boolean }
  | { kind: "keybindings" }
  | { kind: "retry" }
  | { kind: "none" };

export interface PauseInfo {
  detail: string;
  /** Said under the buttons when the action is off. */
  off?: string;
  action: PauseAction;
}

/** Sync is paused, and why in plain words. A pause that is not the person's choice keeps the title "Paused". */
/** A scheduled deletion wins over any other cause: even a pause the person chose cannot be resumed then. */
export function pauseInfo(sync: AccountSyncView, deletionScheduled = false): PauseInfo {
  switch (deletionScheduled ? "deletion_scheduled" : sync.reason) {
    case "by_person":
      return { detail: "You paused sync. Changes are kept on this computer and don’t send or arrive until you resume.", action: { kind: "resume", enabled: true } };
    case "deletion_scheduled":
      return { detail: "Deletion is scheduled. Sync is paused.", off: "Resume sync is not available while deletion is scheduled.", action: { kind: "resume", enabled: false } };
    case "protection_advanced":
      return { detail: "Sync is paused while protection changes.", action: { kind: "none" } };
    case "invalid_keyboard_file":
      return { detail: "Sync is paused until the keybindings file is fixed.", action: { kind: "keybindings" } };
    default:
      return { detail: "Sync is paused.", action: { kind: "retry" } };
  }
}
