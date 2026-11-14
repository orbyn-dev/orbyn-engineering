// The Approval log's Routing view (Crew Designs frames `route-log` and `route-log-none`): one row for each routing decision.
import { useState, type ReactNode } from "react";
import { ErrorState, Loading } from "../../../../product/ui/states";
import type { ApiFailure } from "../../../../api/client";
import { toFailure, useAsync } from "../../../../lib/useAsync";
import { crewApi, PAGE } from "./api";
import { Chip } from "./parts";
import { dayLine } from "./ledger";
import { ROUTING_KIND, routingRow, samplesOf, short, type RoutingRow } from "./routing";

export const ROUTING_LINE = "Routing is decided by code from .orbyn/crew.json, before any engine is scored";
const NOTE = "Every routing decision is in the ledger with its rule and the version of crew.json that made it.";

/** One page of the routing decisions, newest first, from `offset`. */
async function page(offset: number, signal?: AbortSignal) {
  const read = await crewApi.approvals({ kind: ROUTING_KIND, offset, limit: PAGE }, signal);
  return { rows: read.rows.flatMap((row) => routingRow(row) ?? []), next: read.next_offset ?? null };
}

/** The routing decisions, newest first, and the number the "Routing" filter shows. */
export function useRoutingRows() {
  return useAsync("crew-routing", (signal) => page(0, signal));
}

function Row({ row }: { row: RoutingRow }) {
  const { shown, more } = samplesOf(row);
  const names = row.classes.map((one) => one.name);
  const none = row.result === "none";
  return (
    <div className="ap-row ap-route-row" role="row">
      <span role="cell" className="ap-num">{row.time}</span>
      <span role="cell">
        <b>{row.task}</b>
        <small>Class {names.length ? `${names.join(" and ")} · ${row.classes.map((one) => one.pattern).join(", ")}` : "Everything else"}</small>
        {shown.length ? <span className="ap-glyphs">{shown.map((path, at) => <code key={`${at}-${path}`}>{path}</code>)}{more ? <small>+{more} more</small> : null}</span> : null}
      </span>
      <span role="cell">
        {row.blocked.length ? row.blocked.map((one) => <small key={`${one.engine}-${one.rule}`} title={one.reason}>{one.engine} · {one.rule}</small>) : <small>No engine blocked</small>}
      </span>
      <span role="cell">
        <b>{row.chosen ?? "No engine"}</b>
        <small>{none ? "Waiting in Needs you" : names.length ? `Allowed by ${names.join(" and ")}` : "Allowed: any engine"}</small>
      </span>
      <span role="cell"><span className="ap-num">{short(row.commit)}</span><small>{row.blob ? `blob ${short(row.blob)}` : ""}</small></span>
      <span role="cell">{none ? <Chip tone="am">No engine allowed</Chip> : <Chip tone="ok">Routed</Chip>}</span>
    </div>
  );
}

export function RoutingLog({ rows, more }: { rows: RoutingRow[]; more?: ReactNode }) {
  const none = rows.some((row) => row.result === "none");
  return (
    <div className="ap-main" data-frame={none ? "route-log-none" : "route-log"}>
      <div className="ap-title">
        <h2>Approval log</h2>
        <span>Routing{dayLine(rows[0]?.at) ? ` · ${dayLine(rows[0]?.at)}` : ""}</span>
      </div>
      <div className="ap-scroll" tabIndex={0} role="region" aria-label="Routing decisions">
        <div className="ap-log ap-routing" role="table" aria-label="Routing decisions">
          <div className="ap-row ap-route-row ap-head" role="row">
            {["Time", "Task, class and sample paths", "Blocked by a rule", "Chosen", "crew.json", "Result"].map((name) => <span key={name} role="columnheader">{name}</span>)}
          </div>
          {rows.map((row) => <Row key={row.id} row={row} />)}
        </div>
        {more}
      </div>
      <div className="ap-legend">
        <span>Result:</span>
        <Chip tone="ok">Routed</Chip>
        {none ? <Chip tone="am">No engine allowed</Chip> : null}
        <span>{NOTE}</span>
      </div>
    </div>
  );
}

export function RoutingView({ read }: { read: ReturnType<typeof useRoutingRows> }) {
  const [extra, setExtra] = useState<{ rows: RoutingRow[]; next: number | null } | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<ApiFailure | null>(null);
  if (read.status === "loading") return <Loading label="Reading the routing decisions" rows={4} />;
  if (read.status === "error") return <ErrorState title="Can't read the routing decisions" failure={read.error} onRetry={read.reload} />;
  const next = extra ? extra.next : read.data.next;
  const rows = [...read.data.rows, ...(extra?.rows ?? [])];
  const load = () => {
    if (next === null || busy) return;
    setBusy(true);
    setFailure(null);
    void page(next).then((more) => setExtra({ rows: [...(extra?.rows ?? []), ...more.rows], next: more.next }), (error: unknown) => setFailure(toFailure(error))).finally(() => setBusy(false));
  };
  const more = (
    <>
      {failure ? <ErrorState title="Can't read more routing decisions" failure={failure} onRetry={load} /> : null}
      {next !== null && !failure ? <button type="button" className="ap-more" disabled={busy} onClick={load}>{busy ? "Loading" : "Load more"}</button> : null}
    </>
  );
  return <RoutingLog rows={rows} more={more} />;
}
