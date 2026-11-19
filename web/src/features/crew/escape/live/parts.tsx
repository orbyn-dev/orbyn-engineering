import { useState } from "react";
import type { Project } from "../../../../api/types";
import type { EscapeGap } from "../../../../generated/EscapeGap";
import type { EscapeRow } from "../../../../generated/EscapeRow";
import { Menu, type MenuItem } from "../../../../ui/Menu";
import { Presence } from "../../../../ui/Presence";
import { Ic } from "../../shared/parts";
import { dayMoment, gapLabel, nameOf, sourceLabel, stateLabel } from "./model";

/** The state chip: a possible escape is neutral, a confirmed one strong, a dismissed one plain. */
export function StateChip({ row }: { row: Pick<EscapeRow, "state"> }) {
  const strong = row.state === "confirmed" ? " cx-no" : "";
  return <span className={`cx-apchip${strong}`}>{stateLabel[row.state]}</span>;
}

export function GapChip({ gap }: { gap: EscapeGap | null }) {
  return gap ? <span className="cx-apchip">{gapLabel[gap]}</span> : <small>Not set</small>;
}

/** The project chip. With several projects it opens a short menu: the one shown is checked, and choosing another reads that project. */
export function ProjectChip({ name, projects, current, onPick }: { name: string; projects: readonly Project[]; current: string; onPick(id: string): void }) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  if (projects.length < 2) return <span className="cx-t10sel"><Ic n="folder" sm />{name}</span>;
  const items: MenuItem[] = projects.map((entry) => ({
    label: entry.name,
    icon: <Ic n="folder" sm />,
    selected: entry.id === current,
    leadCheck: true,
    onSelect: () => onPick(entry.id),
  }));
  return (
    <>
      <button
        type="button"
        className="cx-t10sel"
        aria-haspopup="menu"
        aria-expanded={at !== null}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setAt(at ? null : { x: rect.left, y: rect.bottom + 4 });
        }}
      >
        <Ic n="folder" sm />
        {name}
        <Ic n="down" sm />
      </button>
      <Presence>{at ? <Menu x={at.x} y={at.y} label="Project" items={items} minWidth={230} onClose={() => setAt(null)} /> : null}</Presence>
    </>
  );
}

const COLUMNS = "minmax(0,.9fr) minmax(0,1.7fr) minmax(0,1.1fr) minmax(0,.7fr) minmax(0,1fr)";

/** One escape row of the log's table: what the read carries, and nothing it does not (no merge title, no merge time). */
function Row({ row, onOpen }: { row: EscapeRow; onOpen(id: string): void }) {
  return (
    <>
      <div>
        <b>{dayMoment(row.found_unix)}</b>
        <small>{sourceLabel(row.source)}</small>
      </div>
      <div>
        <button type="button" className="cx-linkname" onClick={() => onOpen(row.id)}>
          <b>{nameOf(row)}</b>
        </button>
        {row.reason ? <small>{row.line}</small> : null}
      </div>
      <div>
        <StateChip row={row} />
        {row.state === "possible" ? (
          <small>Waiting for you</small>
        ) : row.decided_unix && row.decided_by === "person" ? (
          <small>
            {row.state === "confirmed" ? "Confirmed" : "Dismissed"} by you, {dayMoment(row.decided_unix)}
          </small>
        ) : null}
      </div>
      <div>
        <GapChip gap={row.gap} />
      </div>
      <div>
        <b>{row.engine}</b>
        <small>{row.areas.join(" · ")}</small>
      </div>
    </>
  );
}

/** The log's table of escapes, newest found first. A row's name opens the escape's own page. */
export function EscapeTable({ rows, onOpen }: { rows: readonly EscapeRow[]; onOpen(id: string): void }) {
  return (
    <div className="cx-aptb" style={{ gridTemplateColumns: COLUMNS }}>
      <div className="cx-h">When found</div>
      <div className="cx-h">What escaped</div>
      <div className="cx-h">State</div>
      <div className="cx-h">Gap</div>
      <div className="cx-h">Counts against</div>
      {rows.map((row) => (
        <Row key={row.id} row={row} onOpen={onOpen} />
      ))}
    </div>
  );
}
