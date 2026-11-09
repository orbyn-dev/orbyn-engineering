import {
  Check,
  ChevronRight,
  Copy,
  Eye,
  FileDiff as FileDiffIcon,
  FileText,
  Folder,
  FolderOpen,
  Globe,
  GitFork,
  Image as ImageIcon,
  Link2,
  ListChecks,
  Loader2,
  Plug,
  Search,
  Square,
  SquarePen,
  SquareTerminal,
  WrapText,
  X,
  AppWindow,
  Wrench,
  ChevronDown,
  type LucideIcon,
} from "lucide-react";
import { use, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { api } from "../../api/client";
import { callDuration, prettyJson } from "../../lib/format";
import { useAsync } from "../../lib/useAsync";
import { useNow } from "../../lib/useNow";
import { useUi } from "../../state/ui";
import { useWorkspace } from "../../state/workspace";
import { Collapsible } from "../../ui/Collapsible";
import { Dialog } from "../../ui/Dialog";
import { useOpenControl, useOpenState } from "../../ui/openState";
import { outputSpans } from "../content/code/ansi";
import { HtmlRender } from "./HtmlRender";
import { McpApp } from "./McpApp";
import type { PlanItem, ToolItem } from "./projection";
import { useThreadLogs } from "./threadLogs";
import { useThreadOfTimeline } from "./timelineThread";
import { argEntries, callKind, callLabel, callMs, callPath, callState, editDiff, LINES_SHOWN, ENTRIES_SHOWN, lineCount, listEntries, readRange, resultRows, ROWS_SHOWN, runLabel, plain, subagentTask, commandText, isWebAddress, scalar, shownPath, sourceLines, type CallKind, type CallRun, type CallState, pluralize, editStat, hostOf } from "./toolCalls";
import { parsed, toolFailed, traceTool } from "./toolTrace";
import { SurfacedWorkflows, WorkflowCards } from "./WorkflowControls";

const KIND_ICONS: Record<CallKind, LucideIcon> = {
  command: SquareTerminal,
  read: Eye,
  edit: SquarePen,
  create: FileText,
  search: Search,
  list: Folder,
  fetch: Globe,
  mcp: Plug,
  subagent: GitFork,
  image: ImageIcon,
  web: Globe,
  browser: AppWindow,
  other: Wrench,
};

function ItemDetails({ threadId, id }: { threadId: string; id: string }) {
  const raw = useAsync(`item:${threadId}:${id}`, (signal) => api.threadItem(threadId, id, signal));
  if (raw.error) return <p className="tool-empty">Not recorded for this item.</p>;
  return (
    <pre tabIndex={0} className="tool-output">
      {raw.data ? JSON.stringify(raw.data, null, 2) : "Loading..."}
    </pre>
  );
}

/** A quiet footer button that opens the call's stored record. */
function InspectItem({ item, label = "Raw result" }: { item: ToolItem; label?: string }) {
  // Kept with the thread's other open state, so the dialog survives its row leaving and returning.
  const [open, setOpen] = useOpenState(`inspect:${item.key}`, false);
  const threadId = useThreadOfTimeline();
  if (!threadId) return null;
  return (
    <>
      <button type="button" className="bt tool-inspect" onClick={() => setOpen(true)}>
        {label}
      </button>
      {open ? (
        <Dialog title="Item details" onClose={() => setOpen(false)}>
          <ItemDetails threadId={threadId} id={item.callId} />
        </Dialog>
      ) : null}
    </>
  );
}

/** The copy button; it says Copied for a moment. */
function CopyButton({ value, label }: { value: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const copy = () => {
    void navigator.clipboard?.writeText(value).then(() => {
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    });
  };
  return (
    <>
      {copied ? (
        <span className="ex" data-find-skip role="status">
          Copied
        </span>
      ) : null}
      <button type="button" className="icb" aria-label={copied ? "Copied" : label} title={label} onClick={copy}>
        {copied ? <Check size={14} aria-hidden /> : <Copy size={14} aria-hidden />}
      </button>
    </>
  );
}

/** A bar under capped content: the control on the left, a quiet note on the right. */
function MoreBar({ icon, label, note, pressed, onClick }: { icon: ReactNode; label: string; note: string; pressed?: boolean; onClick(): void }) {
  return (
    <button type="button" className="mo" aria-pressed={pressed} onClick={onClick}>
      {icon}
      {label}
      <span className="sp" />
      <span className="mt">{note}</span>
    </button>
  );
}

function OutputBlock({ item, running }: { item: ToolItem; running: boolean }) {
  const text = item.result ?? item.output;
  const [all, setAll] = useOpenState(`out-all:${item.key}`, false);
  const [wrap, setWrap] = useOpenState(`out-wrap:${item.key}`, true);
  const scroller = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const lines = lineCount(text ?? "");
  const capped = !running && lines > LINES_SHOWN && !all;
  const mode = running ? "tail scr" : capped ? "cap" : lines > LINES_SHOWN ? "scr" : "";
  // Only a running call follows its end. A capped block is clipped, so whatever it had scrolled to would
  // stay hidden under it: it always starts at its first line, also after a run ends and after "Show less".
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    if (mode === "cap") element.scrollTop = 0;
    else if (running && following.current) element.scrollTop = element.scrollHeight;
  });
  // A line wider than the block would scroll sideways with wrapping off; measure that, and again when the block resizes.
  const [wide, setWide] = useState(false);
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const measure = () => {
      const before = element.style.whiteSpace;
      element.style.whiteSpace = "pre";
      const overflows = element.scrollWidth > element.clientWidth + 1;
      element.style.whiteSpace = before;
      setWide(overflows);
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [text, all, mode]);
  if (!text) return running ? null : <div className="co mute tool-empty">No output</div>;
  return (
    <>
      <div
        ref={scroller}
        tabIndex={mode.includes("scr") ? 0 : undefined}
        className={`co ${mode} ${wrap ? "" : "nowrap"}`}
        onScroll={(event) => {
          const element = event.currentTarget;
          following.current = element.scrollHeight - element.scrollTop - element.clientHeight <= 8;
        }}
      >
        {outputSpans(text).map((span, at) =>
          span.color || span.bold ? (
            <span key={at} style={{ color: span.color ?? undefined, fontWeight: span.bold ? 600 : undefined }}>
              {span.text}
            </span>
          ) : (
            span.text
          ),
        )}
      </div>
      {!running && lines > LINES_SHOWN ? (
        <MoreBar
          icon={<ChevronDown size={14} aria-hidden className={all ? "up" : undefined} />}
          label={all ? "Show less" : `Show all ${lines} lines`}
          note={all ? `${lines} lines · scrolls here` : `Fades after ${LINES_SHOWN} lines`}
          onClick={() => setAll(!all)}
        />
      ) : null}
      {wide || !wrap ? (
        <MoreBar
          icon={<WrapText size={14} aria-hidden />}
          label="Wrap lines"
          note={wrap ? "On" : "Scrolls sideways"}
          pressed={wrap}
          onClick={() => setWrap(!wrap)}
        />
      ) : null}
    </>
  );
}

function Kvs({ rows }: { rows: [string, string][] }) {
  return (
    <div className="kvs">
      {rows.flatMap(([key, value], at) => [
        <b key={`k${at}`} data-find-skip>
          {key}
        </b>,
        <span key={`v${at}`}>{value}</span>,
      ])}
    </div>
  );
}

/** A header line inside a block: an icon or `$`, what it is, and a quiet note. */
function BlockHead({ lead, children, note, tools, counted }: { lead: ReactNode; children: ReactNode; note?: string | null; tools?: ReactNode; counted?: boolean }) {
  return (
    <div className="cm" data-find-skip={counted ? undefined : true}>
      <span className="pr" aria-hidden>
        {lead}
      </span>
      <span className="cx">{children}</span>
      {note ? <span className="ex">{note}</span> : null}
      {tools}
    </div>
  );
}

function SourceLines({ lines, total, expandable, id }: { lines: { n: number | null; text: string }[]; total: number; expandable: boolean; id: string }) {
  const [all, setAll] = useOpenState(`src-all:${id}`, false);
  const capped = expandable && total > LINES_SHOWN && !all;
  return (
    <>
      <div className={`co ${capped ? "cap" : total > LINES_SHOWN ? "scr" : ""}`} style={{ padding: 0 }} tabIndex={total > LINES_SHOWN && !capped ? 0 : undefined}>
        <div className="src">
          {lines.map((line, at) => (
            <div key={at} className="sl">
              <i data-find-skip>{line.n ?? ""}</i>
              <span>{line.text}</span>
            </div>
          ))}
        </div>
      </div>
      {expandable && total > LINES_SHOWN ? (
        <MoreBar
          icon={<ChevronDown size={14} aria-hidden className={all ? "up" : undefined} />}
          label={all ? "Show less" : `Show all ${total} lines`}
          note={all ? `${total} lines · scrolls here` : `Fades after ${LINES_SHOWN} lines`}
          onClick={() => setAll(!all)}
        />
      ) : null}
    </>
  );
}

function FooterText({ children }: { children: ReactNode }) {
  return (
    <span className="tfm" data-find-skip>
      {children}
    </span>
  );
}

const Dot = () => <span className="dot"> · </span>;

function stopNote(state: CallState): ReactNode {
  return state === "stopped" ? "Stopped" : null;
}

function Block({ item, running, state, ms, cwd }: { item: ToolItem; running: boolean; state: CallState; ms: number | null; cwd: string | null }) {
  const kind = callKind(item);
  const origin = { threadId: useThreadOfTimeline() ?? "", cwd: cwd ?? "" };
  const revealFile = useUi((s) => s.revealFile);
  const revealDiff = useUi((s) => s.revealDiff);
  const showInBrowser = useUi((s) => s.showInBrowser);
  const body = parsed(item.args);
  const duration = ms === null ? null : callDuration(ms);
  const exit = item.exitCode;
  const failed = toolFailed(item);
  const out = item.result ?? item.output;
  const path = callPath(item);
  const shown = path ? shownPath(path, cwd) : null;
  const stopped = stopNote(state);
  const timing = (...parts: ReactNode[]) => (
    <FooterText>
      {[...parts, duration].filter(Boolean).map((part, at) => (
        <span key={at}>
          {at > 0 ? <Dot /> : null}
          {part}
        </span>
      ))}
    </FooterText>
  );

  switch (kind) {
    case "command": {
      const command = traceTool(item).command;
      return (
        <div className="cb">
          <BlockHead counted lead="$" tools={command ? <CopyButton value={command} label="Copy command" /> : null}>
            {commandText(item)}
          </BlockHead>
          <OutputBlock item={item} running={running} />
          <div className="cf">
            <FooterText>
              {running ? (
                <>
                  <Loader2 size={12} className="spin" aria-hidden /> Running
                </>
              ) : stopped ? (
                stopped
              ) : exit !== null ? (
                <span className={failed ? "er" : undefined}>exit {exit}</span>
              ) : null}
              {duration ? (
                <>
                  {running || stopped || exit !== null ? <Dot /> : null}
                  {duration}
                </>
              ) : null}
              {cwd ? (
                <>
                  <Dot />
                  {cwd}
                </>
              ) : null}
            </FooterText>
            <InspectItem item={item} />
          </div>
        </div>
      );
    }
    case "read": {
      const range = readRange(item);
      const lines = sourceLines(item.result, range?.from ?? 1);
      return (
        <div className="cb">
          <BlockHead lead={<FileText size={14} />} note={range ? `lines ${range.from}${range.to !== null ? `–${range.to}` : ""}` : null}>
            {shown ?? "File"}
          </BlockHead>
          <SourceLines lines={lines} total={lines.length} expandable id={item.key} />
          <div className="cf">
            {timing(lines.length ? pluralize(lines.length, "line", "lines") : null)}
            {path ? (
              <button type="button" className="bt" onClick={() => revealFile(path, range?.from ?? null, origin)}>
                <FolderOpen size={14} aria-hidden />
                <span>Open file</span>
              </button>
            ) : null}
            <InspectItem item={item} />
          </div>
        </div>
      );
    }
    case "edit": {
      const diff = editDiff(item);
      const stat = editStat(item);
      if (!diff) return <GenericBlock item={item} running={running} duration={duration} />;
      return (
        <div className="cb">
          <BlockHead lead={<SquarePen size={14} />}>{shown ?? "Files"}</BlockHead>
          <div className="hunk">
            {diff.removed.map((line, at) => (
              <div key={`d${at}`} className="del">
                <span data-find-skip />
                <span data-find-skip>−</span>
                <span>{line}</span>
              </div>
            ))}
            {diff.added.map((line, at) => (
              <div key={`a${at}`} className="add">
                <span data-find-skip />
                <span data-find-skip>+</span>
                <span>{line}</span>
              </div>
            ))}
          </div>
          <div className="cf">
            {timing(stat ? `+${stat.added} −${stat.removed}` : null)}
            {path ? (
              <button type="button" className="bt" onClick={() => revealDiff(path, origin)}>
                <FileDiffIcon size={14} aria-hidden />
                <span>Open in Diff</span>
              </button>
            ) : null}
            <InspectItem item={item} />
          </div>
        </div>
      );
    }
    case "create": {
      const content = typeof body.content === "string" ? body.content : null;
      if (content === null) return <GenericBlock item={item} running={running} duration={duration} />;
      const lines = content.replace(/\n$/, "").split("\n").map((text, at) => ({ n: at + 1, text }));
      return (
        <div className="cb">
          <BlockHead lead={<FileText size={14} />}>{shown ?? "File"}</BlockHead>
          <SourceLines lines={lines} total={lines.length} expandable id={item.key} />
          <div className="cf">
            {timing(`${/created successfully/i.test(item.result ?? "") ? "New file" : "File"} · ${pluralize(lines.length, "line", "lines")}`)}
            {path ? (
              <button type="button" className="bt" onClick={() => revealFile(path, null, origin)}>
                <FolderOpen size={14} aria-hidden />
                <span>Open file</span>
              </button>
            ) : null}
            <InspectItem item={item} />
          </div>
        </div>
      );
    }
    case "search":
      return <SearchBlock item={item} duration={duration} />;
    case "list": {
      const entries = listEntries(out);
      return (
        <ListBlock item={item} entries={entries} duration={duration} onOpen={path ? () => revealFile(path, null, origin) : null} shown={shown} />
      );
    }
    case "fetch": {
      const url = typeof body.url === "string" ? body.url : null;
      return (
        <div className="cb">
          <BlockHead lead={<Globe size={14} />}>{url ? hostOf(url) : "Page"}</BlockHead>
          {url ? <Kvs rows={[["URL", url]]} /> : null}
          {out ? <div className="co sans">{plain(out)}</div> : null}
          <div className="cf">
            {timing()}
            {url ? (
              <>
                {isWebAddress(url) ? (
                  <button type="button" className="bt" onClick={() => showInBrowser(url)}>
                    <Globe size={14} aria-hidden />
                    <span>Open in Browser</span>
                  </button>
                ) : null}
                <button type="button" className="bt" onClick={() => void navigator.clipboard?.writeText(url)}>
                  <Link2 size={14} aria-hidden />
                  <span>Copy link</span>
                </button>
              </>
            ) : null}
            <InspectItem item={item} />
          </div>
        </div>
      );
    }
    case "mcp": {
      const label = callLabel(item, cwd);
      const rows = argEntries(item);
      return (
        <div className="cb">
          <BlockHead lead={<Plug size={14} />}>
            {label.verb} · {label.object}
          </BlockHead>
          {rows.length ? <Kvs rows={rows} /> : null}
          {out ? <div className="co sans">{plain(out)}</div> : null}
          <div className="cf">
            {timing()}
            <InspectItem item={item} />
          </div>
        </div>
      );
    }
    case "subagent": {
      const rows: [string, string][] = [];
      const task = subagentTask(body) ?? "";
      if (task) rows.push(["Task", task]);
      if (out) rows.push(["Result", plain(out)]);
      return (
        <div className="cb">
          <BlockHead lead={<GitFork size={14} />}>{callLabel(item, cwd).object ?? "Subagent"}</BlockHead>
          {rows.length ? <Kvs rows={rows} /> : null}
          <div className="cf">
            {timing()}
            <InspectItem item={item} />
          </div>
        </div>
      );
    }
    default:
      return <GenericBlock item={item} running={running} duration={duration} />;
  }
}

/** What a call with no drawn block shows: its arguments, then its output. */
function GenericBlock({ item, running, duration }: { item: ToolItem; running: boolean; duration: string | null }) {
  const args = prettyJson(item.args);
  return (
    <div className="cb">
      {args ? (
        <pre className="co" tabIndex={0}>
          {args}
        </pre>
      ) : (
        <pre className="co" tabIndex={0} data-find-skip>
          None
        </pre>
      )}
      <OutputBlock item={item} running={running} />
      <div className="cf">
        <FooterText>{duration}</FooterText>
        <InspectItem item={item} />
      </div>
    </div>
  );
}

function SearchBlock({ item, duration }: { item: ToolItem; duration: string | null }) {
  const [all, setAll] = useOpenState(`rows-all:${item.key}`, false);
  const body = parsed(item.args);
  const rows = resultRows(item.result ?? item.output);
  const query = scalar(body.pattern ?? body.query ?? body.path);
  return (
    <div className="cb">
      <BlockHead lead={<Search size={14} />} note={`${pluralize(rows.length, "result", "results")}`}>
        {query}
      </BlockHead>
      {rows.length ? (
        <div className={`rs ${all ? (rows.length > 12 ? "scr" : "") : rows.length > ROWS_SHOWN ? "cap7" : ""}`}>
          {rows.map((row, at) => (
            <div key={at} className="r1">
              <span>{row.where}</span>
              <span>{row.text}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="co mute tool-empty">No results</div>
      )}
      {rows.length > ROWS_SHOWN ? (
        <MoreBar
          icon={<ChevronDown size={14} aria-hidden className={all ? "up" : undefined} />}
          label={all ? "Show less" : `Show all ${rows.length} results`}
          note=""
          onClick={() => setAll(!all)}
        />
      ) : null}
      <div className="cf">
        <FooterText>{duration}</FooterText>
        <InspectItem item={item} />
      </div>
    </div>
  );
}

function ListBlock({ item, entries, duration, onOpen, shown }: { item: ToolItem; entries: string[]; duration: string | null; onOpen: (() => void) | null; shown: string | null }) {
  const [all, setAll] = useOpenState(`rows-all:${item.key}`, false);
  return (
    <div className="cb">
      <BlockHead lead={<Folder size={14} />}>{shown ?? "Folder"}</BlockHead>
      {entries.length ? (
        <div className={`rs ${all ? "" : entries.length > ENTRIES_SHOWN ? "cap6" : ""}`}>
          {entries.map((name, at) => (
            <div key={at} className="r1 one">
              {name.endsWith("/") ? <Folder size={14} aria-hidden /> : <FileText size={14} aria-hidden />}
              <span>{name}</span>
            </div>
          ))}
        </div>
      ) : (
        <div className="co mute tool-empty">Nothing here</div>
      )}
      {entries.length > ENTRIES_SHOWN ? (
        <MoreBar
          icon={<ChevronDown size={14} aria-hidden className={all ? "up" : undefined} />}
          label={all ? "Show less" : `Show all ${entries.length} entries`}
          note=""
          onClick={() => setAll(!all)}
        />
      ) : null}
      <div className="cf">
        <FooterText>
          {pluralize(entries.length, "entry", "entries")}
          {duration ? <> <Dot />{duration}</> : null}
        </FooterText>
        {onOpen ? (
          <button type="button" className="bt" onClick={onOpen}>
            <FolderOpen size={14} aria-hidden />
            <span>Open in Files</span>
          </button>
        ) : null}
        <InspectItem item={item} />
      </div>
    </div>
  );
}

function Duration({ ms, state, item }: { ms: number | null; state: CallState; item: ToolItem }) {
  const text = ms === null ? null : callDuration(ms);
  const exit = item.exitCode;
  if (state === "failed" && exit !== null && exit !== 0) {
    return (
      <span className="du">
        <span className="er">exit {exit}</span>
        {text ? ` · ${text}` : ""}
      </span>
    );
  }
  return <span className="du">{text ?? ""}</span>;
}

/** The row's status for a reader who cannot see its mark: the label replaces the button's own content. */
function withState(label: string, state: CallState): string {
  // A label that already says "Running" does not say it twice.
  if (state === "running" && label.startsWith("Running")) return label;
  const word = state === "running" ? "running" : state === "failed" ? "failed" : state === "stopped" ? "stopped" : "";
  return word ? `${label}, ${word}` : label;
}

/** The duration as it is read aloud: with the exit code when there is one. */
function spokenDuration(ms: number | null, state: CallState, item: ToolItem): string {
  const text = ms === null ? "" : callDuration(ms);
  if (state === "failed" && item.exitCode !== null && item.exitCode !== 0) return `exit ${item.exitCode}${text ? ` · ${text}` : ""}`;
  return text;
}

function StateIcon({ state, Icon }: { state: CallState; Icon: LucideIcon }) {
  if (state === "running") return <Loader2 size={14} className="spin" role="img" aria-label="Working" />;
  if (state === "failed") return <X size={14} role="img" aria-label="Tool call failed" />;
  if (state === "stopped") return <Square size={14} role="img" aria-label="Stopped" />;
  return <Icon size={14} aria-hidden />;
}

const stateClass = (state: CallState) => (state === "running" ? "s-run" : state === "failed" ? "s-fail" : state === "stopped" ? "s-stop" : "");

/** Shows the full text under a row whose label was cut off, while the row has the pointer or focus. */
function useTip() {
  const [tip, setTip] = useState(false);
  const label = useRef<HTMLSpanElement>(null);
  const show = () => {
    const element = label.current;
    if (element && element.scrollWidth > element.clientWidth) setTip(true);
  };
  return { tip, label, props: { onMouseEnter: show, onFocus: show, onMouseLeave: () => setTip(false), onBlur: () => setTip(false) } };
}

/** One tool call: a row, and the block it opens. */
export function ToolCallRow({ item, live }: { item: ToolItem; live: boolean }) {
  const threadId = useThreadOfTimeline();
  const cwd = useWorkspace((s) => (threadId ? s.threads[threadId]?.cwd ?? null : null));
  const cancelled = useThreadLogs((s) => (threadId && item.runId ? s.logs[threadId]?.runs[item.runId]?.outcome === "cancelled" : false));
  const surfaced = use(SurfacedWorkflows);
  const state = callState(item, live, cancelled);
  const running = state === "running";
  const now = useNow(running);
  const ms = callMs(item, running ? now : null);
  const kind = callKind(item);
  const label = callLabel(item, cwd);
  const Icon = KIND_ICONS[kind];
  const full = [label.verb, label.object, label.note].filter(Boolean).join(" ");
  const { tip, label: labelRef, props } = useTip();
  const spoken = withState([full, spokenDuration(ms, state, item)].filter(Boolean).join(" "), state);
  const command = kind === "command" ? traceTool(item).command : null;
  return (
    <>
      <Collapsible
        className="tcw"
        persistId={`tool:${item.key}`}
        data-live={running || undefined}
        summaryClassName={`tcr ${stateClass(state)}`.trim()}
        summaryProps={{ ...props, "aria-label": spoken }}
        summary={
          <>
            <span className="tic">
              <StateIcon state={state} Icon={Icon} />
            </span>
            <span className="lb" ref={labelRef} data-command={command ?? undefined}>
              {label.verb ? <span className="tla">{label.verb}</span> : null}
              {label.object ? <span className={`tlb${label.fill ? " tlfill" : ""}`}>{label.object}</span> : null}
              {label.note ? <span className="tln">{label.note}</span> : null}
              {tip ? (
                <span className="tctip" role="tooltip">
                  {full}
                </span>
              ) : null}
            </span>
            <Duration ms={ms} state={state} item={item} />
            <span className="tcv">
              <ChevronRight size={14} aria-hidden />
            </span>
          </>
        }
      >
        <Block item={item} running={running} state={state} ms={ms} cwd={cwd} />
      </Collapsible>
      {/* A published page is the result, so it stays visible with the details closed. */}
      <HtmlRender item={item} threadId={threadId} />
      <McpApp item={item} threadId={threadId} />
      {surfaced ? null : <WorkflowCards items={[item]} threadId={threadId} live={live} />}
    </>
  );
}

/** Consecutive calls of one kind: a header, then the calls one indent in. One call is just its own row. */
export function TrailRun({ run, live }: { run: CallRun; live: boolean }) {
  const [open, setOpen] = useOpenState(`tg:${run.key}`, false);
  const { peek } = useOpenControl();
  const threadId = useThreadOfTimeline();
  const cancelled = useThreadLogs((s) => (threadId && run.items[0]?.runId ? s.logs[threadId]?.runs[run.items[0].runId]?.outcome === "cancelled" : false));
  const only = run.items[0];
  const states = run.items.map((item) => callState(item, live, cancelled));
  const running = states.includes("running");
  const failedAt = states.indexOf("failed");
  const state: CallState = running ? "running" : failedAt >= 0 ? "failed" : states.includes("stopped") ? "stopped" : "done";
  const now = useNow(running);
  const total = run.items.reduce<number | null>((sum, item) => {
    const ms = callMs(item, callState(item, live, cancelled) === "running" ? now : null);
    return ms === null ? sum : (sum ?? 0) + ms;
  }, null);
  const cwd = useWorkspace((s) => (threadId ? s.threads[threadId]?.cwd ?? null : null));
  // When a second call joins a row someone opened, the new group opens with it rather than hiding it.
  // Keyed off what the thread remembers, not what is mounted: a group nobody has opened or closed yet.
  const count = run.items.length;
  useEffect(() => {
    if (count >= 2 && peek(`tg:${run.key}`) === undefined && run.items.some((item) => peek(`tool:${item.key}`) === true)) setOpen(true);
  }, [count, run.key, run.items, peek, setOpen]);
  if (!only) return null;
  if (count === 1) return <ToolCallRow item={only} live={live} />;
  const label = runLabel(run, running);
  const Icon = KIND_ICONS[run.kind];
  const current = run.items[states.lastIndexOf("running")];
  const failing = failedAt >= 0 ? run.items[failedAt] : undefined;
  const lead = !open && running && current ? "Running" : label.verb;
  const detail = !open ? (running && current ? callLabel(current, cwd).object : failing ? callLabel(failing, cwd).object : null) : null;
  const exit = failing?.exitCode;
  const exited = !open && failing && exit !== null && exit !== undefined && exit !== 0;
  const duration = total === null ? "" : callDuration(total);
  const command = detail && run.kind === "command" ? detail : undefined;
  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      className="tcw"
      summaryClassName={`tcr ${stateClass(state)}`.trim()}
      summaryProps={{
        "aria-label": withState(
          [lead, detail, !detail && label.note ? `· ${label.note}` : null, exited ? `exit ${exit} ·` : null, duration].filter(Boolean).join(" "),
          state,
        ),
      }}
      summary={
        <>
          <span className="tic">
            <StateIcon state={state} Icon={Icon} />
          </span>
          <span className="lb" data-command={command}>
            <span className="tla">{lead}</span>
            {detail ? <span className="tlb tlfill">{detail}</span> : null}
            {!detail && label.note ? <span className="tln">· {label.note}</span> : null}
          </span>
          <span className="du">
            {exited ? (
              <>
                <span className="er">exit {exit}</span> · {duration}
              </>
            ) : (
              duration
            )}
          </span>
          <span className="tcv">
            <ChevronRight size={14} aria-hidden />
          </span>
        </>
      }
    >
      <div className="tk">
        {run.items.map((item) => (
          <ToolCallRow key={item.key} item={item} live={live} />
        ))}
      </div>
    </Collapsible>
  );
}

/** A plan update, as a row that lists the steps. */
export function PlanRow({ item, live }: { item: PlanItem; live: boolean }) {
  if (item.steps.length === 0) return null;
  const done = (status: string | null) => status === "completed" || status === "done";
  const active = (status: string | null) => status === "in_progress" || status === "running";
  const finished = item.steps.filter((step) => done(step.status)).length;
  return (
    <section className="steps tc" aria-label="Plan">
      <div className="tg">
        <Collapsible
          className="tcw"
          persistId={`plan:${item.key}`}
          summaryClassName="tcr"
          summary={
            <>
              <span className="tic">
                <ListChecks size={14} aria-hidden />
              </span>
              <span className="lb">
                <span className="tla">Updated plan</span>
                <span className="tln">· {finished} of {item.steps.length} done</span>
              </span>
              <span className="du" />
              <span className="tcv">
                <ChevronRight size={14} aria-hidden />
              </span>
            </>
          }
        >
          <div className="cb">
            <BlockHead lead={<ListChecks size={14} />}>Plan</BlockHead>
            <ol className="td">
              {item.steps.map((step, at) => (
                <li key={at} className={`t1r ${done(step.status) ? "tdone" : active(step.status) ? "tnow" : ""}`}>
                  <span className="bx">
                    {done(step.status) ? <Check size={12} aria-hidden /> : active(step.status) && live ? <Loader2 size={12} className="spin" aria-hidden /> : null}
                  </span>
                  <span>{step.text}</span>
                  {step.status ? <span className="sr-only">({step.status.replaceAll("_", " ")})</span> : null}
                </li>
              ))}
            </ol>
            <div className="cf">
              <FooterText>{pluralize(item.steps.length, "step", "steps")}</FooterText>
            </div>
          </div>
        </Collapsible>
      </div>
    </section>
  );
}

/** Rows a person moves between with the arrow keys. */
export function trailKeys(event: React.KeyboardEvent<HTMLElement>, onCopied?: () => void) {
  const target = event.target as HTMLElement;
  if (!target.matches("button.tcr")) return;
  const root = event.currentTarget;
  const rows = [...root.querySelectorAll<HTMLButtonElement>("button.tcr")].filter((row) => !row.closest("[hidden]"));
  const at = rows.indexOf(target as HTMLButtonElement);
  const open = target.getAttribute("aria-expanded") === "true";
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      rows[Math.min(at + 1, rows.length - 1)]?.focus();
      break;
    case "ArrowUp":
      event.preventDefault();
      rows[Math.max(at - 1, 0)]?.focus();
      break;
    case "ArrowRight":
      if (!open) {
        event.preventDefault();
        target.click();
      }
      break;
    case "ArrowLeft":
      if (open) {
        event.preventDefault();
        target.click();
      }
      break;
    case "c":
    case "C": {
      if (event.metaKey || event.ctrlKey || event.altKey) break;
      const command = target.querySelector<HTMLElement>("[data-command]")?.dataset.command;
      if (command) {
        event.preventDefault();
        void navigator.clipboard?.writeText(command).then(() => onCopied?.());
      }
      break;
    }
  }
}
