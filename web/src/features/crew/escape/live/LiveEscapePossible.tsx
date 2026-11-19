import { useState } from "react";
import type { Project } from "../../../../api/types";
import { ApiFailure, NETWORK_ERROR } from "../../../../api/client";
import { Loading } from "../../../../product/ui/states";
import type { EscapeRow } from "../../../../generated/EscapeRow";
import { Frame } from "../../shared/Frame";
import { Btn, Ic } from "../../shared/parts";
import { Planned } from "../../capacity/Planned";
import { count, dayMoment, sourceLabel, WHY } from "./model";
import { EscapeTable, ProjectChip, StateChip } from "./parts";
import { escapeApi } from "./api";
import { useEscapeLog, usePossible } from "./useEscapes";

/** What a card says under its acts when a write could not be made: the service's own plain words, so nothing is invented here. */
const why = (failure: ApiFailure) => (failure.code === NETWORK_ERROR ? failure.message : failure.code === "stale" ? failure.message : `${failure.code}: ${failure.message}`);

/** One possible escape: a question, not a failure, so it is neutral and blocks nothing. The two acts are the person's alone. */
function Card({ row, busy, onDecide }: { row: EscapeRow; busy: boolean; onDecide(id: string, kind: "confirm" | "dismiss"): void }) {
  const against = `${row.engine} · ${row.areas.join(" · ")}`;
  return (
    <div className="cx-fbx">
      <div className="cx-t14r">
        <b>{row.line}</b>
        <StateChip row={row} />
        <span className="cx-apchip">{sourceLabel(row.source)}</span>
      </div>
      <p>{row.engine} · {row.areas.join(" · ")} · found {dayMoment(row.found_unix)}</p>
      <div className="cx-fkv">
        <span>Why this one</span>
        <b>{row.reason ?? row.line}</b>
        <span>If you confirm</span>
        <b>It becomes a confirmed escape and counts against {against}. You can then set its gap and ask Crew to propose a check</b>
        <span>If you dismiss</span>
        <b>Nothing is counted and this merge is not offered again. There is no undo in v1</b>
      </div>
      <div className="cx-t14r">
        <button type="button" className="cx-btn" disabled={busy} onClick={() => onDecide(row.id, "confirm")}>
          Confirm
        </button>
        <button type="button" className="cx-ghost" disabled={busy} onClick={() => onDecide(row.id, "dismiss")}>
          Dismiss
        </button>
        <Planned why={WHY.queue} />
        <Btn className="cx-ghost" reason={`Planned: ${WHY.queue}`}>Open the merge</Btn>
      </div>
    </div>
  );
}

/**
 * Crew Designs v79, `escape-possible`: what Crew's scan found and nobody has decided, on the real possible read, with the
 * confirmed escapes of the same project below. Confirm and Dismiss are the person's own clicks and the only writes this
 * screen ever sends; both are final in v1, so each asks for no reason. The scan itself runs on the service's tick: this
 * page only reads.
 */
export function LiveEscapePossible({ project, name, projects, onPick }: { project: string; name: string; projects: readonly Project[]; onPick(id: string): void }) {
  const possible = usePossible(project);
  const confirmed = useEscapeLog(project, "confirmed");
  const [busy, setBusy] = useState<string | null>(null);
  const [refused, setRefused] = useState<{ id: string; why: string; stale: boolean } | null>(null);
  const rows = possible.status === "ready" ? possible.data.rows : null;
  const total = possible.status === "ready" ? possible.data.total : 0;

  const decide = (id: string, kind: "confirm" | "dismiss") => {
    const row = rows?.find((one) => one.id === id);
    if (!row) return;
    setBusy(id);
    setRefused(null);
    const body = { project, seen: { state: row.state, gap: row.gap } };
    void (kind === "confirm" ? escapeApi.confirm(id, body) : escapeApi.dismiss(id, body))
      .then(() => {
        setBusy(null);
        void possible.reload();
        void confirmed.reload();
      })
      .catch((error: unknown) => {
        setBusy(null);
        const failure = error instanceof ApiFailure ? error : new ApiFailure("unexpected", "Something went wrong.", 0);
        setRefused({ id, why: why(failure), stale: failure.code === "stale" });
        // A refusal may mean the page's copy is out of date: read both lists again.
        void possible.reload();
        void confirmed.reload();
      });
  };

  return (
    <Frame live>
      <div className="cx-cwpg">
        <div className="cx-flcr">
          <span>Dashboard</span>
          <Ic n="right" sm />
          <b>Escape log</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Escape log</b>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <ProjectChip name={name} projects={projects} current={project} onPick={onPick} />
            </span>
          </div>
          {possible.status === "loading" ? <Loading label="Reading possible escapes" rows={4} /> : null}
          {possible.status === "error" ? (
            <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
              <Ic n="alert" sm />
              <span>Can’t read the possible escapes on this device. Nothing was changed, and no escape was counted or removed.</span>
              <span className="cx-acts">
                <button type="button" className="cx-btn" onClick={() => void possible.reload()}>Try again</button>
              </span>
            </div>
          ) : null}
          {rows ? (
            <>
              <div className="cx-cwbn">
                <Ic n="info" sm />
                <span>
                  {total === 0 ? (
                    <b>No possible escapes.</b>
                  ) : (
                    <b>
                      {count(total)} possible {total === 1 ? "escape" : "escapes"}.
                    </b>
                  )}{" "}
                  Crew is not sure. They aren’t counted unless you confirm them.
                </span>
              </div>
              {rows.map((row) => (
                <div key={row.id} style={{ display: "contents" }}>
                  <Card row={row} busy={busy === row.id} onDecide={decide} />
                  {refused?.id === row.id ? (
                    <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
                      <Ic n="alert" sm />
                      <span>{refused.why}{refused.stale ? " Look again." : ""}</span>
                    </div>
                  ) : null}
                </div>
              ))}
            </>
          ) : null}
          {confirmed.status === "ready" ? (
            <>
              <h5 className="cx-t10h5">
                Confirmed escapes <span className="cx-t10r">{count(confirmed.data.confirmed)} in the last {confirmed.data.window_days} days</span>
              </h5>
              <EscapeTable
                rows={confirmed.data.rows}
                onOpen={(id) => {
                  window.location.hash = `#/preview/crew/escape-detail?id=${encodeURIComponent(id)}`;
                }}
              />
            </>
          ) : null}
        </div>
      </div>
    </Frame>
  );
}
