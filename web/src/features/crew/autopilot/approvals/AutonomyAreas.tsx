// Settings › Crew › Autopilot by area (Crew Designs v39, frames `autonomy-*`): each area of the code with the ceiling, the mode it is on now and
// its record, why it moved, the way back, and a person's override. Every figure comes from the areas read; the thresholds are shown as Proposed.
import { Info, Folder } from "lucide-react";
import { useEffect, useState } from "react";
import { ApiFailure } from "../../../../api/client";
import { toFailure, useAsync } from "../../../../lib/useAsync";
import { navigate, useRoute } from "../../../../product/state/route";
import { useWorkspace } from "../../../../product/state/workspace";
import { ErrorState, Loading } from "../../../../product/ui/states";
import type { AutopilotAreaEvent } from "../../../../generated/AutopilotAreaEvent";
import type { AutopilotAreaRow } from "../../../../generated/AutopilotAreaRow";
import type { AutopilotAreas } from "../../../../generated/AutopilotAreas";
import type { AutopilotThresholds } from "../../../../generated/AutopilotThresholds";
import { Button } from "../../../../ui/controls";
import { ConfirmDialog } from "../../../../ui/Dialog";
import { Menu } from "../../../../ui/Menu";
import { Presence } from "../../../../ui/Presence";
import { crewApi } from "./api";
import { asOf, clock, frameOf, isLowered, modeName, plural, quiet, recentRestore, rowView, type RowView } from "./areas";
import { GUARD_WORKERS } from "./copy";
import { Chip, Facts } from "./parts";

const HISTORY_ROWS = 12;

/** The repository the page is about: `?repo=` in the address, else the first project that is not hidden. */
function useRepo(): { id: string; name: string } | "loading" | null {
  const route = useRoute();
  const projects = useWorkspace((now) => now.projects.data);
  if (!projects) return "loading";
  const wanted = route.name === "settings" ? route.repo : undefined;
  const one = projects.find((project) => (wanted ? project.id === wanted : !project.scratch && !project.library));
  return one ? { id: one.id, name: one.name } : null;
}

function ModeChip({ mode, ceiling, now }: { mode: AutopilotAreaRow["now"]; ceiling: AutopilotAreaRow["ceiling"]; now?: boolean }) {
  return <Chip tone={quiet(mode, ceiling, now === true) ? "dim" : "plain"}>{modeName(mode)}</Chip>;
}

function AreaRow({ row, view }: { row: AutopilotAreaRow; view: RowView }) {
  return (
    <div className="ar-row" role="row" data-state={row.state}>
      <span role="cell" className="ar-area"><b>{row.area}</b></span>
      <span role="cell" className="ar-now">
        <ModeChip mode={row.now} ceiling={row.ceiling} now />
        <small className="ar-why">{view.why}</small>
      </span>
      <span role="cell" className="ar-rev">{view.measured ? null : <span className="ar-n">Reverted: </span>}<b>{view.reverted}</b>{view.measured ? <span className="ar-n"> reverted</span> : null}</span>
      <span role="cell" className="ar-esc"><span className="ar-n">Escaped: </span><b>Not measured</b></span>
      <span role="cell" className="ar-win">{view.days}<small>{view.merges}</small></span>
    </div>
  );
}

function Panel({ title, children, rows }: { title: string; children?: React.ReactNode; rows?: { label: string; text: string }[] }) {
  return (
    <section className="ap-box" aria-label={title}>
      <h4>{title}</h4>
      {rows ? <Facts rows={rows} /> : null}
      {children}
    </section>
  );
}

function OverrideButton({ onOpen }: { onOpen(anchor: HTMLElement): void }) {
  return <Button small onClick={(event) => onOpen(event.currentTarget)}>Override…</Button>;
}

type Panels = { row: AutopilotAreaRow; restored: AutopilotAreaEvent | null; t: AutopilotThresholds; open(row: AutopilotAreaRow, anchor: HTMLElement): void };

function AreaPanel({ row, restored, t, open }: Panels) {
  const record = row.record;
  const recovery = row.recovery;
  const ceilingWins = isLowered(row) && row.ceiling === "off";
  if (ceilingWins) {
    return (
      <Panel
        title={`${row.area}: the ceiling wins`}
        rows={[
          { label: "What the row shows", text: `Repo ceiling ${modeName(row.ceiling)}, Now ${modeName(row.now)}. An area never goes above its ceiling, so the demotion to On changes nothing.` },
          { label: "The record", text: `${record.reverts} of ${record.merges} merges reverted in ${record.window_days} days: still shown, and still counted` },
          { label: "While it is Off", text: `Everything in ${row.area} waits for you` },
          { label: "If you raise it", text: `To Full: ${row.area} follows the record again, so it is on On until it recovers. To On: it stays On.` },
        ]}
      >
        <div className="ar-acts"><OverrideButton onOpen={(anchor) => open(row, anchor)} /></div>
      </Panel>
    );
  }
  if (isLowered(row) && row.ceiling !== "full") return null;
  if (row.state === "demoted") {
    return (
      <Panel
        title={`Why ${row.area} moved from ${modeName(row.ceiling)} to ${modeName(row.now)}`}
        rows={[
          { label: "The record", text: `${plural(record.reverts, "revert")} in ${record.window_days} days, in ${plural(record.merges, "merge")} (a trip needs ${plural(t.min_merges, "merge")} and ${t.trip_reverts} reverted)` },
          { label: "What changed", text: `A High risk diff in ${row.area} waits for you at the merge gate. Low and Medium still merge by themselves. The risk tier itself is unchanged.` },
          { label: "It returns when", text: `${t.recovery_days} days have passed since the last revert with none, and ${t.recovery_merges} merges since then with none reverted${t.proposed ? " (Proposed)" : ""}` },
          { label: "Your ceiling", text: `${modeName(row.ceiling)}: the circuit breaker only demotes an area whose ceiling is Full, to On, and it never goes above the ceiling` },
        ]}
      >
        <div className="ar-acts"><OverrideButton onOpen={(anchor) => open(row, anchor)} /></div>
      </Panel>
    );
  }
  if (row.state === "recovering" && recovery) {
    return (
      <Panel title={`${row.area} is recovering`}>
        <ul className="ar-who">
          <li>{recovery.clean_days} of the {recovery.days_needed} days with no revert.</li>
          <li>{recovery.merges} of the {recovery.merges_needed} merges needed since then, none reverted. Nothing returns before both counts are met.</li>
        </ul>
        <p className="ap-muted">A new revert in this area starts the count again. A High risk diff in {row.area} still waits for you at the merge gate.</p>
        <div className="ar-acts"><OverrideButton onOpen={(anchor) => open(row, anchor)} /></div>
      </Panel>
    );
  }
  if (row.state === "criteria_met" && recovery) {
    return (
      <Panel title={`${row.area} meets the criteria, and returns at the next merge`}>
        <ul className="ar-who">
          <li>No revert for {Math.min(recovery.clean_days, recovery.days_needed)} of the {recovery.days_needed} days.</li>
          <li>{plural(recovery.merges, "merge")} since then, none reverted ({recovery.merges_needed} needed).</li>
        </ul>
        <p className="ap-muted">Both counts are met, but the restore is checked at a merge, not on a clock. {row.area} stays on {modeName(row.now)} until the next merge in it, and a High risk diff there still waits for you at the merge gate until then.</p>
        <div className="ar-acts"><OverrideButton onOpen={(anchor) => open(row, anchor)} /></div>
      </Panel>
    );
  }
  if (restored && row.state === "follows") {
    return (
      <Panel title={`${row.area} is back on ${modeName(row.now)}`}>
        <p>
          {row.area} returned to your ceiling at the next merge, at {clock(restored.at)}. The reverts that tripped it stay in the ledger and in the dashboard history. If the record worsens again, it drops again.
        </p>
        <div className="ar-acts"><OverrideButton onOpen={(anchor) => open(row, anchor)} /></div>
      </Panel>
    );
  }
  return null;
}

/** A person's choice for one area, in the words of the design. */
function optionLine(title: string, text: string) {
  return <span className="ar-opt"><b>{title}</b><small>{text}</small></span>;
}

type Risky = "keep_ceiling" | "off";

/** The warning before a choice that changes what waits for the person: the same dialog for Keep the ceiling and Off. */
function OverrideDialog({ row, choice, onClose, onConfirm }: { row: AutopilotAreaRow; choice: Risky; onClose(): void; onConfirm(): Promise<void> }) {
  const [ack, setAck] = useState(false);
  const record = row.record;
  const off = choice === "off";
  return (
    <ConfirmDialog
      title={off ? `Turn ${row.area} off?` : `Keep the ceiling for ${row.area}?`}
      confirmLabel={off ? "Turn it off" : "Keep the ceiling"}
      confirmDisabled={!ack}
      className="ap-dialog"
      onClose={onClose}
      onConfirm={onConfirm}
    >
      <div className="ar-dialog">
        <p>
          {off
            ? `If you turn ${row.area} off, everything in ${row.area} waits for you at the merge gate, whatever its risk.`
            : `${row.area} is on ${modeName(row.now)} because ${record.reverts} of its last ${record.merges} merges were reverted in ${record.window_days} days. If you keep the ceiling (${modeName(row.ceiling)}), a High risk diff in ${row.area} merges by itself again, without waiting for you at the merge gate.`}
        </p>
        <Facts
          rows={[
            { label: "The record", text: `${plural(record.reverts, "revert")} in ${record.window_days} days · ${plural(record.merges, "merge")}` },
            off
              ? { label: "What it does", text: `Nothing in ${row.area} merges until you have looked at it. Other areas keep their mode.` }
              : { label: "What you risk", text: `A High risk diff in ${row.area} can reach main before you have looked at it. You review it afterwards, at the top of your Review queue.` },
            off
              ? { label: "How long", text: `Until you choose Follow the record. Crew keeps measuring ${row.area} and shows the record next to it.` }
              : { label: "How long", text: `Until a new trip. If ${row.area} is demoted again, Keep the ceiling is cleared and the area follows the record. Crew keeps measuring ${row.area} and shows the record next to it.` },
          ]}
        />
        <label className="ar-ack">
          <input type="checkbox" checked={ack} onChange={(event) => setAck(event.target.checked)} />
          <span>{off ? `I understand that everything in ${row.area} will wait for me.` : `I understand that High risk diffs in ${row.area} will merge without waiting for me.`}</span>
        </label>
        <p className="ap-muted">Only you can make this choice. It is logged with the time. It never raises an area above your ceiling, and the hard-denied list still applies.</p>
      </div>
    </ConfirmDialog>
  );
}

const seenOf = (row: AutopilotAreaRow) => ({ merges: row.record.merges, reverts: row.record.reverts, tripped_at: row.since ?? null });

function AreasView({ project, areas, t, reload }: { project: { id: string; name: string }; areas: AutopilotAreas; t: AutopilotThresholds; reload(): Promise<unknown> }) {
  const [trail, setTrail] = useState<Record<string, AutopilotAreaEvent[]>>({});
  const [menu, setMenu] = useState<{ row: AutopilotAreaRow; x: number; y: number } | null>(null);
  const [keep, setKeep] = useState<{ row: AutopilotAreaRow; choice: Risky } | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  // A return is a line in the trail, not a state of the row: read the trail of the rows that follow their ceiling, the first few only.
  useEffect(() => {
    const control = new AbortController();
    const wanted = areas.rows.filter((row) => row.state === "follows" && row.ceiling === "full").slice(0, HISTORY_ROWS);
    void Promise.all(wanted.map((row) => crewApi.areaHistory(project.id, row.area_key, control.signal).then((reply) => [row.area_key, reply.events] as const, () => null))).then((found) => {
      if (!control.signal.aborted) setTrail(Object.fromEntries(found.filter((one) => one !== null)));
    });
    return () => control.abort();
  }, [project.id, areas]);

  const open = (row: AutopilotAreaRow, anchor: HTMLElement) => {
    const rect = anchor.getBoundingClientRect();
    setMenu({ row, x: rect.left, y: rect.bottom + 4 });
  };
  const refuse = (error: unknown) => {
    setFailed(toFailure(error).message);
    void reload();
  };
  const choose = (row: AutopilotAreaRow, choice: "follow" | "keep_ceiling" | "off") => {
    setFailed(null);
    // Both risky choices ask first: nothing is sent with an acknowledgement the person did not give.
    if (choice !== "follow") {
      if (row.override?.choice !== choice) setKeep({ row, choice });
      return;
    }
    if (!row.override) return;
    void crewApi.followArea({ project: project.id, area: row.area_key, seen: seenOf(row) }).then(() => reload(), refuse);
  };

  // One ceiling per repository: every row is read against it.
  const rows = areas.rows.map((row) => ({ ...row, ceiling: areas.ceiling }));
  const allThin = rows.length > 0 && rows.every((row) => row.state === "not_enough_data");
  const footerTarget = rows.find(isLowered) ?? rows.find((row) => row.state === "overridden");
  const footerBoth = footerTarget?.state === "overridden";
  const items = (row: AutopilotAreaRow) => [
    { label: "Follow the record", selected: !row.override, leadCheck: true, content: optionLine("Follow the record", "The default. It returns by itself when the record recovers"), onSelect: () => choose(row, "follow") },
    ...(isLowered(row) || row.override?.choice === "keep_ceiling"
      ? [{ label: "Keep the ceiling", selected: row.override?.choice === "keep_ceiling", leadCheck: true, content: optionLine("Keep the ceiling", `${modeName(row.ceiling)}, your ceiling. Needs an acknowledgement; a new trip clears it`), onSelect: () => choose(row, "keep_ceiling") }]
      : []),
    { label: "Off", selected: row.override?.choice === "off", leadCheck: true, content: optionLine("Off", `Everything in ${row.area} waits for you`), onSelect: () => choose(row, "off") },
  ];

  return (
    <section className="ap-set ar" aria-labelledby="ar-title" data-frame={keep ? (keep.choice === "off" ? "autonomy-override-off-warning" : "autonomy-override-warning") : menu ? "autonomy-override" : frameOf(rows, rows.some((row) => recentRestore(trail[row.area_key], areas.as_of, t.window_days) !== null && row.state === "follows"))}>
      <header className="ap-set-head">
        <h2 id="ar-title">Crew › Autopilot by area</h2>
        <Chip tone="plain">Preview</Chip>
      </header>
      <p className="ap-lead">
        Your Autopilot mode is the ceiling: no area goes higher. An area is the first two folders of a path, such as web/app. For each area Crew counts merges and reverts over a rolling window. If an area whose ceiling is Full has at least {t.min_merges} merges in the window and at least {t.trip_reverts} of them reverted, it drops to On and says why; it returns when its record recovers. Below {t.min_merges} merges the row says “Not enough data” and nothing changes. Escapes are not measured. {t.proposed ? "These thresholds are Proposed." : ""}
      </p>
      <div className="ar-chips">
        <span className="ar-mk"><Folder size={12} aria-hidden />Repo: {project.name}</span>
        <span className="ar-mk">Repo ceiling: {modeName(areas.ceiling)} · change it in Settings › Crew</span>
        <span className="ar-mk">Window: {plural(t.window_days, "day")}</span>
      </div>
      <small className="ar-sub">As of {asOf(areas.as_of)}</small>
      <div className="ar-table" role="table" aria-label="Autopilot by area">
        <div className="ar-row ap-head" role="row">
          {["Area", "Now", "Reverted", "Escaped", "Window"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {rows.map((row) => <AreaRow key={row.area_key} row={row} view={rowView(row, recentRestore(trail[row.area_key], areas.as_of, t.window_days))} />)}
      </div>
      {allThin ? (
        <Panel title={`Not enough data: each area has fewer than ${t.min_merges} merges`}>
          <p>A trip needs at least {t.min_merges} merges in the window. Below that Crew shows the merges themselves and changes nothing: no area is demoted on a handful of merges, and an area that was never measured says “Not measured”.</p>
        </Panel>
      ) : null}
      {rows.map((row) => <AreaPanel key={row.area_key} row={row} restored={recentRestore(trail[row.area_key], areas.as_of, t.window_days)} t={t} open={open} />)}
      {failed ? <p role="alert" className="ap-error">{failed}</p> : null}
      <div className="ar-acts">
        <Button small onClick={() => navigate({ name: "settings", section: "crew", crewPage: "thresholds", repo: project.id })}>Open the thresholds</Button>
        <span className="ar-wide"><Button small onClick={() => navigate({ name: "crew", tab: "approvals" })}>Open the ledger</Button></span>
        {footerTarget ? <span className={footerBoth ? "" : "ar-narrow"}><OverrideButton onOpen={(anchor) => open(footerTarget, anchor)} /></span> : null}
      </div>
      <p className="ar-note"><Info size={14} aria-hidden /><span>{GUARD_WORKERS}</span></p>
      <Presence>
        {menu ? (
          <Menu
            x={menu.x}
            y={menu.y}
            minWidth={408}
            label={`${menu.row.area} override`}
            heading={`${menu.row.area} is on ${modeName(menu.row.now)}. What should it follow?`}
            footer="Only you can change this. Never above the ceiling."
            items={items(menu.row)}
            onClose={() => setMenu(null)}
          />
        ) : null}
      </Presence>
      {keep ? (
        <OverrideDialog
          row={keep.row}
          choice={keep.choice}
          onClose={() => setKeep(null)}
          onConfirm={() =>
            crewApi.overrideArea({ project: project.id, area: keep.row.area_key, choice: keep.choice, acknowledged: true, seen: seenOf(keep.row) }).then(
              () => void reload(),
              (error: unknown) => {
                // A record that moved while the person read the warning answers `stale`: show the service's words, and the new record behind the dialog.
                if (error instanceof ApiFailure) void reload();
                throw error;
              },
            )
          }
        />
      ) : null}
    </section>
  );
}

/** Settings › Crew › Autopilot by area. */
export function AutonomyAreas() {
  const repo = useRepo();
  const project = repo === "loading" || repo === null ? null : repo;
  const areas = useAsync(`crew-areas:${project?.id ?? ""}`, (signal) => (project ? crewApi.areas(project.id, signal) : Promise.reject(new ApiFailure("not_found", "No repository", 404))));
  const limits = useAsync("crew-thresholds", (signal) => crewApi.thresholds(signal), { keep: true });
  if (repo === "loading" || areas.status === "loading" || limits.status === "loading") return <Loading label="Reading Autopilot by area" rows={4} />;
  if (!project) return null;
  if (areas.status === "error") return <ErrorState title="Can't read Autopilot by area" failure={areas.error} onRetry={areas.reload} />;
  if (limits.status === "error") return <ErrorState title="Can't read Autopilot by area" failure={limits.error} onRetry={limits.reload} />;
  return <AreasView project={project} areas={areas.data} t={limits.data} reload={areas.reloadNow} />;
}
