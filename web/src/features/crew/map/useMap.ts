import { useRef, useState } from "react";
import type { ApiFailure } from "../../../api/client";
import { toFailure, useAsync } from "../../../lib/useAsync";
import type { FlowMessage } from "../../../generated/FlowMessage";
import { mapApi } from "./api";
import { memberFilterable } from "./model";

export const useCrewMap = (crew: string) => useAsync(`crew-map:${crew}`, (signal) => mapApi.map(crew, signal));
export const useCrewFacts = (crew: string) => useAsync(`crew-facts:${crew}`, (signal) => mapApi.facts(crew, signal));

/** One crew's messages (all of them, or one member's) and the pages "+N more" added. A page is a read. */
export function useCrewMessages(crew: string, member: string | null) {
  // A member whose name the service would refuse is never sent: nothing is read, and the list says it cannot be filtered.
  const filterable = member === null || memberFilterable(member);
  const first = useAsync(`crew-messages:${crew}|${member ?? ""}`, (signal) => (filterable ? mapApi.messages(crew, member, 0, signal) : Promise.resolve({ messages: [], total: 0, next_offset: null })));
  const [more, setMore] = useState<{ of: unknown; rows: FlowMessage[]; next: number | null } | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState<ApiFailure | null>(null);
  // A second call while a page is on its way sends nothing: a ref is read at once, state only after the next render.
  const inFlight = useRef(false);
  const page = first.data;
  const mine = more && more.of === page ? more : null;
  const rows = [...(page?.messages ?? []), ...(mine?.rows ?? [])];
  const next = mine ? mine.next : (page?.next_offset ?? null);
  const showMore = async () => {
    if (!page || next === null || inFlight.current || !filterable) return;
    inFlight.current = true;
    setBusy(true);
    setFailed(null);
    try {
      const read = await mapApi.messages(crew, member, next);
      setMore({ of: page, rows: [...(mine?.rows ?? []), ...read.messages], next: read.next_offset ?? null });
    } catch (error) {
      setFailed(toFailure(error));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
  };
  return { first, rows, total: page?.total ?? 0, next, showMore, busy, failed, filterable };
}
