// The escape log client (slice 4k): the three reads are Local class and Preview-gated, so a load, a refresh and a retry may
// all send them. The person's writes are Person-only and are never sent by a load, a refresh or a retry of a read: only a
// person's own click may call confirm, dismiss and the gap. The check routes (propose, accept, approve, retry, withdraw)
// belong to the check screens and are not sent here.
import { ApiFailure, request } from "../../../../api/client";
import type { EscapeDecide } from "../../../../generated/EscapeDecide";
import type { EscapeDetail } from "../../../../generated/EscapeDetail";
import type { EscapeGapSet } from "../../../../generated/EscapeGapSet";
import type { EscapeLog } from "../../../../generated/EscapeLog";
import type { EscapePossible } from "../../../../generated/EscapePossible";
import type { EscapeRow } from "../../../../generated/EscapeRow";
import type { EscapeState } from "../../../../generated/EscapeState";

/** The same shape the service's every crew id check accepts. */
const validEscapeId = (id: string): boolean => id.length <= 96 && /^[A-Za-z0-9:_-][A-Za-z0-9:_.-]*$/.test(id);

const query = (project: string) => `project=${encodeURIComponent(project)}`;
const path = (id: string): string => {
  if (!validEscapeId(id)) throw new ApiFailure("invalid", "That escape’s id is not valid.", 0);
  return `/api/crew/escapes/${encodeURIComponent(id)}`;
};

export const escapeApi = {
  /** One project's escape log: counts by engine for the stored window, and one page of rows, newest found first. `state` filters the rows. */
  log: (project: string, state?: EscapeState, signal?: AbortSignal) =>
    request<EscapeLog>("GET", `/api/crew/escapes?${query(project)}${state ? `&state=${state}` : ""}`, undefined, signal),
  /** What Crew's scan found and nobody has decided. A read never writes: the scan runs on the service's own tick. */
  possible: (project: string, signal?: AbortSignal) =>
    request<EscapePossible>("GET", `/api/crew/escapes/possible?${query(project)}`, undefined, signal),
  /** One escape and its trail, oldest first. */
  detail: (id: string, signal?: AbortSignal) => request<EscapeDetail>("GET", path(id), undefined, signal),
  /** A person confirms one possible escape. Their own click sends it; the answer is the escape as it now stands. */
  confirm: (id: string, body: EscapeDecide) => request<EscapeRow>("POST", `${path(id)}/confirm`, body),
  /** A person dismisses one possible escape. Their own click sends it; the answer is the escape as it now stands. */
  dismiss: (id: string, body: EscapeDecide) => request<EscapeRow>("POST", `${path(id)}/dismiss`, body),
  /** A person sets the gap of a possible or confirmed escape. Their own click sends it. */
  gap: (id: string, body: EscapeGapSet) => request<EscapeRow>("POST", `${path(id)}/gap`, body),
};
