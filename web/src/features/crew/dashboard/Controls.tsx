import { useEffect, useRef, useState } from "react";
import { Ic } from "../shared/parts";
import { PERIODS, type Period } from "./contract";
import { Planned, WHY } from "./parts";
import { ALL } from "./prefs";
import { text } from "./model";

export interface Choosable {
  id: string;
  name: string;
}

/**
 * The project menu: the repositories Crew can be asked about and All projects. The one shown is checked, and choosing another reads that one. It
 * closes on Escape, on a click elsewhere and on choosing; there is no count beside a name, because a count would need a read of every repository first.
 */
export function ProjectMenu({ projects, current, onPick, open: initialOpen }: { projects: readonly Choosable[]; current: string; onPick(id: string): void; open?: boolean }) {
  const [open, setOpen] = useState(initialOpen === true);
  const root = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (event: Event) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);
  const label = current === ALL ? "All projects" : text(projects.find((entry) => entry.id === current)?.name) || "Choose a project";
  const choose = (id: string) => {
    setOpen(false);
    if (id !== current) onPick(id);
  };
  return (
    <span ref={root} style={{ position: "relative", display: "inline-flex" }}>
      <button type="button" className="cx-t10sel" aria-haspopup="menu" aria-expanded={open} aria-label={`Project: ${label}`} onClick={() => setOpen(!open)}>
        <Ic n="folder" sm />
        {label}
        <Ic n="down" sm />
      </button>
      {open ? (
        <div className="cx-t10pop" role="menu" aria-label="Project">
          <div className="cx-t10g2">Project</div>
          {projects.map((entry) => (
            <button key={entry.id} type="button" role="menuitemradio" aria-checked={entry.id === current} className={entry.id === current ? "cx-t10o cx-on" : "cx-t10o"} onClick={() => choose(entry.id)}>
              {entry.id === current ? <Ic n="check2" sm /> : <span className="cx-t10gap" />}
              {text(entry.name) || "Unnamed project"}
            </button>
          ))}
          <div className="cx-t10g2">Or</div>
          <button type="button" role="menuitemradio" aria-checked={current === ALL} className={current === ALL ? "cx-t10o cx-on" : "cx-t10o"} onClick={() => choose(ALL)}>
            {current === ALL ? <Ic n="check2" sm /> : <Ic n="folder" sm />}
            All projects
          </button>
        </div>
      ) : null}
    </span>
  );
}

/**
 * 30, 60 or 90 days. While the service has no history read it reads one window, so `fixed` names that window: it is the one drawn as chosen, the others
 * are disabled and Planned, not hidden, and the page never claims a period it did not read.
 */
export function PeriodChoice({ days, onPick, fixed }: { days: number; onPick(days: Period): void; fixed: boolean }) {
  return (
    <span className="cx-cwseg cx-dsseg" role="radiogroup" aria-label="Period">
      {PERIODS.map((period) => {
        const locked = fixed && period !== days;
        const why = `Planned: ${WHY.period}`;
        return (
          <button key={period} type="button" role="radio" aria-checked={period === days} disabled={locked} title={locked ? why : undefined} aria-description={locked ? why : undefined} className={period === days ? "cx-on" : undefined} onClick={() => onPick(period)}>
            {period} days
          </button>
        );
      })}
      {fixed ? <Planned why={WHY.period} /> : null}
    </span>
  );
}
