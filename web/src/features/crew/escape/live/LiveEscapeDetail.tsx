import { useRef, useState } from "react";
import type { EscapeCheckRow } from "../../../../generated/EscapeCheckRow";
import type { EscapeDetail } from "../../../../generated/EscapeDetail";
import type { EscapeEventRow } from "../../../../generated/EscapeEventRow";
import type { EscapeGap } from "../../../../generated/EscapeGap";
import type { EscapeRow } from "../../../../generated/EscapeRow";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { Button } from "../../../../ui/controls";
import { Dialog } from "../../../../ui/Dialog";
import { Frame } from "../../shared/Frame";
import { useCrewHref } from "../../shared/nav";
import { Ic } from "../../shared/parts";
import { Planned } from "../../capacity/Planned";
import { againstOf, checkStateLabel, eventMoment, gapLabel, gapText, nameOf, refusalOf, sourceLabel, stateLabel, WHY } from "./model";
import { escapeApi } from "./api";
import { CheckCard, CheckChip, Commit, short, type CheckAct } from "./check";
import { StateChip } from "./parts";
import { ProposeAction, type Proposal } from "./propose";
import { useEscapeDetail, useEscapeLog } from "./useEscapes";

const mono = { fontFamily: "var(--font-mono)", fontSize: "11px", wordBreak: "break-all", lineHeight: 1.5 } as const;

/** The trail of one escape, oldest first. An event's time is the time alone on the day the escape was found. */
function Timeline({ detail: one }: { detail: EscapeDetail }) {
  const row = one.escape;
  const lastReclassified = one.events.map((event) => event.kind === "reclassified").lastIndexOf(true);
  return (
    <div className="cx-t14tl">
      <div className="cx-h">
        <b style={{ color: "var(--color-text)" }}>Timeline</b>
        <span style={{ color: "var(--color-muted)" }}>Same day unless a date is shown</span>
      </div>
      {one.events.map((event, index) => (
        <EventRow key={event.id} event={event} row={row} latestGap={index === lastReclassified} />
      ))}
      {one.truncated ? <small style={{ color: "var(--color-muted)" }}>Older events are not listed.</small> : null}
    </div>
  );
}

function EventRow({ event, row, latestGap }: { event: EscapeEventRow; row: EscapeRow; latestGap: boolean }) {
  const time = eventMoment(event.at, row.found_unix);
  const against = againstOf(row);
  const icon =
    event.kind === "detected"
      ? row.source === "revert"
        ? "restore"
        : row.source === "post_merge_check"
          ? "eye"
          : row.source === "person_marked"
            ? "user"
            : "edit"
      : event.kind === "confirmed"
        ? "ok"
        : event.kind === "dismissed"
          ? "x"
          : event.kind === "reclassified"
            ? "tree"
            : event.kind === "check_merged"
              ? "ok"
              : event.kind === "check_failed"
                ? "alert"
                : event.kind === "check_withdrawn"
                  ? "x"
                  : "check2";
  const [title, copy] =
    event.kind === "detected"
      ? row.source === "revert"
        ? ["Detected: a revert", `The merge was reverted (cause ${event.cause}). It is a possible escape at once and is not counted yet.`]
        : row.source === "post_merge_check"
          ? ["Detected: a check failed after the merge", `A repository check failed on this device and the merge was not reverted (cause ${event.cause}). It was a possible escape.`]
          : row.source === "person_marked"
            ? ["Detected: you marked it", "You marked this merge. Your mark is the confirm."]
            : ["Detected: a later fix", `A later fix commit changed lines this merge added (cause ${event.cause}). It may be ordinary follow-up work.`]
      : event.kind === "confirmed"
        ? row.source === "person_marked"
          ? ["Confirmed by your mark", `Your mark is the confirm: it was confirmed at once and counts against ${against}.`]
          : ["Confirmed by you", `It now counts as a confirmed escape against ${against}. Final: there is no undo in v1.`]
        : event.kind === "dismissed"
          ? ["Dismissed by you", "Not an escape. Final: there is no undo in v1."]
          : event.kind === "reclassified"
            ? [
                latestGap && row.gap ? `Gap set: ${gapLabel[row.gap]}` : "Gap set",
                event.decided_by === "person" ? "You changed the gap." : "Crew suggested the gap.",
              ]
            : event.kind === "check_proposed"
              ? ["Check proposed", `Cause ${event.cause}.`]
              : event.kind === "check_failed"
                ? ["Check failed", `Cause ${event.cause}.`]
                : event.kind === "check_proven"
                  ? ["Check proven", `Cause ${event.cause}.`]
                  : event.kind === "check_merged"
                    ? ["Check merged", `Cause ${event.cause}.`]
                    : ["Check withdrawn", `Cause ${event.cause}.`];
  return (
    <div>
      <span className="cx-t">{time}</span>
      <Ic n={icon} sm />
      <span>
        <b>{title}</b>
        <small>{copy}</small>
      </span>
    </div>
  );
}

/**
 * The escape's check, read as it stands. Its own frames (`escape-check-*`) draw each state with its acts in ./check; the
 * merged state keeps this card, whose proof row carries what the read says. The person's acts on a check are theirs.
 */
function TheCheck({ row, check, busy, act, propose }: { row: EscapeRow; check: EscapeCheckRow | null; busy: boolean; act(kind: CheckAct, done?: () => void): void; propose(proposal: Proposal): Promise<string | null> }) {
  if (check) {
    if (check.state !== "merged") return <CheckCard row={row} check={check} busy={busy} onAct={act} onPropose={propose} />;
    return (
      <div className="cx-fbx">
        <h6>The check</h6>
        <div className="cx-fkv">
          <span>State</span>
          <b>
            <span className="cx-apchip cx-ok">{checkStateLabel(check.state)}</span>
          </b>
          <span>Where it stands</span>
          <b>{check.line}</b>
          <span>New test file</span>
          <b>{check.path}</b>
          <span>It asserts</span>
          <b>{check.asserts}</b>
          <span>How it runs</span>
          <b>
            <span style={mono}>{check.run}</span>
          </b>
          <span>Must pass on</span>
          <b>
            <Commit id={check.fix_commit} />
          </b>
          <span>Proof</span>
          <b>
            {check.proof
              ? `Fails on ${short(check.proof.escaped_commit)}: ${check.proof.fails_on_escape ? "yes" : "no"} · Passes on ${short(check.proof.fix_commit)}: ${check.proof.passes_on_fix ? "yes" : "no"}`
              : "Not run yet"}
          </b>
          <span>Attempts</span>
          <b>{check.attempts}</b>
        </div>
      </div>
    );
  }
  return (
    <div className="cx-fbx">
      <h6>The check</h6>
      <div className="cx-fkv">
        <span>State</span>
        <b>None yet</b>
        <span>Allowed when</span>
        <b>The escape is confirmed and its gap is a test gap, or not set yet</b>
        <span>What it adds</span>
        <b>One new test file. Never a policy file or .orbyn/crew.json</b>
      </div>
      <div className="cx-t13row" style={{ marginTop: "8px" }}>
        <ProposeAction row={row} busy={busy} onPropose={propose} />
      </div>
    </div>
  );
}

/** A tooling gap gets no proposal in v1: the box says why a test can’t help, and that the rule is yours to add. */
function ToolingCheck({ row, check, busy, act, propose }: { row: EscapeRow; check: EscapeCheckRow | null; busy: boolean; act(kind: CheckAct): void; propose(proposal: Proposal): Promise<string | null> }) {
  if (row.gap !== "tooling_gap") return <TheCheck row={row} check={check} busy={busy} act={act} propose={propose} />;
  return (
    <>
      <div className="cx-fbx">
        <h6>Why a test can’t help</h6>
        <p>
          In v1 a created check only adds test files, so no check is proposed for a tooling gap. It is listed here so you can
          see where a test can’t help.
        </p>
      </div>
      <div className="cx-fbx">
        <h6>
          The check <Planned why={WHY.tooling} />
        </h6>
        <div className="cx-fkv">
          <span>State</span>
          <b>None</b>
          <span>Not built</span>
          <b>A created check never edits .orbyn/crew.json or any policy file. Add the rule yourself.</b>
        </div>
      </div>
    </>
  );
}

/** The engine's own counts in the window, from the same log read the log page draws; the window is the stored one. */
function CountedAgainst({ row }: { row: EscapeRow }) {
  const log = useEscapeLog(row.project);
  const days = log.status === "ready" ? log.data.window_days : null;
  const engine = log.status === "ready" ? log.data.engines.find((one) => one.engine === row.engine) : undefined;
  return (
    <div className="cx-fbx">
      <h6>Counted against</h6>
      <div className="cx-fkv">
        <span>Engine</span>
        <b>{row.engine}</b>
        <span>Model</span>
        <b>
          {row.model ?? <>Not recorded <Planned why={WHY.model} /></>}
        </b>
        <span>Area</span>
        <b>{row.areas.length ? row.areas.join(" · ") : "Not recorded"}</b>
        <span>This engine{days ? `, last ${days} days` : ""}</span>
        {engine ? (
          <b>
            {engine.confirmed} confirmed · {engine.possible} possible · {engine.dismissed} dismissed, of {engine.merges} merges
            {engine.low_sample ? " · low sample" : ""}
          </b>
        ) : log.status === "ready" ? (
          <b>No merges of this engine in the window</b>
        ) : (
          <b>Not measured</b>
        )}
        <span>In {row.areas[0] ?? "this area"}, by this engine</span>
        <b>
          Not measured <Planned why={WHY.counts} />
        </b>
      </div>
    </div>
  );
}

/** The gap dialog of `escape-gap`: a choice between a test gap and a tooling gap, sent only by the person's own click. */
function GapDialog({ row, busy, onClose, onSet }: { row: EscapeRow; busy: boolean; onClose(): void; onSet(gap: EscapeGap): void }) {
  const [pick, setPick] = useState<EscapeGap>(row.gap ?? "test_gap");
  return (
    <Dialog
      title="Set the gap"
      busy={busy}
      onClose={onClose}
      footer={
        <>
          <Button small disabled={busy} onClick={onClose}>Cancel</Button>
          <Button small tone="primary" disabled={busy} onClick={() => onSet(pick)}>Set the gap</Button>
        </>
      }
    >
      <p>What kind of gap was it? You can set it on a possible or a confirmed escape; Crew only suggests it. Not on a dismissed one.</p>
      <div className="cx-fld2">
        <span>Kind of gap</span>
        <button type="button" data-dialog-focus className={`cx-t14opt${pick === "test_gap" ? " cx-on" : ""}`} aria-pressed={pick === "test_gap"} onClick={() => setPick("test_gap")}>
          {pick === "test_gap" ? <Ic n="ok" sm /> : <span />}
          <div>
            <b>Test gap</b>
            <small>A test can catch this: it is a plain input and output. Suggested by Crew. Allows a proposed check</small>
          </div>
        </button>
        <button type="button" className={`cx-t14opt${pick === "tooling_gap" ? " cx-on" : ""}`} aria-pressed={pick === "tooling_gap"} onClick={() => setPick("tooling_gap")}>
          {pick === "tooling_gap" ? <Ic n="ok" sm /> : <span />}
          <div>
            <b>Tooling gap</b>
            <small>
              No cheap test could catch it. It is logged with no proposal in v1 <Planned why={WHY.tooling} />
            </small>
          </div>
        </button>
      </div>
      <p>The change is recorded as “reclassified” in the trail with who set it. If the escape changed since you opened it (another window), Crew refuses and asks you to look again.</p>
    </Dialog>
  );
}

/**
 * Crew Designs v79, `escape-detail` with its tooling-gap, confirmed, dismissed and gap-dialog states, on the real detail
 * read, and the check frames (`escape-check-*`), which open the same page and draw the escape's check in whichever state
 * it is read. The escape's own reason or line names it: the merge's task title is not in any read, so none is invented.
 * The person's acts here are the two decisions on a possible escape, the gap, and the check's accept, approve, retry and
 * withdraw (./check); a proposal is the create-check dialog (./propose), sent only from its own button.
 */
export function LiveEscapeDetail({ id }: { id: string }) {
  if (!id) return <Frame sheet live><Empty title="No escape is named" /></Frame>;
  return <NamedEscape id={id} />;
}

function NamedEscape({ id }: { id: string }) {
  const href = useCrewHref();
  const read = useEscapeDetail(id);
  const [gapOpen, setGapOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const row = read.status === "ready" ? read.data.escape : null;
  const decide = (kind: "confirm" | "dismiss") => {
    if (!row) return;
    setBusy(true);
    setRefused(null);
    const body = { project: row.project, seen: { state: row.state, gap: row.gap } };
    void (kind === "confirm" ? escapeApi.confirm(id, body) : escapeApi.dismiss(id, body))
      .then(() => {
        setBusy(false);
        void read.reload();
      })
      .catch((error: unknown) => {
        setBusy(false);
        setRefused(refusalOf(error));
        void read.reload();
      });
  };
  const sending = useRef(false);
  const setGap = (gap: EscapeGap) => {
    if (!row || sending.current) return;
    sending.current = true;
    setBusy(true);
    setRefused(null);
    void escapeApi
      .gap(id, { project: row.project, seen: { state: row.state, gap: row.gap }, gap })
      .then(() => {
        sending.current = false;
        setBusy(false);
        setGapOpen(false);
        void read.reload();
      })
      .catch((error: unknown) => {
        sending.current = false;
        setBusy(false);
        setRefused(refusalOf(error));
        setGapOpen(false);
        void read.reload();
      });
  };

  /** A person's act on the check: accept, approve, retry or withdraw, each one write with what they saw. */
  const act = (kind: CheckAct, done?: () => void) => {
    const one = read.status === "ready" ? read.data : null;
    if (!row || !one || sending.current) return;
    const seen = { state: row.state, gap: row.gap, check: one.check?.state ?? null };
    const send =
      kind === "accept"
        ? escapeApi.accept(id, { project: row.project, seen })
        : kind === "retry"
          ? escapeApi.retry(id, { project: row.project, seen })
          : kind === "withdraw"
            ? escapeApi.withdraw(id, { project: row.project, seen })
            : one.check?.diff_commit && one.check?.diff_hash
              ? escapeApi.approve(id, { project: row.project, seen, commit: one.check.diff_commit, diff_hash: one.check.diff_hash })
              : null;
    if (!send) return;
    sending.current = true;
    setBusy(true);
    setRefused(null);
    void send
      .then(() => {
        sending.current = false;
        setBusy(false);
        done?.();
        void read.reload();
      })
      .catch((error: unknown) => {
        sending.current = false;
        setBusy(false);
        setRefused(refusalOf(error));
        done?.();
        void read.reload();
      });
  };
  /**
   * A person's proposal of a check, one write with what they saw. It answers null when the service took it, or the service's own
   * words when it refused, so the dialog can stay open with what was typed; a refusal reads the page again, as every act does.
   */
  const propose = (proposal: Proposal): Promise<string | null> => {
    const one = read.status === "ready" ? read.data : null;
    if (!row || !one || sending.current) return Promise.resolve("Wait for the page to finish its last act, then try again.");
    sending.current = true;
    setBusy(true);
    setRefused(null);
    const seen = { state: row.state, gap: row.gap, check: one.check?.state ?? null };
    return escapeApi
      .check(id, { project: row.project, seen, ...proposal })
      .then(() => {
        sending.current = false;
        setBusy(false);
        void read.reload();
        return null;
      })
      .catch((error: unknown) => {
        sending.current = false;
        setBusy(false);
        void read.reload();
        return refusalOf(error);
      });
  };
  if (read.status === "loading") return <Frame sheet live><Loading label="Reading the escape" rows={3} /></Frame>;
  if (read.status === "error")
    return <Frame sheet live><ErrorState title="Can’t read the escape" failure={read.error} onRetry={() => void read.reload()} /></Frame>;
  const one = read.data;
  const escape = one.escape;
  return (
    <Frame live>
      <div className="cx-cwpg">
        <div className="cx-flcr">
          <span>Dashboard</span>
          <Ic n="right" sm />
          <button type="button" className="cx-linkname" onClick={() => {
            window.location.hash = href({ page: "escapes" });
          }}>
            Escape log
          </button>
          <Ic n="right" sm />
          <b>{nameOf(escape)}</b>
        </div>
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>{nameOf(escape)}</b>
            <StateChip row={escape} />
            <span className="cx-apchip">{sourceLabel(escape.source)}</span>
            {escape.state !== "dismissed" ? <span className="cx-apchip">{escape.gap ? gapText(escape.gap, escape.gap_by) : "Gap not set"}</span> : null}
            {one.check ? <CheckChip check={one.check} /> : null}
            <span className="cx-t10sp">
              {escape.state === "possible" ? (
                <>
                  <button type="button" className="cx-btn" disabled={busy} onClick={() => decide("confirm")}>Confirm</button>
                  <button type="button" className="cx-ghost" disabled={busy} onClick={() => decide("dismiss")}>Dismiss</button>
                  <button type="button" className="cx-ghost" disabled={busy} onClick={() => setGapOpen(true)}>Set the gap…</button>
                </>
              ) : escape.state === "confirmed" ? (
                <button type="button" className="cx-ghost" disabled={busy} onClick={() => setGapOpen(true)}>Set the gap…</button>
              ) : null}
            </span>
          </div>
          {refused ? (
            <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
              <Ic n="alert" sm />
              <span>{refused}</span>
            </div>
          ) : null}
          <div className="cx-t14two">
            <Timeline detail={one} />
            <div className="cx-t14col">
              {escape.state === "dismissed" ? (
                <>
                  <div className="cx-fbx">
                    <h6>Why Crew thought so</h6>
                    <p>
                      {escape.reason ?? escape.line}
                      {escape.source === "post_merge_check" ? (
                        <>
                          {" "}A forge’s CI status is not read <Planned why={WHY.forge} />.
                        </>
                      ) : null}
                    </p>
                  </div>
                  <div className="cx-fbx">
                    <h6>{stateLabel.dismissed}</h6>
                    <div className="cx-fkv">
                      <span>Counted</span>
                      <b>No: a dismissed escape is not an escape</b>
                      <span>Offered again</span>
                      <b>No</b>
                      <span>Gap</span>
                      <b>Can’t be set on a dismissed escape</b>
                      <span>Undo</span>
                      <b>
                        None in v1 <Planned why={WHY.undo} />
                      </b>
                    </div>
                  </div>
                </>
              ) : (
                <ToolingCheck row={escape} check={one.check ?? null} busy={busy} act={act} propose={propose} />
              )}
              <CountedAgainst row={escape} />
              {escape.state !== "dismissed" ? (
                <div className="cx-fbx">
                  <h6>Where the count is used</h6>
                  <p>Nowhere in v1: confirmed escapes feed neither the circuit breaker nor the Router. Autopilot by area counts merges and reverts, not escapes.</p>
                  <div className="cx-t13row" style={{ marginTop: "8px" }}>
                    <button type="button" className="cx-ghost" onClick={() => {
                      window.location.hash = `#/settings/crew/areas?repo=${encodeURIComponent(escape.project)}`;
                    }}>
                      Open Autopilot by area
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
          </div>
        </div>
      </div>
      {gapOpen && escape.state !== "dismissed" ? <GapDialog row={escape} busy={busy} onClose={() => setGapOpen(false)} onSet={setGap} /> : null}
    </Frame>
  );
}
