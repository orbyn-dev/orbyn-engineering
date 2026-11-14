import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "../../../ui/controls";
import { UNAVAILABLE_REASON, unavailable } from "../autopilot/review/Parts";
import { KIND_ONE, type NeedsAction, type NeedsItem, type NeedsKind, type NeedsView } from "./model";
import "./needsyou.css";

const RISK_WORD = { high: "High", medium: "Medium", low: "Low" } as const;
const UNDO_WORD = { easy: "Easy to undo", hard: "Hard to undo" } as const;

/** What an item's action does, and why it cannot yet. With no `act`, every control is disabled with the one reason. */
export interface NeedsActions {
  act?(item: NeedsItem, action: NeedsAction): void;
  /** Why this action is off right now, or null when it is on. */
  off?(item: NeedsItem, action: NeedsAction): string | null;
}

function ActionButton({ item, action, handlers }: { item: NeedsItem; action: NeedsAction; handlers: NeedsActions }) {
  const reason = handlers.act ? (handlers.off?.(item, action) ?? null) : UNAVAILABLE_REASON;
  const off = !handlers.act || reason !== null;
  return (
    <Button
      small
      tone={action.primary ? "primary" : "quiet"}
      aria-label={`${action.label}: ${item.title}`}
      onClick={() => handlers.act?.(item, action)}
      {...(!handlers.act ? unavailable : reason ? { title: reason, "aria-description": reason } : {})}
      disabled={off}
    >
      {action.label}
    </Button>
  );
}

/** One item of the inbox as a row: rank, what it is, who asked, the risk in words, what waiting costs, and its actions. */
export function NeedsRow({ item, rank, handlers }: { item: NeedsItem; rank: number; handlers: NeedsActions }) {
  return (
    <li className="ny-item" data-kind={item.kind}>
      <span className="ny-rk" aria-hidden>{rank}</span>
      <div className="ny-tt">
        <b>{item.title}</b>
        <div className="ny-m">
          {item.risk ? <span className="ny-tag" data-tone={item.risk === "high" || item.overdue ? "amber" : item.risk === "low" ? "lo" : undefined}>Risk · {RISK_WORD[item.risk]}</span> : null}
          {item.undo ? <span className="ny-tag" data-tone="lo">{UNDO_WORD[item.undo]}</span> : null}
          <span>{item.from}</span>
        </div>
        {item.cost ? <div className="ny-cost">{item.cost}</div> : null}
      </div>
      <div className="ny-ac">
        {item.actions.map((action) => <ActionButton key={action.key} item={item} action={action} handlers={handlers} />)}
      </div>
    </li>
  );
}

/** The 320 px panel's card: kind, risk and reversibility, the title, who asked, what waiting costs, and the actions. */
export function NeedsCard({ item, handlers }: { item: NeedsItem; handlers: NeedsActions }) {
  const kind = [KIND_ONE[item.kind], item.risk ? RISK_WORD[item.risk] : null, item.undo === "hard" ? "hard to undo" : null].filter(Boolean).join(" · ");
  return (
    <li className="ny-card" data-kind={item.kind}>
      <span className="ny-k">{kind}</span>
      <span className="ny-t">{item.title}</span>
      <span className="ny-e">{item.engine ?? item.from}</span>
      {item.cost ? <span className="ny-ln">{item.cost}</span> : null}
      <div className="ny-ca">
        {item.actions.map((action) => <ActionButton key={action.key} item={item} action={action} handlers={handlers} />)}
      </div>
    </li>
  );
}

/** The width, in pixels, of the element: the inbox draws cards below `NARROW`. */
export const NARROW = 480;

export function useNarrow(): [React.RefObject<HTMLDivElement | null>, boolean] {
  const ref = useRef<HTMLDivElement>(null);
  const [narrow, setNarrow] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(([entry]) => {
      const width = entry?.contentRect.width ?? 0;
      if (width > 0) setNarrow(width < NARROW);
    });
    watch.observe(el);
    return () => watch.disconnect();
  }, []);
  return [ref, narrow];
}

export interface Chip<K extends string = string> {
  key: K;
  label: string;
  count: number;
}

/** The filter chips, and the sentence that says how the list is ranked. */
export function NeedsBar<K extends string>({ chips, active, onPick, rule, label = "Filter the inbox" }: { chips: Chip<K>[]; active: K; onPick?(next: K): void; rule?: string; label?: string }) {
  return (
    <div className="ny-bar" role="group" aria-label={label}>
      {chips.map((chip) => (
        <button key={chip.key} type="button" className="ny-fil" aria-pressed={active === chip.key} onClick={() => onPick?.(chip.key)} {...(onPick ? {} : unavailable)}>
          {chip.label} {chip.count}
        </button>
      ))}
      {rule ? <span className="ny-rule">{rule}</span> : null}
    </div>
  );
}

/** The sentence under every filter strip. */
export const RANKED_BY = "Ranked by risk, then by how hard it is to undo, then by what waiting costs";

/** "Needs you 4" and one chip for each kind that has an item. */
export function kindChips(view: NeedsView, total: number): Chip<NeedsKind | "all">[] {
  return [{ key: "all", label: "Needs you", count: total }, ...view.filters.map((one) => ({ key: one.kind, label: one.label, count: one.count }))];
}

/** The inbox's list, as rows or, in a narrow panel, as cards. */
export function NeedsList({ items, narrow, handlers, label = "Needs you" }: { items: NeedsItem[]; narrow?: boolean; handlers: NeedsActions; label?: string }) {
  return narrow ? (
    <ol className="ny-cards" aria-label={label}>
      {items.map((item) => <NeedsCard key={item.id} item={item} handlers={handlers} />)}
    </ol>
  ) : (
    <ol className="ny-list" aria-label={label}>
      {items.map((item, index) => <NeedsRow key={item.id} item={item} rank={index + 1} handlers={handlers} />)}
    </ol>
  );
}

/** Nothing waiting: the inbox says so and what Crew is doing instead. */
export function NeedsEmpty({ title, children, action }: { title: string; children: ReactNode; action?: ReactNode }) {
  return (
    <div className="ny-state" role="status">
      <b>{title}</b>
      <p>{children}</p>
      {action}
    </div>
  );
}

/** A source that could not be read: the other rows stay, and Try again reads again. With no `onRetry` (a frame) the button is off. */
export function SourceError({ sources, onRetry }: { sources: string[]; onRetry?(): void }) {
  return (
    <div className="ny-err" role="status">
      <span>Can’t read {sources.join(", ")}. The rest is shown.</span>
      <Button small onClick={onRetry} {...(onRetry ? {} : unavailable)}>Try again</Button>
    </div>
  );
}
