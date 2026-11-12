import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { navigate } from "../../../../product/state/route";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { toFailure } from "../../../../lib/useAsync";
import type { ApiFailure } from "../../../../api/client";
import type { AutopilotRule } from "../../../../generated/AutopilotRule";
import { crewApi } from "./api";
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
  next: number | null;
  /** How many ledger rows were read, merge rows included: the next page starts here. */
  read: number;
}

/** The ledger as pages: the first read, then more as the list is scrolled. */
function useApprovalLog(rules: AutopilotRule[] | null) {
  const [state, setState] = useState<{ status: "loading" } | { status: "error"; failure: ApiFailure } | ({ status: "ready" } & Loaded)>({ status: "loading" });
  const [tick, setTick] = useState(0);
  const more = useRef(false);
  const rulesRef = useRef<AutopilotRule[]>(rules ?? []);
  useEffect(() => {
    rulesRef.current = rules ?? [];
  });
  const waiting = rules === null;
  useEffect(() => {
    if (waiting) return;
    const abort = new AbortController();
    void crewApi.approvals({ offset: 0 }, abort.signal).then(
      (page) => setState({ status: "ready", rows: page.rows.flatMap((row) => rowOf(row, rulesRef.current) ?? []), total: page.total, next: page.next_offset ?? null, read: page.rows.length }),
      (error: unknown) => {
        if (!abort.signal.aborted) setState({ status: "error", failure: toFailure(error) });
      },
    );
    return () => abort.abort();
  }, [tick, waiting]);
  const loadMore = useCallback(() => {
    if (state.status !== "ready" || more.current || state.next === null) return;
    more.current = true;
    void crewApi.approvals({ offset: state.next }).then(
      (page) =>
        setState((now) =>
          now.status === "ready" ? { status: "ready", rows: [...now.rows, ...page.rows.flatMap((row) => rowOf(row, rulesRef.current) ?? [])], total: page.total, next: page.next_offset ?? null, read: now.read + page.rows.length } : now,
        ),
      () => undefined,
    ).finally(() => {
      more.current = false;
    });
  }, [state]);
  return { state, loadMore, reload: () => setTick((n) => n + 1) };
}

const matches = (row: ApprovalRow, filter: ApprovalFilter, q: string) => {
  const byFilter = filter === "all" ? true : filter === "allowed" ? row.result === "allowed" || row.result.startsWith("approved") : filter === row.result;
  return byFilter && (!q.trim() || `${row.title} ${row.taskId ?? ""}`.toLowerCase().includes(q.trim().toLowerCase()));
};

export function ApprovalsTab() {
  const guards = useAutopilotGuards();
  const modes = useAutopilotModes();
  const { state, loadMore, reload } = useApprovalLog(guards.status === "loading" ? null : (guards.data?.always_denied ?? []));
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
  const long = state.total > rows.length || rows.length > 18;

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
                if (el.scrollTop + el.clientHeight >= el.scrollHeight - 80) loadMore();
              }}
            >
              <ApprovalTable rows={shown} selected={selected} onSelect={setSelected} />
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
