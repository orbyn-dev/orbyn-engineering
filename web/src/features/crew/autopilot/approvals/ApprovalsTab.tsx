import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { navigate } from "../../../../product/state/route";
import { useWorkspace } from "../../../../product/state/workspace";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { toFailure } from "../../../../lib/useAsync";
import type { ApiFailure } from "../../../../api/client";
import type { AutopilotRule } from "../../../../generated/AutopilotRule";
import { crewApi, PAGE } from "./api";
import { ApproveMyself } from "./ApproveMyself";
import { ApprovalTable } from "./ApprovalRows";
import { DecisionDrawer, type DrawerActions } from "./DecisionDrawer";
import { HardDeniedDialog } from "./HardDenied";
import { LEDGER_NOTE, LEGEND, MODE_LINE, GUARD_LINE } from "./copy";
import { dayLine, rowOf } from "./ledger";
import type { ApprovalFilter, ApprovalRow } from "./model";
import { Chip, ResultChip } from "./parts";
import { StopsList } from "./Tripwire";
import { useAutopilotGuards, useAutopilotModes } from "./useCrew";

const FILTERS: { id: ApprovalFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "allowed", label: "Allowed" },
  { id: "denied_rule", label: "Denied by rule" },
  { id: "denied_lead", label: "Denied by lead" },
  { id: "keys_disagree", label: "Keys disagree" },
  { id: "person_only", label: "Person-only" },
];

interface Loaded {
  rows: ApprovalRow[];
  total: number;
  /** Where the next read starts; null once the ledger is read to its end. */
  next: number | null;
}

/**
 * Reads the ledger until `PAGE` more rows can be drawn or it ends: the ledger also holds merge rows and trip
 * wires that this log leaves out, so one page of it may show few rows or none.
 */
async function fill(from: number, rules: AutopilotRule[], roots: string[], signal?: AbortSignal): Promise<Loaded> {
  const rows: ApprovalRow[] = [];
  let next: number | null = from;
  let total = 0;
  while (next !== null && rows.length < PAGE) {
    const page = await crewApi.approvals({ offset: next }, signal);
    rows.push(...page.rows.flatMap((row) => rowOf(row, rules, roots) ?? []));
    total = page.total;
    next = page.next_offset ?? null;
  }
  return { rows, total, next };
}

function useApprovalLog(rules: AutopilotRule[] | null, roots: string[]) {
  const [state, setState] = useState<{ status: "loading" } | { status: "error"; failure: ApiFailure } | ({ status: "ready" } & Loaded)>({ status: "loading" });
  const [more, setMore] = useState<{ busy: boolean; failure: ApiFailure | null }>({ busy: false, failure: null });
  const [tick, setTick] = useState(0);
  const busy = useRef(false);
  const rulesRef = useRef<AutopilotRule[]>(rules ?? []);
  const rootsRef = useRef(roots);
  useEffect(() => {
    rulesRef.current = rules ?? [];
    rootsRef.current = roots;
  });
  const waiting = rules === null;
  useEffect(() => {
    if (waiting) return;
    const abort = new AbortController();
    void fill(0, rulesRef.current, rootsRef.current, abort.signal).then(
      (loaded) => setState({ status: "ready", ...loaded }),
      (error: unknown) => {
        if (!abort.signal.aborted) setState({ status: "error", failure: toFailure(error) });
      },
    );
    return () => abort.abort();
  }, [tick, waiting]);
  const loadMore = useCallback(() => {
    if (state.status !== "ready" || busy.current || state.next === null) return;
    busy.current = true;
    setMore({ busy: true, failure: null });
    void fill(state.next, rulesRef.current, rootsRef.current).then(
      (loaded) => {
        setState((now) => (now.status === "ready" ? { status: "ready", rows: [...now.rows, ...loaded.rows], total: loaded.total, next: loaded.next } : now));
        setMore({ busy: false, failure: null });
      },
      (error: unknown) => setMore({ busy: false, failure: toFailure(error) }),
    ).finally(() => {
      busy.current = false;
    });
  }, [state]);
  return { state, more, loadMore, reload: () => setTick((n) => n + 1) };
}

const matches = (row: ApprovalRow, filter: ApprovalFilter, q: string) => {
  const byFilter = filter === "all" ? true : filter === "allowed" ? row.result === "allowed" || row.result.startsWith("approved") : filter === row.result;
  return byFilter && (!q.trim() || `${row.title} ${row.taskId ?? ""}`.toLowerCase().includes(q.trim().toLowerCase()));
};

export function ApprovalsTab() {
  const guards = useAutopilotGuards();
  const modes = useAutopilotModes();
  const projects = useWorkspace((now) => now.projects.data);
  const roots = useMemo(() => (projects ?? []).map((project) => project.path), [projects]);
  const { state, more, loadMore, reload } = useApprovalLog(guards.status === "loading" ? null : (guards.data?.always_denied ?? []), roots);
  const [filter, setFilter] = useState<ApprovalFilter>("all");
  const [q, setQ] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [myself, setMyself] = useState<ApprovalRow | null>(null);
  const [hardList, setHardList] = useState(false);
  const rowsRef = useRef<HTMLDivElement>(null);
  const rows = useMemo(() => (state.status === "ready" ? state.rows : []), [state]);
  const shown = useMemo(() => rows.filter((row) => matches(row, filter, q)), [rows, filter, q]);
  const counts = useMemo(() => Object.fromEntries(FILTERS.map((one) => [one.id, rows.filter((row) => matches(row, one.id, "")).length])) as Record<ApprovalFilter, number>, [rows]);

  const close = useCallback(() => {
    setSelected(null);
    // Focus goes back to the row that opened the drawer.
    requestAnimationFrame(() => rowsRef.current?.querySelector<HTMLElement>(".ap-open")?.focus());
  }, []);

  if (state.status === "loading") return <Loading label="Reading the approval log" rows={5} />;
  if (state.status === "error") return <ErrorState title="Can't read the approval log" failure={state.failure} onRetry={reload} />;

  const row = rows.find((one) => one.id === selected) ?? null;
  const act: DrawerActions = {
    approveMyself: setMyself,
    seeHardDenied: () => setHardList(true),
    signInSecondEngine: () => navigate({ name: "settings", section: "providers" }),
    close,
  };
  const mode = modes.data?.effective_default ?? "full";
  const guarded = rows.some((one) => one.hard === "guard" || one.hard === "guard_two_keys");
  const line = (rows.some((one) => one.result === "held_you") ? GUARD_LINE : MODE_LINE)[mode];
  const long = state.next !== null || rows.length > PAGE;

  return (
    <section className="ap-page" aria-label="Approvals">
      <StopsList />
      <div className="ap-bar">
        <Chip tone="plain">{line}</Chip>
      </div>
      {rows.length === 0 ? (
        <Empty title="No requests yet">
          <p>When an agent asks to run a command, install something or reach the network, the request appears here with how it was decided, and who decided each part.</p>
        </Empty>
      ) : (
        <div className="ap-split">
          <div className="ap-main">
            <div className="ap-title">
              <h2>Approval log</h2>
              {dayLine(rows[0]?.at ?? undefined) ? <span>{dayLine(rows[0]?.at ?? undefined)}</span> : null}
              {guarded ? <span>Guard · production-linked targets</span> : null}
            </div>
            {long ? (
              <div className="ap-tool">
                <input type="search" className="ap-search" aria-label="Find a request" placeholder="Find a request" value={q} onChange={(event) => setQ(event.target.value)} />
                <div role="group" aria-label="Filter by result" className="ap-filters">
                  {FILTERS.map((one) => (
                    <button key={one.id} type="button" className="ap-fil" aria-pressed={filter === one.id} onClick={() => setFilter(one.id)}>
                      {one.label} <span className="ap-num">{counts[one.id]}</span>
                    </button>
                  ))}
                </div>
                <span className="ap-showing" role="status">Showing 1 to {shown.length} of the {rows.length} loaded requests</span>
              </div>
            ) : null}
            <div
              ref={rowsRef}
              className="ap-scroll"
              tabIndex={0}
              role="region"
              aria-label="Requests"
              onScroll={(event) => {
                const el = event.currentTarget;
                if (!more.failure && el.scrollTop + el.clientHeight >= el.scrollHeight - 80) loadMore();
              }}
            >
              <ApprovalTable rows={shown} selected={selected} onSelect={setSelected} />
              {more.failure ? <ErrorState title="Can't read more requests" failure={more.failure} onRetry={loadMore} /> : null}
              {state.next !== null && !more.failure ? (
                <button type="button" className="ap-more" disabled={more.busy} onClick={loadMore}>{more.busy ? "Loading" : "Load more"}</button>
              ) : null}
            </div>
            <div className="ap-legend">
              <span>Result:</span>
              {LEGEND.map((result) => <ResultChip key={result} result={result} />)}
              <span>{LEDGER_NOTE}</span>
            </div>
          </div>
          {row ? <DecisionDrawer row={row} act={act} mode={mode} /> : null}
        </div>
      )}
      {myself ? <ApproveMyself row={myself} onClose={() => setMyself(null)} /> : null}
      {hardList ? <HardDeniedDialog onClose={() => setHardList(false)} /> : null}
    </section>
  );
}
