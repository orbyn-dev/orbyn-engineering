import { useEffect } from "react";
import { create } from "zustand";
import type { IssueRef } from "./contract";
import { issuesApi, useApiVersion } from "./api";
import { refKey } from "./model";

/** What the chip, the row markers and the detail card share: every thread's linked issues, read in one request. */
interface LinksStore {
  byThread: Record<string, IssueRef[]>;
  /** The client version the links were read from, or null before the first read. */
  readFrom: number | null;
  failed: boolean;
}

const useLinks = create<LinksStore>(() => ({ byThread: {}, readFrom: null, failed: false }));
const EMPTY: IssueRef[] = [];
let inflight: { version: number; done: Promise<void> } | null = null;

export function resetIssueLinks(): void {
  inflight = null;
  useLinks.setState({ byThread: {}, readFrom: null, failed: false });
}

/** Reads the links again. A read for the same client already in flight is shared. */
export function reloadIssueLinks(version: number): Promise<void> {
  if (inflight?.version === version) return inflight.done;
  const done = issuesApi().links().then(
    (byThread) => { if (inflight?.version === version) useLinks.setState({ byThread, readFrom: version, failed: false }); },
    () => { if (inflight?.version === version) useLinks.setState({ readFrom: version, failed: true }); },
  ).finally(() => { if (inflight?.version === version) inflight = null; });
  inflight = { version, done };
  return done;
}

/** Reads once per client, the first time a consumer shows. */
function useEnsureLinks(): number {
  const version = useApiVersion();
  const readFrom = useLinks((state) => state.readFrom);
  useEffect(() => {
    if (readFrom !== version) void reloadIssueLinks(version);
  }, [readFrom, version]);
  return version;
}

/** A thread's linked issues, oldest link first. Empty until read, and when there are none. */
export function useThreadIssues(threadId: string): { issues: IssueRef[]; reload(): Promise<void> } {
  const version = useEnsureLinks();
  const issues = useLinks((state) => state.byThread[threadId] ?? EMPTY);
  return { issues, reload: () => reloadIssueLinks(version) };
}

/** The thread ids that link this issue, for its Linked threads card. */
export function useIssueThreads(ref: Pick<IssueRef, "tracker" | "scope" | "key">): { threadIds: string[]; loaded: boolean; failed: boolean; retry(): Promise<void> } {
  const version = useEnsureLinks();
  const failed = useLinks((state) => state.failed);
  const byThread = useLinks((state) => state.byThread);
  const loaded = useLinks((state) => state.readFrom !== null);
  const wanted = refKey(ref);
  return { threadIds: Object.keys(byThread).filter((id) => byThread[id]!.some((issue) => refKey(issue) === wanted)), loaded, failed, retry: () => reloadIssueLinks(version) };
}
