// KC10: one desktop notification the first time an item reaches Needs you, once per wait, from the one needs-you read.
// The service is asked before a card is shown, so another window never announces the same item; the item is told even
// when Thread notifications, the permission or the window's focus keep the card from being raised. This hook never asks
// for the permission. It is mounted once by the shell and does nothing while the preference or Crew Preview is off.
import { useEffect } from "react";
import { ApiFailure, request } from "../../../api/client";
import type { NeedsYouItem } from "../../../generated/NeedsYouItem";
import type { NeedsYouList } from "../../../generated/NeedsYouList";
import type { NeedsYouNotified } from "../../../generated/NeedsYouNotified";
import type { NeedsYouRef } from "../../../generated/NeedsYouRef";
import { navigate, type Route } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { useConnection } from "../../../product/state/connection";
import { groupNoticeOf, NEEDS_YOU_INBOX, noticeOf } from "./text";

/** The first read waits a little after the shell mounts, as the scheduled-task notices do. */
export const CREW_NOTIFY_FIRST_POLL_MS = 5_000;
/** How often the needs-you read is asked again. */
export const CREW_NOTIFY_POLL_MS = 30_000;
/** A burst gathers this long after its first new item, so the cards are counted whole. */
export const CREW_NOTIFY_WINDOW_MS = 10_000;
/** The service takes at most this many refs in one notify call. */
export const CREW_NOTIFY_MAX = 50;

const keyOf = (item: NeedsYouItem): string => `${item.kind}:${item.id}:${item.since}`;

export function useCrewNotifications(): void {
  const enabled = useUi((state) => state.crewNotifications);
  // The app's own info read carries the flag; a screen of its own would add a preview request to every boot.
  const preview = useConnection((state) => state.crewPreview);
  useEffect(() => {
    if (!enabled || !preview) return;
    let stopped = false;
    const asked = new Set<string>();
    const buffered = new Map<string, { item: NeedsYouItem; ref: NeedsYouRef }>();
    let flushTimer: ReturnType<typeof setTimeout> | undefined;
    let pollTimer: ReturnType<typeof setTimeout> | undefined;
    const abort = new AbortController();

    /** The card itself, only while the app is in the background and Thread notifications is on. */
    const raise = (title: string, lines: string[], tag: string, target: Route) => {
      const { threadNotifications } = useUi.getState();
      if (!threadNotifications) return;
      if (!(document.hidden || !document.hasFocus())) return;
      if (!("Notification" in window) || Notification.permission !== "granted") return;
      const card = new Notification(title, { body: lines.join("\n"), tag });
      card.onclick = () => {
        window.focus();
        navigate(target);
        card.close();
      };
    };

    /** Asks the service which buffered items this window may tell, then raises what it claimed. */
    const flush = async () => {
      flushTimer = undefined;
      if (stopped || buffered.size === 0) return;
      const entries = [...buffered.values()];
      // The item is offered once per wait in this session, whatever the answer or a failed call.
      for (const { item } of entries) asked.add(keyOf(item));
      buffered.clear();
      const claimed: NeedsYouItem[] = [];
      for (let at = 0; at < entries.length; at += CREW_NOTIFY_MAX) {
        const chunk = entries.slice(at, at + CREW_NOTIFY_MAX);
        try {
          const answer = await request<NeedsYouNotified>(
            "POST",
            "/api/crew/needs-you/notify",
            { items: chunk.map((entry) => entry.ref) },
            abort.signal,
          );
          for (const ref of answer.claimed) {
            const found = chunk.find((entry) => entry.item.kind === ref.kind && entry.item.id === ref.id);
            if (found) claimed.push(found.item);
          }
        } catch {
          // A failed call is told nothing: the service claimed what it answered, and this window does not ask again.
        }
      }
      if (stopped || claimed.length === 0) return;
      if (claimed.length >= 3) {
        const notice = groupNoticeOf(claimed);
        raise(notice.title, notice.lines, "orbyn-crew", NEEDS_YOU_INBOX);
        return;
      }
      for (const item of claimed) {
        const notice = noticeOf(item);
        raise(notice.title, notice.lines, `orbyn-crew:${item.kind}:${item.id}`, notice.target);
      }
    };

    /** Buffers the items this session has not offered yet; the first one starts the window. */
    const offer = (items: NeedsYouItem[]) => {
      let opened = false;
      for (const item of items) {
        const key = keyOf(item);
        if (asked.has(key) || buffered.has(key)) continue;
        buffered.set(key, { item, ref: { kind: item.kind, id: item.id } });
        opened = true;
      }
      if (opened && flushTimer === undefined) {
        flushTimer = setTimeout(() => void flush(), CREW_NOTIFY_WINDOW_MS);
      }
    };

    const poll = async () => {
      try {
        const list = await request<NeedsYouList>("GET", "/api/crew/needs-you", undefined, abort.signal);
        if (!stopped) offer(list.items);
      } catch (failure) {
        if (failure instanceof ApiFailure && failure.status === 404) {
          // No notify routes in this service: nothing is read, said or retried.
          stopped = true;
          return;
        }
        // Any other failed read is ignored; the next poll tries again.
      }
      if (!stopped) pollTimer = setTimeout(() => void poll(), CREW_NOTIFY_POLL_MS);
    };

    pollTimer = setTimeout(() => void poll(), CREW_NOTIFY_FIRST_POLL_MS);
    return () => {
      stopped = true;
      abort.abort();
      if (pollTimer !== undefined) clearTimeout(pollTimer);
      if (flushTimer !== undefined) clearTimeout(flushTimer);
    };
  }, [enabled, preview]);
}
