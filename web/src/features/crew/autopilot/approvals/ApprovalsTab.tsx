import { useCallback, useEffect, useRef, useState } from "react";
import { navigate } from "../../../../product/state/route";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { toFailure } from "../../../../lib/useAsync";
import type { ApiFailure } from "../../../../api/client";
import { useAsync } from "../../../../lib/useAsync";
import { crewApi, PAGE } from "./api";
import { ApproveMyself } from "./ApproveMyself";
import { ApprovalTable } from "./ApprovalRows";
import { DecisionDrawer, type DrawerActions } from "./DecisionDrawer";
import { HardDeniedDialog } from "./HardDenied";
import { LEDGER_NOTE, LEGEND, MODE_LINE, GUARD_LINE } from "./copy";
import type { ApprovalFilter, ApprovalPage, ApprovalRow } from "./model";
import { Chip, ResultChip } from "./parts";
import { Tripwire } from "./Tripwire";

const FILTERS: { id: ApprovalFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "allowed", label: "Allowed" },
  { id: "denied_rule", label: "Denied by rule" },
  { id: "denied_lead", label: "Denied by lead" },
  { id: "keys_disagree", label: "Keys disagree" },
  { id: "person_only", label: "Person-only" },
];

interface Loaded {
  page: ApprovalPage;
  rows: ApprovalRow[];
}

/** The log as pages: the first read, then more as the list is scrolled. The totals stay the real ones. */
function useApprovalLog(filter: ApprovalFilter, q: string) {
  const [state, setState] = useState<{ status: "loading" } | { status: "error"; failure: ApiFailure } | ({ status: "ready" } & Loaded)>({ status: "loading" });
  const [tick, setTick] = useState(0);
  const more = useRef(false);
  useEffect(() => {
    const abort = new AbortController();
    void crewApi.approvals({ filter, q, offset: 0 }, abort.signal).then(
      (page) => setState({ status: "ready", page, rows: page.rows }),
      (error: unknown) => {
        if (!abort.signal.aborted) setState({ status: "error", failure: toFailure(error) });
      },
    );
    return () => abort.abort();
  }, [filter, q, tick]);
  const loadMore = useCallback(() => {
    if (state.status !== "ready" || more.current || state.rows.length >= state.page.total) return;
    more.current = true;
    void crewApi.approvals({ filter, q, offset: state.rows.length }).then(
      (page) => setState((now) => (now.status === "ready" ? { status: "ready", page, rows: [...now.rows, ...page.rows] } : now)),
      () => undefined,
    ).finally(() => {
      more.current = false;
    });
  }, [state, filter, q]);
  return { state, loadMore, reload: () => setTick((n) => n + 1) };
}

export function ApprovalsTab() {
  const [filter, setFilter] = useState<ApprovalFilter>("all");
  const [q, setQ] = useState("");
  const { state, loadMore, reload } = useApprovalLog(filter, q);
  const stops = useAsync("crew-stops", (signal) => crewApi.stops(signal), { keep: true });
  const [selected, setSelected] = useState<string | null>(null);
  const [myself, setMyself] = useState<ApprovalRow | null>(null);
  const [hardList, setHardList] = useState(false);
  const rowsRef = useRef<HTMLDivElement>(null);

  const close = useCallback(() => {
    setSelected(null);
    // Focus goes back to the row that opened the drawer.
    requestAnimationFrame(() => rowsRef.current?.querySelector<HTMLElement>('[aria-pressed="true"], .ap-open')?.focus());
  }, []);

  if (state.status === "loading") return <Loading label="Reading the approval log" rows={5} />;
  if (state.status === "error") return <ErrorState title="Can't read the approval log" failure={state.failure} onRetry={reload} />;

  const { page, rows } = state;
  const row = rows.find((one) => one.id === selected) ?? null;
  const act: DrawerActions = {
    approveMyself: setMyself,
    approveOnce: (one) => void crewApi.approveOnce(one.id).then(() => { setSelected(null); reload(); }),
    deny: (one) => void crewApi.deny(one.id).then(() => { setSelected(null); reload(); }),
    dismiss: (one) => void crewApi.dismiss(one.id).then(() => { setSelected(null); reload(); }),
    seeHardDenied: () => setHardList(true),
    signInSecondEngine: () => navigate({ name: "settings", section: "providers" }),
    close,
  };
  const long = page.counts.all > PAGE;
  const guarded = rows.some((one) => one.hard === "guard" || one.hard === "guard_two_keys");
  const line = (rows.some((one) => one.result === "held_you") ? GUARD_LINE : MODE_LINE)[page.mode];

  return (
    <section className="ap-page" aria-label="Approvals">
      {(stops.data ?? []).map((stop) => <Tripwire key={stop.taskId} stop={stop} />)}
      <div className="ap-bar">
        <Chip tone="plain">{line}</Chip>
      </div>
      {page.counts.all === 0 ? (
        <Empty title="No requests yet">
          <p>When an agent asks to run a command, install something or reach the network, the request appears here with how it was decided, and who decided each part.</p>
        </Empty>
      ) : (
        <div className="ap-split">
          <div className="ap-main">
            <div className="ap-title">
              <h2>Approval log</h2>
              {page.day ? <span>{page.day}</span> : null}
              {guarded ? <span>Guard · production-linked targets</span> : null}
            </div>
            {long ? (
              <div className="ap-tool">
                <input type="search" className="ap-search" aria-label="Find a request" placeholder="Find a request" value={q} onChange={(event) => setQ(event.target.value)} />
                <div role="group" aria-label="Filter by result" className="ap-filters">
                  {FILTERS.map((one) => (
                    <button key={one.id} type="button" className="ap-fil" aria-pressed={filter === one.id} onClick={() => setFilter(one.id)}>
                      {one.label} <span className="ap-num">{page.counts[one.id]}</span>
                    </button>
                  ))}
                </div>
                <span className="ap-showing" role="status">Showing 1 to {rows.length}</span>
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
              <ApprovalTable rows={rows} selected={selected} onSelect={setSelected} />
            </div>
            <div className="ap-legend">
              <span>Result:</span>
              {LEGEND.map((result) => <ResultChip key={result} result={result} />)}
              <span>{LEDGER_NOTE}</span>
            </div>
          </div>
          {row ? <DecisionDrawer row={row} act={act} mode={page.mode} /> : null}
        </div>
      )}
      {myself ? (
        <ApproveMyself
          row={myself}
          onClose={() => setMyself(null)}
          onApprove={async () => {
            await crewApi.approveOnce(myself.id);
            setSelected(null);
            reload();
          }}
        />
      ) : null}
      {hardList ? <HardDeniedDialog onClose={() => setHardList(false)} /> : null}
    </section>
  );
}
