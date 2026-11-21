// The override menu of Autopilot by area (Crew Designs: autonomy-override, -override-warning, -override-off-warning). The
// Settings page and the Needs you inbox open the same menu, so its words, its warning and its write flow live here once.
// Nothing here raises an area above its ceiling; every write goes through the person-only routes that already exist.
import { useState, type ReactNode } from "react";
import { ApiFailure } from "../../../../api/client";
import { toFailure } from "../../../../lib/useAsync";
import type { AutopilotAreaRow } from "../../../../generated/AutopilotAreaRow";
import { ConfirmDialog } from "../../../../ui/Dialog";
import { Menu, type MenuItem } from "../../../../ui/Menu";
import { Presence } from "../../../../ui/Presence";
import { crewApi } from "./api";
import { isLowered, modeName, plural } from "./areas";
import { Facts } from "./parts";
import "./autopilot.css";

export type OverrideChoice = "follow" | "keep_ceiling" | "off";
type Risky = "keep_ceiling" | "off";

/** What the person last saw of the record, so a choice made on an old one is refused with `stale`. */
export const seenOf = (row: AutopilotAreaRow) => ({ merges: row.record.merges, reverts: row.record.reverts, tripped_at: row.since ?? null });

/** A person's choice for one area, in the words of the design. */
function optionLine(title: string, text: string) {
  return <span className="ar-opt"><b>{title}</b><small>{text}</small></span>;
}

/** The menu's items for one area: Follow the record; Keep the ceiling, offered only while an area is lowered under a Full ceiling; Off. */
export function overrideItems(row: AutopilotAreaRow, choose: (row: AutopilotAreaRow, choice: OverrideChoice) => void): MenuItem[] {
  const held = row.override?.choice;
  const follow = { label: "Follow the record", selected: !held, leadCheck: true, content: optionLine("Follow the record", "The default. It returns by itself when the record recovers"), onSelect: () => choose(row, "follow") };
  const keepItem = { label: "Keep the ceiling", selected: held === "keep_ceiling", leadCheck: true, content: optionLine("Keep the ceiling", `${modeName(row.ceiling)}, your ceiling. Needs an acknowledgement; a new trip clears it`), onSelect: () => choose(row, "keep_ceiling") };
  const offItem = { label: "Off", selected: held === "off", leadCheck: true, content: optionLine("Off", `Everything in ${row.area} waits for you`), onSelect: () => choose(row, "off") };
  if (held === "keep_ceiling") return [follow, keepItem];
  if (held === "off") return [follow, offItem];
  return [follow, ...(isLowered(row) && row.ceiling === "full" && row.now !== row.ceiling ? [keepItem] : []), offItem];
}

/** The warning before a choice that changes what waits for the person: the same dialog for Keep the ceiling and Off. */
export function OverrideDialog({ row, choice, onClose, onConfirm }: { row: AutopilotAreaRow; choice: Risky; onClose(): void; onConfirm(): Promise<void> }) {
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

export interface AreaOverride {
  /** Opens the menu for one area, at the anchor the person clicked. */
  open(row: AutopilotAreaRow, anchor: HTMLElement): void;
  /** The menu, its warning dialog and the failure line, rendered where the surface draws them. */
  node: ReactNode;
  /** The frame the open menu or warning names, for the surface's `data-frame`. */
  frame: "autonomy-override" | "autonomy-override-warning" | "autonomy-override-off-warning" | null;
}

/** The override menu, its warning and its failure line for one repository's areas. */
export function useAreaOverride({ project, reload }: { project: string | null; reload(): void }): AreaOverride {
  const [menu, setMenu] = useState<{ row: AutopilotAreaRow; x: number; y: number } | null>(null);
  const [keep, setKeep] = useState<{ row: AutopilotAreaRow; choice: Risky } | null>(null);
  const [failed, setFailed] = useState<string | null>(null);

  const refuse = (error: unknown) => {
    setFailed(toFailure(error).message);
    reload();
  };
  const choose = (row: AutopilotAreaRow, choice: OverrideChoice) => {
    if (!project) return;
    setFailed(null);
    // Both risky choices ask first: nothing is sent with an acknowledgement the person did not give.
    if (choice !== "follow") {
      if (row.override?.choice !== choice) setKeep({ row, choice });
      return;
    }
    if (!row.override) return;
    void crewApi.followArea({ project, area: row.area_key, ...(row.since ? { seen: seenOf(row) } : {}) }).then(() => reload(), refuse);
  };
  const open = (row: AutopilotAreaRow, anchor: HTMLElement) => {
    const rect = anchor.getBoundingClientRect();
    setMenu({ row, x: rect.left, y: rect.bottom + 4 });
  };
  const node = (
    <>
      {failed ? <p role="alert" className="ap-error">{failed}</p> : null}
      <Presence>
        {menu ? (
          <Menu
            x={menu.x}
            y={menu.y}
            minWidth={408}
            label={`${menu.row.area} override`}
            heading={`${menu.row.area} is on ${modeName(menu.row.now)}. What should it follow?`}
            footer="Only you can change this. Never above the ceiling."
            items={overrideItems(menu.row, choose)}
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
            project
              ? crewApi.overrideArea({ project, area: keep.row.area_key, choice: keep.choice, acknowledged: true, seen: seenOf(keep.row) }).then(
                  () => reload(),
                  (error: unknown) => {
                    // A record that moved while the person read the warning answers `stale`. The warning described the old record, so it closes:
                    // the service's words show on the surface, with the new record, and the person chooses again.
                    if (error instanceof ApiFailure && error.code === "stale") {
                      setKeep(null);
                      refuse(error);
                      return;
                    }
                    throw error;
                  },
                )
              : Promise.resolve()
          }
        />
      ) : null}
    </>
  );
  const frame = keep ? (keep.choice === "off" ? "autonomy-override-off-warning" : "autonomy-override-warning") : menu ? "autonomy-override" : null;
  return { open, node, frame };
}
