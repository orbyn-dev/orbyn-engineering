import { ArrowRight, ChevronDown, GitBranch, RefreshCw, SlidersHorizontal } from "lucide-react";
import { useState, type ReactNode } from "react";
import { defaultPrefs, useUi } from "../../../product/state/ui";
import { IconButton } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";

export type DiffSource = "worktree" | "turn" | "thread" | "branch" | "commit";

export const DIFF_SOURCES: { id: DiffSource; label: string }[] = [
  { id: "worktree", label: "Working tree" },
  { id: "turn", label: "Turn" },
  { id: "thread", label: "Thread" },
  { id: "branch", label: "Branch" },
  { id: "commit", label: "Commit" },
];

/** The least width the panel needs for a split view, and for the file list to stand beside the diff. */
export const WIDE = 640;

const count = (value: number) => value.toLocaleString("en-US");

/** A button that opens a menu under it, closing and returning focus as every menu does. */
export function MenuButton({ items, menuLabel, width, align = "start", disabled, children }: {
  items: MenuItem[];
  menuLabel: string;
  /** The menu's width in the design. */
  width?: number;
  align?: "start" | "end";
  disabled?: boolean;
  children(props: { onClick(event: React.MouseEvent<HTMLElement>): void; "aria-expanded": boolean; "aria-haspopup": "menu"; "aria-disabled"?: true }): ReactNode;
}) {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <>
      {children({
        "aria-haspopup": "menu",
        "aria-expanded": at !== null,
        ...(disabled ? { "aria-disabled": true as const } : {}),
        onClick: (event) => {
          if (disabled) return;
          const rect = event.currentTarget.getBoundingClientRect();
          setAt(at ? null : { x: align === "end" ? rect.right - (width ?? 160) : rect.left, y: rect.bottom + 4 });
        },
      })}
      <Presence>{at ? <Menu x={at.x} y={at.y} label={menuLabel} items={items} minWidth={width} onClose={() => setAt(null)} /> : null}</Presence>
    </>
  );
}

/** The scope menu: what is being compared. */
function Scope({ source, onPick }: { source: DiffSource; onPick(source: DiffSource): void }) {
  const label = DIFF_SOURCES.find((entry) => entry.id === source)?.label ?? "";
  const items: MenuItem[] = DIFF_SOURCES.map((entry) => ({ label: entry.label, selected: entry.id === source, onSelect: () => onPick(entry.id) }));
  return (
    <MenuButton items={items} menuLabel="Scope">
      {(props) => (
        <button type="button" className="scope" aria-label={`Scope: ${label}`} {...props}>
          {label}
          <ChevronDown size={11} aria-hidden />
        </button>
      )}
    </MenuButton>
  );
}

/** One menu for how the diff is drawn. The button is lit when anything differs from the defaults. */
function ViewOptions({ wide }: { wide: boolean }) {
  const layout = useUi((state) => state.diffLayout);
  const ignoreSpace = useUi((state) => state.diffIgnoreWhitespace);
  const hideTests = useUi((state) => state.diffHideTests);
  const collapsed = useUi((state) => state.diffFilesCollapsed);
  const wrap = useUi((state) => state.diffWrapLines);
  const setPrefs = useUi((state) => state.setPrefs);
  const changed = layout !== defaultPrefs.diffLayout || ignoreSpace !== defaultPrefs.diffIgnoreWhitespace
    || hideTests !== defaultPrefs.diffHideTests || collapsed !== defaultPrefs.diffFilesCollapsed || wrap !== defaultPrefs.diffWrapLines;
  const toggle = (label: string, on: boolean, change: () => void): MenuItem => ({ label, selected: on, keepOpen: true, leadCheck: true, onSelect: change });
  const items: MenuItem[] = [
    { ...toggle("Unified", layout === "stacked" || !wide, () => setPrefs({ diffLayout: "stacked" })), radio: true },
    {
      ...toggle("Split", layout === "split" && wide, () => setPrefs({ diffLayout: "split" })),
      radio: true,
      disabled: !wide,
      ...(wide ? {} : { title: `Needs about ${WIDE} px` }),
    },
    { ...toggle("Ignore whitespace", ignoreSpace, () => setPrefs({ diffIgnoreWhitespace: !ignoreSpace })), separatorBefore: true },
    toggle("Hide test files", hideTests, () => setPrefs({ diffHideTests: !hideTests })),
    toggle("Collapse all files", collapsed, () => setPrefs({ diffFilesCollapsed: !collapsed })),
    toggle("Wrap long lines", wrap, () => setPrefs({ diffWrapLines: !wrap })),
  ];
  return (
    <MenuButton items={items} menuLabel="View options" align="end" width={220}>
      {(props) => (
        <IconButton label="View options" className={changed ? "on" : undefined} {...props}>
          <SlidersHorizontal size={14} aria-hidden />
        </IconButton>
      )}
    </MenuButton>
  );
}

export interface ComparePair {
  base: string;
  head: string | null;
  bases: string[];
  /** Only the Branch scope has a base to pick. */
  enabled: boolean;
  onBase(name: string): void;
  /** For the Turn and Commit scopes: the pair opens the list of turns or commits instead of a base menu. */
  onOpen?(): void;
}

/** “main → origin/main”: the base and the branch compared with it, as one button that opens both choices. */
function Pair({ pair }: { pair: ComparePair }) {
  const items: MenuItem[] = [
    ...pair.bases.map((name, at) => ({
      label: name,
      icon: <GitBranch size={14} aria-hidden />,
      selected: name === pair.base,
      ...(at === 0 ? { heading: "Base" } : {}),
      onSelect: () => pair.onBase(name),
    })),
    ...(pair.head
      ? [{ label: pair.head, icon: <GitBranch size={14} aria-hidden />, selected: true, shortcut: "this branch", separatorBefore: true, heading: "Compare with" } satisfies MenuItem]
      : []),
  ];
  const name = pair.head ? `Compare ${pair.base} to ${pair.head}` : `Compare ${pair.base}`;
  const contents = (
    <>
      <span className="mono">{pair.base}</span>
      {pair.head ? (
        <>
          <ArrowRight size={11} aria-hidden />
          <span className="mono">{pair.head}</span>
        </>
      ) : null}
      <ChevronDown size={11} aria-hidden />
    </>
  );
  if (pair.onOpen) {
    return (
      <button type="button" className="pair" aria-label={name} aria-haspopup="menu" onClick={pair.onOpen}>
        {contents}
      </button>
    );
  }
  return (
    <MenuButton items={items} menuLabel="Compare" width={260} disabled={!pair.enabled}>
      {(props) => (
        <button type="button" className="pair" aria-label={name} title={pair.enabled ? undefined : "Compare applies to the Branch scope"} {...props}>
          {contents}
        </button>
      )}
    </MenuButton>
  );
}

/** The header: scope and totals on one row, the compared pair beside them when there is room and under them when not. */
export function ChangesHeader({ source, onSource, total, wide, pair, refreshing, onRefresh }: {
  source: DiffSource;
  onSource(source: DiffSource): void;
  /** `null` while nothing is known: a placeholder is drawn. `undefined` leaves the total out, as an error does. */
  total: { added: number; removed: number } | { conflicts: number } | null | undefined;
  wide: boolean;
  pair: ComparePair | null;
  refreshing: boolean;
  onRefresh(): void;
}) {
  const pairNode = pair ? <Pair pair={pair} /> : null;
  return (
    <div className="ch2">
      <div className="chrow">
        <Scope source={source} onPick={onSource} />
        {total === null ? (
          <span className="num x2n" aria-hidden><i className="x2pill" /></span>
        ) : total && "conflicts" in total ? (
          <span className="num x2n">{total.conflicts === 1 ? "1 conflict" : `${total.conflicts} conflicts`}</span>
        ) : total ? (
          <span className="num x2n">
            +{count(total.added)} <span className="r">−{count(total.removed)}</span>
          </span>
        ) : null}
        {wide ? pairNode : null}
        <span className="x2sp" />
        <ViewOptions wide={wide} />
        <IconButton label="Refresh" onClick={onRefresh} disabled={refreshing}>
          <RefreshCw size={14} className={refreshing ? "spin" : undefined} aria-hidden />
        </IconButton>
      </div>
      {!wide && pairNode ? <div className="chrow">{pairNode}</div> : null}
    </div>
  );
}
