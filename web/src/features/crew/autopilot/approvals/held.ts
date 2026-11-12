// The commands the guard holds for the person: the held list becomes log rows, and the person's three answers
// (approve once, deny, dismiss) go to the service. Approving is the only answer that needs proof a person is
// present: the "Confirm it's you" dialog first (the password, sent once), then its one-time nonce on the route.
// Deny and Dismiss never send a password. Nothing here keeps a password or a nonce.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { navigate } from "../../../../product/state/route";
import { useConnection } from "../../../../product/state/connection";
import { useAsync, toFailure } from "../../../../lib/useAsync";
import { NETWORK_ERROR } from "../../../../api/client";
import type { AutopilotHeld } from "../../../../generated/AutopilotHeld";
import type { AutopilotHeldList } from "../../../../generated/AutopilotHeldList";
import { useAccountStanding } from "../../../account";
import { crewApi } from "./api";
import type { ApprovalRow, HeldInfo } from "./model";

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

/** Who decided and what, in the designs' words: "Denied by you on this device", with the time when it is known. */
export function decidedLine(state: HeldInfo["state"], at: string | null): { sentence: string | null; fact: string | null } {
  const what = state === "denied" ? "Denied by you on this device" : state === "approved" || state === "used" ? "Approved by you" : state === "dismissed" ? "Dismissed by you on this device" : null;
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
  const result = state === "held" ? "held_you" : state === "expired" ? "held_expired" : state === "denied" ? "denied_you" : "approved_you";
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
    second: state === "held" ? { state: "yours" } : state === "expired" ? { state: "nobody" } : state === "denied" ? { state: "answered" } : { state: "you" },
    result,
    decidedInMs: null,
    guard: null,
    held: { id: item.id, state, reason, readable: item.readable, since, decidedAt: at },
  };
}

/** What a card needs to draw and answer one hold. */
export interface HoldControl {
  /** The account: Approve once needs a signed-in one. */
  standing: "loading" | "signed_in" | "none";
  /** This window reaches the app from another computer: the person at that computer answers. */
  remote: boolean;
  /** An answer this window sent was refused because the hold ended or was decided. */
  closed: Closed | null;
  /** An answer is on its way. */
  busy: boolean;
  /** Why the last Deny or Dismiss did not happen. */
  error: string | null;
  approve(): void;
  deny(): void;
  dismiss(): void;
  signIn(): void;
}

type Outcome = "done" | "closed" | "failed";

export function useHeldCommands(onAnswered: () => void) {
  const read = useAsync<AutopilotHeldList>("crew-held", (signal) => crewApi.held(signal));
  const { reload } = read;
  const standing = useAccountStanding();
  const remote = useConnection((state) => state.remote);
  const [closed, setClosed] = useState<Record<string, Closed>>({});
  const [decidedAt, setDecidedAt] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<{ id: string; message: string } | null>(null);
  const [noAccount, setNoAccount] = useState(false);
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

  /**
   * Approve once, after "Confirm it's you" made `nonce` for this hold. Resolves `false` when the nonce was not used
   * (the hold ended, or approving is not available), so the dialog gives it up. A network failure throws: the dialog
   * says so and offers Try again.
   */
  const approve = useCallback(
    async (id: string, nonce: string): Promise<boolean> => {
      try {
        await crewApi.approveOnce(id, nonce);
        setDecidedAt((now) => ({ ...now, [id]: clock(Date.now()) }));
        await reload();
        answered.current();
        return true;
      } catch (caught) {
        const failure = toFailure(caught);
        if (ended(id, failure)) return false;
        if (failure.code === "account_disabled" || (failure.code === "policy_denied" && failure.message.startsWith("Approving once is not available"))) {
          setNoAccount(true);
          return false;
        }
        throw caught;
      }
    },
    [ended, reload],
  );

  const control = useCallback(
    (row: ApprovalRow, ask: (row: ApprovalRow) => void): HoldControl => ({
      standing: noAccount ? "none" : standing,
      remote,
      closed: closed[row.id] ?? null,
      busy: busy === row.id,
      error: error?.id === row.id ? error.message : null,
      approve: () => ask(row),
      deny: () => void answer(row.id, "deny"),
      dismiss: () => void answer(row.id, "dismiss"),
      signIn: () => navigate({ name: "settings", section: "account" }),
    }),
    [answer, busy, closed, error, noAccount, remote, standing],
  );

  return { rows, waiting, failure: list ? null : read.error, reload, approve, control };
}
