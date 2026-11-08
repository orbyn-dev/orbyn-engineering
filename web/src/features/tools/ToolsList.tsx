import { Check, ChevronDown, ChevronRight, ExternalLink, Link2, LoaderCircle, Plug, Puzzle } from "lucide-react";
import { useState } from "react";
import type { EngineTools, ToolEntry, ToolGroup } from "../../api/types";
import { Button } from "../../ui/controls";
import { navigate } from "../../state/route";
import { GROUP_LABEL, notReportedNotes } from "./toolsModel";

/** Groups that start open: the ones a person set up. The rest are always there. */
const OPEN_AT_FIRST = new Set(["mcp", "connectors", "plugins"]);

const ICON: Record<string, typeof Plug> = { mcp: Plug, connectors: Link2, plugins: Puzzle };

function StatusCell({ entry, onDetails, open }: { entry: ToolEntry; onDetails(): void; open: boolean }) {
  if (entry.state === "ready") {
    return (
      <span className="tg-status">
        <span className="tg-ready">
          <Check size={13} aria-hidden /> Ready
        </span>
      </span>
    );
  }
  if (entry.state === "starting") {
    return (
      <span className="tg-status">
        <span className="tg-ready">
          <LoaderCircle size={13} className="spin" aria-hidden /> Starting…
        </span>
      </span>
    );
  }
  if (entry.state === "needs_sign_in") {
    return (
      <span className="tg-status">
        <span className="tg-amber">Needs sign-in</span>
        {entry.sign_in_url ? (
          <a className="btn btn-small tg-link" href={entry.sign_in_url} target="_blank" rel="noreferrer noopener">
            Sign in <ExternalLink size={11} aria-hidden />
          </a>
        ) : null}
      </span>
    );
  }
  return (
    <span className="tg-status">
      <span className="tg-bad">Failed</span>
      {entry.detail || entry.reason ? (
        <Button small tone="quiet" aria-expanded={open} onClick={onDetails}>
          Details
        </Button>
      ) : null}
    </span>
  );
}

function ToolRow({ entry, group, mode }: { entry: ToolEntry; group: string; mode: "thread" | "settings" }) {
  const [open, setOpen] = useState(false);
  const Icon = ICON[group] ?? Plug;
  const tint = mode === "thread" && entry.state === "failed" ? " tg-row-bad" : mode === "thread" && entry.state === "needs_sign_in" ? " tg-row-amber" : "";
  // The Settings page puts where an entry comes from on its description line.
  const line = [entry.description ?? (entry.tools ? `${entry.tools} tools` : null), mode === "settings" ? entry.source : null]
    .filter((part): part is string => Boolean(part))
    .join(" · ");
  return (
    <li className={`tg-row${tint}`}>
      <Icon size={14} aria-hidden className="tg-icon" />
      <span className="tg-name">
        <b>{entry.name}</b>
        {line ? <small>{line}</small> : null}
        {entry.reason && entry.state !== "ready" ? <small className="tg-why">{entry.reason}</small> : null}
        {open && entry.detail ? <code className="tg-detail">{entry.detail}</code> : null}
      </span>
      <StatusCell entry={entry} open={open} onDetails={() => setOpen(!open)} />
    </li>
  );
}

function GroupBlock({ group, mode }: { group: ToolGroup; mode: "thread" | "settings" }) {
  const [open, setOpen] = useState(OPEN_AT_FIRST.has(group.id));
  const expandable = group.items.length > 0;
  const label = GROUP_LABEL[group.id] ?? group.id;
  return (
    <section className="tg-group" aria-label={label}>
      <div className="tg-head">
        <button
          type="button"
          className="tg-toggle"
          aria-expanded={expandable ? open : undefined}
          disabled={!expandable}
          onClick={() => setOpen(!open)}
        >
          {expandable ? open ? <ChevronDown size={13} aria-hidden /> : <ChevronRight size={13} aria-hidden /> : <span className="tg-nochev" aria-hidden />}
          <span>{label}</span>
          <span className="tg-count">{group.count}</span>
        </button>
        {group.id === "skills" ? (
          <a className="tg-open-skills" href="#/skills" onClick={(event) => { event.preventDefault(); navigate({ name: "skills" }); }}>
            Open Skills
          </a>
        ) : null}
      </div>
      {expandable && open ? (
        <ul className="tg-rows">
          {group.items.map((entry) => (
            <ToolRow key={`${entry.name}:${entry.source ?? ""}`} entry={entry} group={group.id} mode={mode} />
          ))}
        </ul>
      ) : null}
    </section>
  );
}

/** The groups an engine reported, then one quiet line for each it did not. Read-only: no switches. */
export function ToolsList({ tools, engineName, mode }: { tools: EngineTools; engineName: string; mode: "thread" | "settings" }) {
  return (
    <div className="tg-list">
      {tools.groups.map((group) => (
        <GroupBlock key={group.id} group={group} mode={mode} />
      ))}
      {tools.incomplete ? <p className="tg-note">The list may be incomplete.</p> : null}
      {notReportedNotes(engineName, tools).map((line) => (
        <p key={line} className="tg-note">
          {line}
        </p>
      ))}
    </div>
  );
}
