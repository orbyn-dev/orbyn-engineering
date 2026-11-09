import { Check, ChevronDown, Search } from "lucide-react";
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import type { EngineStatus } from "../../../api/types";
import { useCommand } from "../../../product/shell/shortcuts";
import { useUi } from "../../../product/state/ui";
import { EngineGlyph } from "../../../product/ui/badges";
import { closingProps, Presence } from "../../../ui/Presence";
import { usePopover } from "../../../ui/usePopover";
import {
  arrangeModels,
  modelKey,
  resolveModel,
  searchRows,
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

export function FloatingPanel({
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

/** The provider headings and rows, in display order; `index` is the row's place in the flat keyboard order. */
interface Group {
  engine: string;
  rows: { row: PickerRow; index: number }[];
}

function groupRows(rows: readonly PickerRow[], engines: readonly EngineStatus[]): Group[] {
  const groups: Group[] = [];
  let index = 0;
  for (const engine of engines) {
    const own = rows.filter((row) => row.engine === engine.id);
    if (own.length > 0) groups.push({ engine: engine.id, rows: own.map((row) => ({ row, index: index++ })) });
  }
  return groups;
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
  const [chip, setChip] = useState(ALL);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(-1);
  const [more, setMore] = useState({ above: false, below: false });
  const [chipsMore, setChipsMore] = useState(false);
  const listId = useId();
  const input = useRef<HTMLInputElement>(null);
  const chips = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  // Two stops in visual order, search then the provider chips: Tab goes search -> chips and closes after
  // them; Shift+Tab the reverse.
  const panes = [
    { root: search, focus: () => input.current?.focus() },
    { root: chips, focus: () => chips.current?.querySelector<HTMLElement>('button[tabindex="0"]')?.focus() },
  ];
  const popover = usePopover({
    kind: "menu",
    root: panel,
    trigger,
    closing,
    panes,
    onClose,
  });

  useLayoutEffect(() => input.current?.focus(), []);
  useEffect(() => {
    for (const engine of engines) void load(engine.id);
  }, [engines, load]);

  const searching = query.trim() !== "";
  // Typing searches every provider, so All is the chip that shows as chosen then.
  const shownChip = searching ? ALL : chip;
  const allRows = useMemo(
    () =>
      engines.flatMap((engine) =>
        arrangeModels(engine.id, byEngine[engine.id]?.models ?? [], hidden, order).map((model) => ({
          engine: engine.id,
          model,
        })),
      ),
    [engines, byEngine, hidden, order],
  );
  const rows = searching
    ? searchRows(allRows, query)
    : chip === ALL
      ? allRows
      : allRows.filter((row) => row.engine === chip);
  const groups = groupRows(rows, engines);
  const ordered = groups.flatMap((group) => group.rows.map((entry) => entry.row));
  const ids = [ALL, ...engines.map((engine) => engine.id)];
  const pick = (row: PickerRow) => {
    popover.dismiss("select");
    onPick({ engine: row.engine, model: row.model.id });
  };
  const choose = (id: string) => {
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
      const row = ordered[active];
      if (row) pick(row);
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

  const failure = chip !== ALL && !searching ? byEngine[chip]?.error : null;
  const loading = engines.some((engine) => byEngine[engine.id] === undefined) && ordered.length === 0;
  const isSelected = (row: PickerRow) => row.engine === selected.engine && row.model.id === selected.model;
  const engineName = (id: string) => engines.find((engine) => engine.id === id)?.name ?? id;

  return (
    <FloatingPanel anchor={anchor} label="Choose a model" className="model-picker" container={portalContainer} closing={closing} panelRef={panel} onKeyDown={keepJumps}>
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
            if (id) choose(id);
          }}
        >
          {ids.map((id) => (
            <button
              key={id}
              type="button"
              className="model-chip"
              aria-label={id !== ALL && engines.find((engine) => engine.id === id)?.capabilities.managed === false ? `${engineName(id)} (Unmanaged)` : undefined}
              aria-pressed={shownChip === id}
              aria-controls={listId}
              tabIndex={shownChip === id ? 0 : -1}
              onClick={() => {
                choose(id);
                input.current?.focus();
              }}
            >
              {id === ALL ? "All" : engineName(id)}
            </button>
          ))}
        </div>
      </div>
      <div className="model-list" data-above={more.above || undefined} data-below={more.below || undefined}>
        <div ref={list} id={listId} className="model-rows" role="listbox" aria-label="Models" onScroll={measure}>
          {groups.map((group) => (
            <div key={group.engine} role="group" aria-label={engineName(group.engine)}>
              <div className="model-heading" aria-hidden>
                {engineName(group.engine)}
              </div>
              {group.rows.map(({ row, index }) => {
                const on = isSelected(row);
                return (
                  <div
                    key={modelKey(row.engine, row.model.id)}
                    id={`${listId}-${index}`}
                    role="option"
                    aria-selected={on}
                    className="model-row"
                    data-active={index === active || undefined}
                    onPointerMove={() => setActive(index)}
                    onClick={() => pick(row)}
                  >
                    <span className="model-row-text">
                      <b>{row.model.label}</b>
                      {row.model.description ? <small>{row.model.description}</small> : null}
                    </span>
                    {on ? <Check size={14} className="model-check" aria-hidden /> : null}
                  </div>
                );
              })}
            </div>
          ))}
          {ordered.length === 0 ? (
            <p className="model-empty" role="status">
              {loading ? "Loading models…" : failure ? failure : searching ? "No models found" : "No models listed"}
            </p>
          ) : null}
        </div>
      </div>
    </FloatingPanel>
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
