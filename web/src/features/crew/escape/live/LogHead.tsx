import type { ReactNode } from "react";
import type { Project } from "../../../../api/types";
import type { EscapeLog } from "../../../../generated/EscapeLog";
import type { AsyncState } from "../../../../lib/useAsync";
import { CoverageLink } from "../../capacity/live/CoverageLink";
import { useCrewHref } from "../../shared/nav";
import { Ic } from "../../shared/parts";
import { count } from "./model";
import { ProjectChip } from "./parts";

type Read = AsyncState<EscapeLog>;

/** The log's own width: the page alone (`full`), the same page in a 640 px window, or the 320 px side panel. */
export type LogVariant = "full" | "window" | "panel";

/** The header link of the escape pages: an underlined word and a chevron, as the Coverage link is. Opening it reads another page and sends nothing. */
function HeaderLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      className="cx-t10r"
      href={href}
      style={{ marginLeft: "0", whiteSpace: "nowrap", display: "inline-flex", alignItems: "center", gap: "2px", color: "inherit" }}
    >
      <span style={{ textDecoration: "underline", textUnderlineOffset: "2px" }}>{children}</span>
      <Ic n="right" sm />
    </a>
  );
}

/**
 * The count on the Possible tab, from the same read as the log's Possible tile: the number in full with its thousands separator,
 * an ellipsis read out as a status while the log is read, and a dash read out as "can’t read" when it can’t be. A count that was
 * never measured is never drawn as 0, and it is never added to the confirmed count.
 */
function PossibleCount({ read }: { read: Read }) {
  const mark = { marginLeft: "3px", fontWeight: 400, fontVariantNumeric: "tabular-nums" } as const;
  if (read.status === "loading")
    return (
      <b role="status" style={mark}>
        <b aria-hidden="true" style={{ fontWeight: 400 }}>…</b>
        <b className="sr-only">reading</b>
      </b>
    );
  if (read.status === "error")
    return (
      <b style={mark}>
        <b aria-hidden="true" style={{ fontWeight: 400 }}>–</b>
        <b className="sr-only">can’t read</b>
      </b>
    );
  return <b style={mark}>{count(read.data.possible)}</b>;
}

/** The two views of the log under its header: every escape, or the possible ones waiting for a decision. Each opens its page for the project shown. */
function LogTabs({ project, tab, read }: { project: string; tab: "all" | "possible"; read: Read }) {
  const href = useCrewHref();
  return (
    <div style={{ display: "flex" }}>
      <span className="cx-cwseg" role="tablist" aria-label="Escape log views">
        <a role="tab" aria-selected={tab === "all"} className={tab === "all" ? "cx-on" : ""} href={href({ page: "escapes", project })}>
          All escapes
        </a>
        <a role="tab" aria-selected={tab === "possible"} className={tab === "possible" ? "cx-on" : ""} href={href({ page: "possible", project })}>
          Possible{" "}
          <PossibleCount read={read} />
        </a>
      </span>
    </div>
  );
}

/** The 320 px side panel's own title row. Open in Crew only navigates. */
export function PanelHead({ onOpen }: { onOpen(): void }) {
  return (
    <div className="cx-phd2">
      <b>Crew</b>
      <span className="cx-cwpv">Preview</span>
      <span className="cx-pha">
        <button type="button" className="cx-ghost" style={{ height: "24px", fontSize: "12px", padding: "0 9px" }} onClick={onOpen}>Open in Crew</button>
      </span>
    </div>
  );
}

/**
 * The header both escape pages share (Crew Designs v87, the Settings link and the tab strip): the title, the links (Coverage
 * where the page has one, then Settings), the project, the window and, under them, the tab strip. `log` is the log read: its window
 * names the stored window, and its Possible count is the tab's. The window is the stored one, so the header names it and offers no
 * window of its own. Every link and tab opens a page and sends nothing.
 */
export function LogHead({
  variant,
  project,
  name,
  projects,
  onPick,
  read,
  tab,
  coverage,
}: {
  variant: LogVariant;
  project: string;
  name: string;
  projects: readonly Project[];
  onPick(id: string): void;
  read: Read;
  tab: "all" | "possible";
  /** The Coverage link sits on the log only. */
  coverage: boolean;
}) {
  const href = useCrewHref();
  const log = read.status === "ready" ? read.data : null;
  // The links sit before the project chip, so neither moves when the window picker changes.
  const links = (
    <>
      {coverage ? <CoverageLink project={project} from="escapes" /> : null}
      <HeaderLink href={href({ page: "escape-settings" })}>Settings</HeaderLink>
    </>
  );
  const chip = <ProjectChip name={name} projects={projects} current={project} onPick={onPick} />;
  const windowSeg = log ? <span className="cx-cwseg"><span className="cx-on">Last {log.window_days} days</span></span> : null;
  return (
    <>
      <div className="cx-t10hd">
        <b style={variant === "panel" ? { fontSize: "13px" } : undefined}>Escape log</b>
        <span className="cx-rtag cx-lo">This device only</span>
        {variant === "full" ? (
          <span className="cx-t10sp">
            {links}
            {chip}
            {windowSeg}
          </span>
        ) : null}
      </div>
      {variant === "panel" ? <div style={{ display: "flex", gap: "12px", flexWrap: "wrap" }}>{links}</div> : null}
      {variant === "window" ? (
        <div style={{ display: "flex", gap: "6px", flexWrap: "wrap", alignItems: "center" }}>
          {links}
          {chip}
          {windowSeg}
        </div>
      ) : null}
      <LogTabs project={project} tab={tab} read={read} />
    </>
  );
}
