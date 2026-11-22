import { useEffect } from "react";
import { navigate } from "../../../../product/state/route";
import { useUi } from "../../../../product/state/ui";
import { useWorkspace } from "../../../../product/state/workspace";
import { listTerminals } from "../../api";
import { attentionApi } from "./contract";
import { useAttention, type AttentionEntry } from "./store";
import { notificationText, toastText } from "./text";

/** One toast and one system notification per terminal in this long. */
export const NOTICE_GAP_MS = 30_000;

/**
 * Goes to the thread and brings its terminal forward. It focuses and nothing else: it writes no key and no text to the terminal. The
 * terminal's tab group is read from the terminal list (a split shares a group); without it the terminal's own id is the group.
 */
export async function openAttentionTerminal(target: { threadId: string; terminalId: string }): Promise<void> {
  navigate({ name: "thread", id: target.threadId });
  let group = target.terminalId;
  try {
    group = (await listTerminals(target.threadId)).find((one) => one.id === target.terminalId)?.group ?? group;
  } catch {
    // The terminal list is unreachable: the drawer still opens on the terminal's own id.
  }
  useUi.getState().revealTerminal(target.threadId, group);
}

const threadTitleOf = (id: string): string => useWorkspace.getState().threads[id]?.title ?? "";

/**
 * Listens for waiting engine consoles and tells the person the way thread notices go: a toast (In-app notifications) and a system
 * notification (Thread notifications) only when the app is in the background and the permission is already granted. It asks for no
 * permission. With no client (the production default) it subscribes to nothing. `openId` is the thread on screen, whose item is
 * already in front of the person.
 */
export function useTerminalAttention(openId: string | null): void {
  useEffect(() => {
    const api = attentionApi();
    if (!api) return;
    const told = new Map<string, number>();
    const tell = (entry: AttentionEntry) => {
      if (entry.dismissed) return;
      const key = `${entry.threadId}\u0000${entry.terminalId}`;
      const now = Date.now();
      if (now - (told.get(key) ?? Number.NEGATIVE_INFINITY) < NOTICE_GAP_MS) return;
      told.set(key, now);
      const { inAppNotifications, threadNotifications, pushToast } = useUi.getState();
      const open = () => void openAttentionTerminal(entry);
      const title = threadTitleOf(entry.threadId);
      if (inAppNotifications && entry.threadId !== openId) pushToast(toastText(entry, title), "needs", { label: "Open", run: open });
      if (threadNotifications && (document.hidden || !document.hasFocus()) && "Notification" in window && Notification.permission === "granted") {
        const note = new Notification(title === "" ? "Orbyn" : title, { body: notificationText(entry, title) });
        note.onclick = () => { window.focus(); open(); };
      }
    };
    return api.subscribe((delivery) => {
      const fresh = useAttention.getState().receive(delivery);
      if (fresh) tell(fresh);
    });
  }, [openId]);
}
