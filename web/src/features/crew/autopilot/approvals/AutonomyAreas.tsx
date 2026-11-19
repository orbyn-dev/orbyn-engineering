// Settings › Crew › Autopilot by area (Crew Designs v39, frames `autonomy-*`): each area of the code with the ceiling, the mode it is on now and
// its record, why it moved, the way back, and a person's override. Every figure comes from the areas read; the thresholds are shown as Proposed.
import { Info, Folder, ChevronDown } from "lucide-react";
import { useEffect, useState } from "react";
import { ApiFailure } from "../../../../api/client";
import { useAsync } from "../../../../lib/useAsync";
import { navigate, useRoute } from "../../../../product/state/route";
import { useVisibleProjects, useWorkspace } from "../../../../product/state/workspace";
import { ErrorState, Loading } from "../../../../product/ui/states";
import type { AutopilotAreaEvent } from "../../../../generated/AutopilotAreaEvent";
import type { AutopilotAreaRow } from "../../../../generated/AutopilotAreaRow";
import type { AutopilotAreas } from "../../../../generated/AutopilotAreas";
import type { AutopilotThresholds } from "../../../../generated/AutopilotThresholds";
import { Button } from "../../../../ui/controls";
import { crewApi } from "./api";
import { asOf, clock, frameOf, isLowered, modeName, plural, quiet, recentRestore, rowView, type RowView } from "./areas";
import { useAreaOverride } from "./areaOverride";
import { GUARD_WORKERS } from "./copy";
import { Chip, Facts } from "./parts";

const HISTORY_ROWS = 12;

/**
 * The repository the page is about: `?repo=` in the address, else the first visible git project.
 * `several` is the workspace's count of visible git projects, for the Repo chip: with more than one the page is
 * about one repo at a time, and the design draws that chip as a selector (still Planned: there is no menu yet).
 */
export function useRepo(): { id: string; name: string; several: boolean } | "loading" | null {
  const route = useRoute();
  const projects = useWorkspace((now) => now.projects.data);
  const visible = useVisibleProjects();
  if (!projects || !visible) return "loading";
  const repos = visible.filter((project) => project.is_git);
  const wanted = route.name === "settings" ? route.repo : undefined;
  const one = wanted ? projects.find((project) => project.id === wanted) : repos[0];
  return one ? { id: one.id, name: one.name, several: repos.length > 1 } : null;
}

function ModeChip({ mode, ceiling, now }: { mode: AutopilotAreaRow["now"]; ceiling: AutopilotAreaRow["ceiling"]; now?: boolean }) {
  return <Chip tone={quiet(mode, ceiling, now === true) ? "dim" : "plain"}>{modeName(mode)}</Chip>;
}

function AreaRow({ row, view, open }: { row: AutopilotAreaRow; view: RowView; open(row: AutopilotAreaRow, anchor: HTMLElement): void }) {
  return (
    <div className="ar-row" role="row" data-state={row.state}>
      <span role="cell" className="ar-area"><b>{row.area}</b></span>
      <span role="cell" className="ar-now">
        <ModeChip mode={row.now} ceiling={row.ceiling} now />
        <small className="ar-why">{view.why}</small>
        {row.state === "overridden" ? <OverrideButton onOpen={(anchor) => open(row, anchor)} /> : null}
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

function AreasView({ project, areas, t, reload }: { project: { id: string; name: string; several: boolean }; areas: AutopilotAreas; t: AutopilotThresholds; reload(): Promise<unknown> }) {
  const [trail, setTrail] = useState<Record<string, AutopilotAreaEvent[]>>({});
  // The same menu the Needs you inbox opens (needsyou-demoted): its words and warning live in areaOverride.tsx.
  const override = useAreaOverride({ project: project.id, reload: () => void reload() });

  // A return is a line in the trail, not a state of the row: read the trail of the rows that follow their ceiling, the first few only.
  useEffect(() => {
    const control = new AbortController();
    const wanted = areas.rows.filter((row) => row.state === "follows" && row.ceiling === "full").slice(0, HISTORY_ROWS);
    void Promise.all(wanted.map((row) => crewApi.areaHistory(project.id, row.area_key, control.signal).then((reply) => [row.area_key, reply.events] as const, () => null))).then((found) => {
      if (!control.signal.aborted) setTrail(Object.fromEntries(found.filter((one) => one !== null)));
    });
    return () => control.abort();
  }, [project.id, areas]);

  const rows = areas.rows;
  const allThin = rows.length > 0 && rows.every((row) => row.state === "not_enough_data");
  const footerTarget = rows.find(isLowered);
  return (
    <section className="ap-set ar" aria-labelledby="ar-title" data-frame={override.frame ?? frameOf(rows, rows.some((row) => recentRestore(trail[row.area_key], areas.as_of, t.window_days) !== null && row.state === "follows"))}>
      <header className="ap-set-head">
        <h2 id="ar-title">Crew › Autopilot by area</h2>
        <Chip tone="plain">Preview</Chip>
      </header>
      <p className="ap-lead">
        Your Autopilot mode is the ceiling: no area goes higher. An area is the first two folders of a path, such as web/app. For each area Crew counts merges and reverts over a rolling window. If an area whose ceiling is Full has at least {t.min_merges} merges in the window and at least {t.trip_reverts} of them reverted, it drops to On and says why; it returns when its record recovers. Below {t.min_merges} merges the row says “Not enough data” and nothing changes. Escapes are not measured. {t.proposed ? "These thresholds are Proposed." : ""}
      </p>
      <div className="ar-chips">
        {project.several ? (
          <span className="ar-pick">
            <span className="ar-mk ar-mk-pick">
              <Folder size={12} aria-hidden />Repo: {project.name}
              <ChevronDown size={12} aria-hidden />
            </span>
            <span className="ar-pv" title="Planned: the app has no repo selector; the page takes ?repo= or the first project">Planned</span>
          </span>
        ) : (
          <span className="ar-mk"><Folder size={12} aria-hidden />Repo: {project.name}</span>
        )}
        <span className="ar-mk">Repo ceiling: {modeName(areas.ceiling)} · change it in Settings › Crew</span>
        <span className="ar-mk">Window: {plural(t.window_days, "day")}</span>
      </div>
      <small className="ar-sub">As of {asOf(areas.as_of)}</small>
      <div className="ar-table" role="table" aria-label="Autopilot by area" data-empty={rows.length === 0 ? "" : undefined}>
        <div className="ar-row ap-head" role="row">
          {["Area", "Now", "Reverted", "Escaped", "Window"].map((name) => <span key={name} role="columnheader">{name}</span>)}
        </div>
        {rows.map((row) => <AreaRow key={row.area_key} row={row} open={override.open} view={rowView(row, recentRestore(trail[row.area_key], areas.as_of, t.window_days), t, areas.ceiling)} />)}
      </div>
      {allThin ? (
        <Panel title={`Not enough data: each area has fewer than ${t.min_merges} merges`}>
          <p>A trip needs at least {t.min_merges} merges in the window. Below that Crew shows the merges themselves and changes nothing: no area is demoted on a handful of merges, and an area that was never measured says “Not measured”.</p>
        </Panel>
      ) : null}
      {rows.map((row) => <AreaPanel key={row.area_key} row={row} restored={recentRestore(trail[row.area_key], areas.as_of, t.window_days)} t={t} open={override.open} />)}
      <div className="ar-acts">
        <Button small onClick={() => navigate({ name: "settings", section: "crew", crewPage: "thresholds", repo: project.id })}>Open the thresholds</Button>
        <span className="ar-wide"><Button small onClick={() => navigate({ name: "crew", tab: "approvals" })}>Open the ledger</Button></span>
        {footerTarget ? <span className="ar-narrow"><OverrideButton onOpen={(anchor) => override.open(footerTarget, anchor)} /></span> : null}
      </div>
      <p className="ar-note"><Info size={14} aria-hidden /><span>{GUARD_WORKERS}</span></p>
      {override.node}
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
