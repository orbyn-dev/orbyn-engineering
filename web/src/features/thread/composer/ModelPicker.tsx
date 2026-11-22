import { Check, ChevronDown, Loader2, Search } from "lucide-react";
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import type { EngineStatus } from "../../../api/types";
import { useCommand } from "../../../product/shell/shortcuts";
import { useUi } from "../../../product/state/ui";
import { EngineGlyph } from "../../../product/ui/badges";
import {
  chipFor, ClaudeHostActions, claudeHostDown, claudeHostSentence, claudeHostWaiting, engineGap, pickerGap, PickerGapRow, useEngineHosts,
  type Gap, type HostRow,
} from "../../enginehost";
import { UnmanagedLabel } from "../../../product/engines/UnmanagedLabel";
import { isUnmanaged } from "../../../product/engines/unmanaged";
import { CLAUDE_CLI_NOTE, CLAUDE_SDK_NOTE, useClaudeEnginePath, type ClaudeEnginePath } from "../../../product/engines/claudePath";
import { closingProps, Presence } from "../../../ui/Presence";
import { usePopover } from "../../../ui/usePopover";
import {
  arrangeModels,
  modelKey,
  resolveModel,
  shortModelName,
  useEngineModels,
  useModelStore,
  type PickerRow,
} from "../../../product/state/modelStore";

const MARGIN = 6;
const GAP = 4;

export type FloatingSide = "below" | "above" | "beside";

/** Below, above, then beside the trigger, kept inside the window, as the reference does. */
export function chooseSide(anchor: DOMRect, height: number, viewport: { height: number }): FloatingSide {
  if (anchor.bottom + GAP + height <= viewport.height - MARGIN) return "below";
  if (anchor.top - GAP - height >= MARGIN) return "above";
  return "beside";
}

/** Where the panel goes on a side. The side stays put while the content's height changes. */
export function placeFloating(
  anchor: DOMRect,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  side: FloatingSide = chooseSide(anchor, size.height, viewport),
): { left: number; top: number; origin: string } {
  const clampLeft = (left: number) => Math.max(MARGIN, Math.min(viewport.width - size.width - MARGIN, left));
  const clampTop = (top: number) => Math.max(MARGIN, Math.min(viewport.height - size.height - MARGIN, top));
  if (side === "below") return { left: clampLeft(anchor.left), top: clampTop(anchor.bottom + GAP), origin: "top left" };
  if (side === "above") {
    return { left: clampLeft(anchor.left), top: clampTop(anchor.top - GAP - size.height), origin: "bottom left" };
  }
  return { left: clampLeft(anchor.left - GAP - size.width), top: clampTop(anchor.top), origin: "right center" };
}

export function FloatingPicker({
  anchor,
  label,
  className,
  container,
  closing,
  panelRef,
  onKeyDown,
  children,
}: {
  anchor: DOMRect;
  label: string;
  className: string;
  container?: HTMLElement | null;
  /** Set by `Presence` while the panel fades out: it takes no focus or pointer. */
  closing?: boolean;
  /** The panel's element, for `usePopover`, which owns closing and focus. */
  panelRef: RefObject<HTMLDivElement | null>;
  onKeyDown?(event: React.KeyboardEvent<HTMLDivElement>): void;
  children: ReactNode;
}) {
  const ref = panelRef;
  const side = useRef<FloatingSide | null>(null);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const viewport = { width: window.innerWidth, height: window.innerHeight };
    // Chosen once, for the tallest it can get, so switching tabs cannot move it.
    const tallest = Math.max(element.offsetHeight, Number.parseFloat(getComputedStyle(element).maxHeight) || 0);
    side.current ??= chooseSide(anchor, tallest, viewport);
    const place = placeFloating(
      anchor,
      { width: element.offsetWidth, height: element.offsetHeight },
      viewport,
      side.current,
    );
    element.style.left = `${place.left}px`;
    element.style.top = `${place.top}px`;
    element.style.transformOrigin = place.origin;
  });
  return createPortal(
    <div ref={ref} className={className} role="dialog" aria-label={label} onKeyDown={onKeyDown} {...closingProps(closing)}>
      {children}
    </div>,
    container ?? document.body,
  );
}

interface Selection {
  engine: string;
  model: string | null;
}

const ALL = "all";

interface PickerSection {
  key: string;
  engine: EngineStatus;
  name: string;
  path: ClaudeEnginePath | null;
  note: string | null;
  host: HostRow | null;
}

interface SectionRow {
  row: PickerRow;
  section: PickerSection;
}

type SectionState =
  | { kind: "gap"; gap: Gap; host: HostRow | null }
  | { kind: "host"; host: HostRow; title: string; sentence: string; waiting: boolean };

const CLAUDE_SDK_KEY = "claude:sdk";
const CLAUDE_CLI_KEY = "claude:cli";

const signedOut = (engine: EngineStatus): boolean =>
  engine.auth === "logged_out" || engine.start_issue?.kind === "signed_out";

/** The engine's own state word, the same on both Claude paths because both run the same installed claude. */
function engineWord(engine: EngineStatus): "Ready" | "Not found" | "Signed out" {
  if (!engine.installed) return "Not found";
  if (signedOut(engine)) return "Signed out";
  return "Ready";
}

/** What stops the engine itself, whatever path is chosen: not installed, or signed out in its own terminal. */
function engineState(engine: EngineStatus, host: HostRow | null): Gap | null {
  if (!engine.installed) return engineGap(engine, host);
  if (signedOut(engine)) {
    return {
      kind: "expired",
      title: "Signed out",
      sentence: "Sign in to Claude Code in a terminal, then try again.",
      chip: { label: "Signed out", tone: "plain" },
      action: null,
    };
  }
  return null;
}

/** The two Claude path sections, or one section for every other engine. */
function sectionsOf(engines: readonly EngineStatus[], hosts: HostRow[] | null): PickerSection[] {
  return engines.flatMap<PickerSection>((engine): PickerSection[] => {
    const host = hosts?.find((row) => row.id === engine.id) ?? null;
    if (engine.id === "claude" && !engine.instance_of) {
      return [
        { key: CLAUDE_SDK_KEY, engine, name: "Claude (SDK)", path: "sdk" as const, note: CLAUDE_SDK_NOTE, host },
        { key: CLAUDE_CLI_KEY, engine, name: "Claude (CLI)", path: "cli" as const, note: CLAUDE_CLI_NOTE, host: null },
      ];
    }
    return [{ key: engine.id, engine, name: engine.name, path: null, note: null, host }];
  });
}

const hostTitle = (host: HostRow): string =>
  claudeHostWaiting(host) ? "Restarting the engine host" : host.state === "version_mismatch" ? "Version mismatch" : "Can’t start";

function sectionState(section: PickerSection): SectionState | null {
  if (!section.path) {
    const gap = pickerGap(section.engine, section.host);
    return gap ? { kind: "gap", gap, host: section.host } : null;
  }
  const own = engineState(section.engine, section.host);
  if (own) return { kind: "gap", gap: own, host: section.host };
  // The SDK alone uses the host: the CLI path stays pickable while it is down.
  if (section.path === "sdk" && section.host) {
    const gap = engineGap(section.engine, section.host);
    if (gap) return { kind: "gap", gap, host: section.host };
    if (claudeHostDown(section.host)) {
      const sentence = claudeHostSentence(section.host);
      if (sentence) return { kind: "host", host: section.host, title: hostTitle(section.host), sentence, waiting: claudeHostWaiting(section.host) };
    }
  }
  return null;
}

/** The engine's state word in the provider's heading, with its dot. */
function StateWord({ word }: { word: "Ready" | "Not found" | "Signed out" }) {
  return <span className={`model-state ${word === "Ready" ? "ok" : "plain"}`}>{word}</span>;
}

/** The engine host's own chip; the SDK row alone carries it. */
function HostStateChip({ host }: { host: HostRow }) {
  const chip = chipFor(host.state);
  return (
    <span className={`model-state ${chip.tone}`}>
      {chip.spin ? <Loader2 size={11} className="spin" aria-hidden /> : null}{chip.label}
    </span>
  );
}

function ModelPopover({
  engines,
  selected,
  anchor,
  portalContainer,
  trigger,
  onPick,
  closing,
  onClose,
}: {
  engines: EngineStatus[];
  selected: Selection;
  anchor: DOMRect;
  closing?: boolean;
  portalContainer?: HTMLElement | null;
  /** The button that opened it, where focus returns. */
  trigger(): HTMLElement | null | undefined;
  onPick(selection: Selection): void;
  onClose(): void;
}) {
  const hidden = useUi((state) => state.modelHidden);
  const order = useUi((state) => state.modelOrder);
  const byEngine = useModelStore((state) => state.byEngine);
  const load = useModelStore((state) => state.load);
  const { path, choose } = useClaudeEnginePath();
  const { hosts } = useEngineHosts();
  const [chip, setChip] = useState(ALL);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(-1);
  const [more, setMore] = useState({ above: false, below: false });
  const [chipsMore, setChipsMore] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const pathPending = useRef(false);
  const listId = useId();
  const input = useRef<HTMLInputElement>(null);
  const chips = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  // Two stops in visual order, search then the provider chips: Tab goes search -> chips and closes after
  // them; Shift+Tab the reverse.
  const stops = [
    { root: search, focus: () => input.current?.focus() },
    { root: chips, focus: () => chips.current?.querySelector<HTMLElement>('button[tabindex="0"]')?.focus() },
  ];
  const popover = usePopover({
    kind: "menu",
    root: panel,
    trigger,
    closing,
    stops,
    onClose,
  });

  useLayoutEffect(() => input.current?.focus(), []);
  useEffect(() => {
    for (const engine of engines) void load(engine.id);
  }, [engines, load]);

  const sections = useMemo(() => sectionsOf(engines, hosts), [engines, hosts]);
  const searching = query.trim() !== "";
  // Typing searches every provider, so All is the chip that shows as chosen then.
  const shownChip = searching ? ALL : chip;
  const allRows = useMemo<SectionRow[]>(
    () =>
      sections.flatMap((section) =>
        arrangeModels(section.engine.id, byEngine[section.engine.id]?.models ?? [], hidden, order).map((model) => ({
          row: { engine: section.engine.id, model },
          section,
        })),
      ),
    [sections, byEngine, hidden, order],
  );
  const rows = useMemo<SectionRow[]>(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (searching) {
      return allRows.filter(({ row }) =>
        words.every((word) => `${row.model.label} ${row.model.id} ${row.engine}`.toLowerCase().includes(word)));
    }
    return chip === ALL ? allRows : allRows.filter((entry) => entry.section.key === chip);
  }, [searching, query, allRows, chip]);
  // An engine whose host is not ready shows one state row in place of its models. None while searching: the rows are
  // not models, so there is nothing to find in them.
  const visible = useMemo(
    () =>
      sections
        .filter((section) => searching || chip === ALL || section.key === chip)
        .map((section) => ({
          section,
          rows: rows.filter((entry) => entry.section.key === section.key),
          state: searching ? null : sectionState(section),
        }))
        .filter((entry) => entry.rows.length > 0 || entry.state !== null),
    [sections, rows, searching, chip],
  );
  const ordered = visible.flatMap((entry) => (entry.state ? [] : entry.rows));
  const indexOf = new Map(ordered.map((entry, at) => [entry, at]));
  const ids = [ALL, ...sections.map((section) => section.key)];
  const pathSection = sections.find((section) => section.key === chip);
  const writePath = async (next: ClaudeEnginePath): Promise<boolean> => {
    if (next === path) return true;
    if (pathPending.current) return false;
    pathPending.current = true;
    setActionError(null);
    const result = await choose(next);
    pathPending.current = false;
    if (result.saved) return true;
    setActionError(`Nothing changed. ${result.error ?? "The Claude path setting is not available yet."}`);
    return false;
  };
  const pick = async (entry: SectionRow) => {
    if (entry.section.path && !(await writePath(entry.section.path))) return;
    popover.dismiss("select");
    onPick({ engine: entry.section.engine.id, model: entry.row.model.id });
  };
  const chooseChip = (id: string) => {
    setChip(id);
    // A chip applies to the whole list, so it clears what was typed.
    setQuery("");
    setActive(-1);
    list.current?.scrollTo?.({ top: 0 });
  };

  // ⌘ or Ctrl and a digit is the listed thread.jump shortcut, which the page behind a popover does not get.
  // Both the typed key and the physical key count: on some layouts (AZERTY) the digit row types symbols.
  const keepJumps = (event: React.KeyboardEvent) => {
    if ((event.metaKey || event.ctrlKey) && (/^[1-9]$/.test(event.key) || /^Digit[1-9]$/.test(event.code))) event.preventDefault();
  };
  const onKey = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((Math.max(active, step > 0 ? -1 : 0) + step + ordered.length) % Math.max(ordered.length, 1));
    } else if (event.key === "ArrowLeft" && !query) {
      event.preventDefault();
      chips.current?.querySelector<HTMLButtonElement>('button[tabindex="0"]')?.focus();
    } else if (event.key === "Enter") {
      event.preventDefault();
      const entry = ordered[active];
      if (entry) void pick(entry);
    }
  };

  // The fades say there is more: below at the top, above at the end, both between. Chips fade at the right.
  const measure = useCallback(() => {
    const element = list.current;
    if (element) {
      const above = element.scrollTop > 1;
      const below = element.scrollTop + element.clientHeight < element.scrollHeight - 1;
      setMore((current) => (current.above === above && current.below === below ? current : { above, below }));
    }
    const row = chips.current;
    if (row) setChipsMore(row.scrollLeft + row.clientWidth < row.scrollWidth - 1);
  }, []);
  useLayoutEffect(measure, [measure, rows.length, engines.length, byEngine]);
  useEffect(() => {
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    if (chips.current) observer.observe(chips.current);
    if (list.current) observer.observe(list.current);
    return () => observer.disconnect();
  }, [measure]);
  // The row the keyboard is on stays in view; the chosen model shows when it opens.
  useLayoutEffect(() => {
    const target = active >= 0 ? list.current?.querySelector(`[id="${listId}-${active}"]`) : null;
    target?.scrollIntoView?.({ block: "nearest" });
  }, [active, listId]);
  const opened = useRef(false);
  useLayoutEffect(() => {
    if (opened.current || ordered.length === 0) return;
    opened.current = true;
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest" });
  }, [ordered.length]);

  const failure = !searching && pathSection ? byEngine[pathSection.engine.id]?.error ?? null : null;
  const loading = engines.some((engine) => byEngine[engine.id] === undefined) && ordered.length === 0;
  const isSelected = (entry: SectionRow) =>
    entry.row.engine === selected.engine && entry.row.model.id === selected.model
    && (entry.section.path === null || entry.section.path === path);
  const engineName = (id: string) => engines.find((engine) => engine.id === id)?.name ?? id;
  // The two Claude paths carry a note and a state each, so their picker is wider, as the frames draw it.
  const claudePaths = sections.some((section) => section.path !== null);

  return (
    <FloatingPicker anchor={anchor} label="Choose a model" className={`model-picker${claudePaths ? " has-claude-paths" : ""}`} container={portalContainer} closing={closing} panelRef={panel} onKeyDown={keepJumps}>
      <div ref={search} className="model-search">
        <Search size={14} aria-hidden />
        <input
          ref={input}
          role="combobox"
          aria-label="Search models"
          aria-expanded="true"
          aria-autocomplete="list"
          aria-controls={listId}
          aria-activedescendant={active >= 0 && active < ordered.length ? `${listId}-${active}` : undefined}
          placeholder="Search models"
          value={query}
          onKeyDown={onKey}
          onChange={(event) => {
            setQuery(event.target.value);
            // A row stays highlighted, so Enter never picks one that is not.
            setActive(0);
          }}
        />
      </div>
      <div className="model-chips-row" data-more={chipsMore || undefined}>
        <div
          ref={chips}
          className="model-chips"
          role="toolbar"
          aria-label="Provider"
          onScroll={measure}
          onKeyDown={(event) => {
            const buttons = [...event.currentTarget.querySelectorAll<HTMLElement>("button")];
            const at = buttons.indexOf(document.activeElement as HTMLElement);
            if (at < 0) return;
            const next =
              event.key === "ArrowRight" ? (at + 1) % buttons.length
              : event.key === "ArrowLeft" ? (at - 1 + buttons.length) % buttons.length
              : event.key === "Home" ? 0
              : event.key === "End" ? buttons.length - 1
              : -1;
            if (next < 0) return;
            event.preventDefault();
            buttons[next]?.focus();
            const id = ids[next];
            if (id) chooseChip(id);
          }}
        >
          {ids.map((id) => {
            const chipEngine = id === ALL ? null : (sections.find((section) => section.key === id)?.engine ?? null);
            const label = id === ALL ? "All" : (sections.find((section) => section.key === id)?.name ?? engineName(id));
            return (
              <button
                key={id}
                type="button"
                className="model-chip"
                aria-label={isUnmanaged(chipEngine) ? `${label} (Unmanaged)` : undefined}
                aria-pressed={shownChip === id}
                aria-controls={listId}
                tabIndex={shownChip === id ? 0 : -1}
                onClick={() => {
                  chooseChip(id);
                  input.current?.focus();
                }}
              >
                {label}
                <UnmanagedLabel engine={chipEngine} variant="chip" />
              </button>
            );
          })}
        </div>
      </div>
      {actionError ? <p className="field-error model-path-error" role="alert">{actionError}</p> : null}
      <div className="model-list" data-above={more.above || undefined} data-below={more.below || undefined}>
        <div ref={list} id={listId} className="model-rows" role="listbox" aria-label="Models" onScroll={measure}>
          {visible.map(({ section, rows: groupRows, state }) => (
            <div key={section.key} role="group" aria-label={section.name}>
              {section.path ? (
                <button
                  type="button"
                  className="model-path-head"
                  aria-label={`${section.name} path`}
                  data-selected={section.path === path || undefined}
                  onClick={() => void writePath(section.path!)}
                >
                  <span className="model-path-name">{section.name}</span>
                  <StateWord word={engineWord(section.engine)} />
                  {section.path === "sdk" && section.host ? <HostStateChip host={section.host} /> : null}
                </button>
              ) : (
                <div className="model-heading" aria-hidden>
                  {section.name}
                  <UnmanagedLabel engine={section.engine} />
                </div>
              )}
              {section.note ? <div className="model-path-note">{section.note}</div> : null}
              {state ? (
                state.kind === "gap" ? (
                  <PickerGapRow gap={state.gap} host={state.host} showChip={!section.path} />
                ) : (
                  <div className="model-state-row" data-state={state.waiting ? "waiting" : "failed"}>
                    <span className="model-row-text">
                      <b>{state.title}</b>
                      <small>{state.sentence}</small>
                    </span>
                    <span className="model-state-actions">
                      <ClaudeHostActions host={state.host} onError={setActionError} />
                    </span>
                  </div>
                )
              ) : groupRows.map((entry) => {
                const at = indexOf.get(entry) ?? -1;
                const on = isSelected(entry);
                return (
                  <div
                    key={`${section.key}:${modelKey(entry.row.engine, entry.row.model.id)}`}
                    id={`${listId}-${at}`}
                    role="option"
                    aria-selected={on}
                    className="model-row"
                    data-active={at === active || undefined}
                    onPointerMove={() => setActive(at)}
                    onClick={() => void pick(entry)}
                  >
                    <span className="model-row-text">
                      <b>{entry.row.model.label}</b>
                      {entry.row.model.description ? <small>{entry.row.model.description}</small> : null}
                    </span>
                    {on ? <Check size={14} className="model-check" aria-hidden /> : null}
                  </div>
                );
              })}
            </div>
          ))}
          {ordered.length === 0 && !visible.some((entry) => entry.state) ? (
            <p className="model-empty" role="status">
              {loading ? "Loading models…" : failure ? failure : searching ? "No models found" : "No models listed"}
            </p>
          ) : null}
        </div>
      </div>
    </FloatingPicker>
  );
}

/** The closed picker in the composer: engine glyph and short model name. */
export function ModelPickerTrigger({
  engines,
  selected,
  disabled,
  triggerLabel,
  onPick,
}: {
  engines: EngineStatus[];
  selected: Selection;
  disabled?: boolean;
  triggerLabel?: string;
  onPick(selection: Selection): void;
}) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const models = useEngineModels(selected.engine);
  const model = resolveModel(models, selected.model);
  const engine = engines.find((entry) => entry.id === selected.engine);
  const button = useRef<HTMLButtonElement>(null);
  const close = useCallback(() => setAnchor(null), []);
  const name = triggerLabel ?? (model ? shortModelName(model.label) : (selected.model ?? engine?.name ?? "No engine"));

  useCommand("modelPicker.toggle", () => {
    if (button.current?.disabled || button.current?.closest("[data-side-chat]")) return false;
    button.current?.click();
  });

  return (
    <>
      <button
        ref={button}
        type="button"
        className="composer-control"
        aria-label={triggerLabel ?? model?.label ?? name}
        aria-haspopup="dialog"
        aria-expanded={anchor !== null}
        disabled={disabled}
        data-model-trigger
        onClick={(event) => setAnchor(anchor ? null : event.currentTarget.getBoundingClientRect())}
      >
        {engine ? <EngineGlyph name={engine.name} /> : null}
        <span>{name}</span>
        <UnmanagedLabel engine={engine} />
        {engines.length === 0 ? <ChevronDown size={12} aria-hidden /> : null}
      </button>
      <Presence>
      {anchor ? (
        <ModelPopover
          engines={engines}
          selected={{ engine: selected.engine, model: model?.id ?? selected.model }}
          anchor={anchor}
          portalContainer={button.current?.closest("dialog")}
          trigger={() => button.current}
          onPick={onPick}
          onClose={close}
        />
      ) : null}
      </Presence>
    </>
  );
}
