// The commands the guard holds for the person: the held list becomes log rows, and the person's three answers
// (approve once, deny, dismiss) go to the service. Approving is the only answer that needs proof a person is
// present: the person's own click asks the desktop shell, which shows its native dialog and hands back a one-time
// nonce only after the person clicks Approve once there; the nonce then goes on the route once. Deny and Dismiss
// need none. Nothing here keeps a nonce, and nothing asks again after a refusal: the person clicks again.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useConnection } from "../../../../product/state/connection";
import { useAsync, toFailure } from "../../../../lib/useAsync";
import { NETWORK_ERROR } from "../../../../api/client";
import { askPresence, canAskPresence, PresenceFailure } from "../../../../api/shell";
import type { AutopilotHeld } from "../../../../generated/AutopilotHeld";
import type { AutopilotHeldList } from "../../../../generated/AutopilotHeldList";
import { crewApi } from "./api";
import type { ApprovalRow, HeldInfo } from "./model";
import { isPersonsClick, useDesktopApp, type Gesture } from "./presence";

/** How often the open list is read again while the window is shown: a hold appears, expires or is answered elsewhere. */
export const HELD_REFRESH_MS = 15_000;

const SHORT = 90;

const clock = (ms: number) => {
  const at = new Date(ms);
  return `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
};

/** The command in a line for the log's row: the first line, cut where it gets long. */
export function titleOfCommand(command: string): string {
  const line = command.split("\n")[0]?.trim() ?? "";
  if (!line) return "A command";
  return line.length > SHORT ? `${line.slice(0, SHORT - 1).trimEnd()}…` : line;
}

type Closed = "expired" | "decided";

/** Where a hold stands as the card tells it: waiting for you, past its time, or answered. */
export type Phase = "open" | "expired" | "decided";

export function phaseOf(held: HeldInfo, closed?: Closed): Phase {
  if (closed === "expired" || held.state === "expired") return "expired";
  if (closed === "decided" || held.state === "denied" || held.state === "approved" || held.state === "used" || held.state === "dismissed") return "decided";
  return "open";
}

/**
 * Who decided and what, in the designs' words, with the time when it is known. The held list does not say who decided, so
 * "by you on this device" is said only of a denial this window sent (`byYou`); another denial is plainly "Denied".
 */
export function decidedLine(state: HeldInfo["state"], at: string | null, byYou: boolean): { sentence: string | null; fact: string | null } {
  const what = state === "denied" ? (byYou ? "Denied by you on this device" : "Denied") : state === "approved" || state === "used" ? "Approved by you" : state === "dismissed" ? "Dismissed by you on this device" : null;
  if (!what) return { sentence: null, fact: null };
  return { sentence: at ? `${what} at ${at}.` : `${what}.`, fact: at ? `${at} · ${what}` : what };
}

/** One held item as a row of the log. A dismissed one is gone from the log: dismissing closes the request. */
export function heldRow(item: AutopilotHeld, approvalSecs: number, now: number, decidedAt: string | null, expired: boolean): ApprovalRow | null {
  const state = expired && item.state === "held" ? "expired" : item.state;
  if (state === "dismissed") return null;
  const since = clock(now - item.age_secs * 1000);
  // An approval lasts `approval_secs`; the time left says when it was given.
  const at = decidedAt ?? (state === "approved" ? clock(now - Math.max(0, approvalSecs - item.expires_in_secs) * 1000) : null);
  // A denial is "by you" only when this window sent it (it recorded the time); the list does not say who denied.
  const byYou = state !== "denied" || decidedAt !== null;
  const result = state === "held" ? "held_you" : state === "expired" ? "held_expired" : state === "denied" ? (byYou ? "denied_you" : "denied_held") : "approved_you";
  const reason = item.reason.trim();
  return {
    id: item.id,
    time: since,
    at: new Date(now - item.age_secs * 1000).toISOString(),
    title: titleOfCommand(item.command),
    asker: null,
    taskId: item.task_id,
    branch: null,
    risk: null,
    notes: [],
    facts: [],
    block: item.command,
    note: null,
    needsYou: null,
    mode: null,
    tier: null,
    hard: "guard",
    rule: null,
    lead: reason ? { state: "reason", reason, short: reason.replace(/\.$/, "") } : { state: "not_asked" },
    leadBy: null,
    second: state === "held" ? { state: "yours" } : state === "expired" ? { state: "nobody" } : state === "denied" ? (byYou ? { state: "answered" } : { state: "decided" }) : { state: "you" },
    result,
    decidedInMs: null,
    guard: null,
    held: { id: item.id, state, reason, readable: item.readable, since, decidedAt: at, byYou },
  };
}

/** What a card needs to draw and answer one hold. */
export interface HoldControl {
  /** This window is the desktop app, so it can ask the shell to confirm approving. A browser tab cannot. */
  desktop: boolean;
  /** This window reaches the app from another computer: the person at that computer answers. */
  remote: boolean;
  /** An answer this window sent was refused because the hold ended or was decided. */
  closed: Closed | null;
  /** An answer is on its way. */
  busy: boolean;
  /** The shell's dialog is open (for any hold: one at a time). */
  asking: boolean;
  /** The last approval was not confirmed as the person's: the card says so and stays held. */
  refused: boolean;
  /** Why the last answer did not happen, other than a refusal to confirm. */
  error: string | null;
  /** Approve once. Only a click by the person counts. */
  approve(event: Gesture): void;
  deny(): void;
  dismiss(): void;
}

type Outcome = "done" | "closed" | "failed";

export function useHeldCommands(onAnswered: () => void) {
  const read = useAsync<AutopilotHeldList>("crew-held", (signal) => crewApi.held(signal));
  const { reload } = read;
  const desktop = useDesktopApp();
  const remote = useConnection((state) => state.remote);
  const [closed, setClosed] = useState<Record<string, Closed>>({});
  const [decidedAt, setDecidedAt] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<{ id: string; message: string } | null>(null);
  /** The hold whose approval is in flight, from the click to the route's answer. Set before anything is awaited, so a second click cannot start another. */
  const approving = useRef<string | null>(null);
  const [asking, setAsking] = useState<string | null>(null);
  const [refused, setRefused] = useState<string | null>(null);
  const answered = useRef(onAnswered);
  useEffect(() => {
    answered.current = onAnswered;
  });

  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState !== "hidden") void reload();
    }, HELD_REFRESH_MS);
    return () => clearInterval(timer);
  }, [reload]);

  const list = read.data;
  const at = useMemo(() => Date.now(), [list]); // eslint-disable-line react-hooks/exhaustive-deps -- the list is the moment it was read
  const rows = useMemo(
    () => (list?.items ?? []).flatMap((item) => heldRow(item, list?.approval_secs ?? 600, at, decidedAt[item.id] ?? null, closed[item.id] === "expired") ?? []),
    [list, at, decidedAt, closed],
  );
  const waiting = useMemo(() => rows.filter((row) => row.held?.state === "held").length, [rows]);

  const ended = useCallback(
    (id: string, failure: ReturnType<typeof toFailure>): Outcome | null => {
      // 410: its time ran out or its task ended; 404: the service forgot it (a restart); 409: someone decided it first.
      if (failure.status === 410 || failure.status === 404) {
        setClosed((now) => ({ ...now, [id]: "expired" }));
        void reload();
        return "closed";
      }
      if (failure.status === 409) {
        setClosed((now) => ({ ...now, [id]: "decided" }));
        void reload();
        return "closed";
      }
      return null;
    },
    [reload],
  );

  /** Deny or Dismiss: no password, one request. */
  const answer = useCallback(
    async (id: string, how: "deny" | "dismiss"): Promise<Outcome> => {
      setBusy(id);
      setError(null);
      try {
        await (how === "deny" ? crewApi.denyHeld(id) : crewApi.dismissHeld(id));
        if (how === "deny") setDecidedAt((now) => ({ ...now, [id]: clock(Date.now()) }));
        await reload();
        answered.current();
        return "done";
      } catch (caught) {
        const failure = toFailure(caught);
        const outcome = ended(id, failure);
        if (outcome) return outcome;
        setError({ id, message: failure.code === NETWORK_ERROR ? "Can’t reach the Orbyn service. Nothing was changed. The request is still held." : failure.message });
        return "failed";
      } finally {
        setBusy(null);
      }
    },
    [ended, reload],
  );

  /** The card says it could not confirm the person, and the hold stays as it is. */
  const refuse = useCallback((id: string) => setRefused(id), []);

  /**
   * Approve once: from the person's own click, one request at a time. The shell asks in its native dialog and hands back a
   * nonce only when the person clicks Approve once there; `cancelled` says nothing, any other refusal says it could not
   * confirm the person. The nonce goes to the route once. Nothing here asks again: a refusal ends the attempt.
   */
  const approve = useCallback(
    async (id: string, gesture: Gesture): Promise<void> => {
      if (!isPersonsClick(gesture) || approving.current !== null || !canAskPresence()) return;
      approving.current = id;
      setRefused(null);
      setError(null);
      setAsking(id);
      try {
        let nonce: string;
        try {
          nonce = await askPresence(id);
        } catch (caught) {
          if (!(caught instanceof PresenceFailure && caught.code === "cancelled")) refuse(id);
          return;
        } finally {
          setAsking(null);
        }
        setBusy(id);
        try {
          await crewApi.approveOnce(id, nonce);
          setDecidedAt((now) => ({ ...now, [id]: clock(Date.now()) }));
          await reload();
          answered.current();
        } catch (caught) {
          const failure = toFailure(caught);
          if (ended(id, failure)) return;
          // The service did not see the shell's confirmation: a wrong or used nonce, or a service not started by the desktop app.
          if (failure.code === "presence_required" || failure.code === "desktop_required") return refuse(id);
          // The always-denied list refused it now: the service already closed the hold as denied, and nothing ran.
          if (failure.code === "policy_denied") {
            setClosed((now) => ({ ...now, [id]: "decided" }));
            void reload();
            return;
          }
          if (failure.code === NETWORK_ERROR) {
            // The request may have landed and only the answer been lost: read the list before saying anything about it.
            const fresh = await crewApi.held().then((list) => list, () => null);
            const now = fresh?.items.find((one) => one.id === id)?.state;
            if (!fresh || !now) {
              setError({ id, message: "Couldn’t tell whether it was approved. Check the approval log before trying again." });
              return;
            }
            if (now !== "held") {
              if (now === "approved" || now === "used") {
                setDecidedAt((state) => ({ ...state, [id]: clock(Date.now()) }));
                answered.current();
              } else setClosed((state) => ({ ...state, [id]: now === "expired" ? "expired" : "decided" }));
              void reload();
              return;
            }
            setError({ id, message: "Can’t reach the Orbyn service. Nothing was changed. The request is still held." });
            return;
          }
          setError({ id, message: failure.message });
        } finally {
          setBusy(null);
        }
      } finally {
        approving.current = null;
      }
    },
    [ended, refuse, reload],
  );

  const control = useCallback(
    (row: ApprovalRow): HoldControl => ({
      desktop,
      remote,
      closed: closed[row.id] ?? null,
      busy: busy === row.id,
      asking: asking !== null,
      refused: refused === row.id,
      error: error?.id === row.id ? error.message : null,
      approve: (gesture) => void approve(row.id, gesture),
      deny: () => void answer(row.id, "deny"),
      dismiss: () => void answer(row.id, "dismiss"),
    }),
    [answer, approve, asking, busy, closed, desktop, error, refused, remote],
  );

  /** What a row without a hold (a denied command that "Approve it myself" can still ask about) needs to draw and ask. */
  const presence = { desktop, asking: asking !== null, refused };

  return { rows, waiting, failure: list ? null : read.error, reload, approve, control, presence };
}
