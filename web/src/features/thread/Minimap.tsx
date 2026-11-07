import { useEffect, useState, type RefObject } from "react";

export interface MinimapItem {
  key: string;
  text: string;
  /** Your message (ink), a failure (red) or a decision waiting on you (amber). */
  kind?: "you" | "failed" | "ask";
}

const SPACING = 8;
const MIN_ITEMS = 2;

/** One tick per prompt beside the conversation. Hover previews; a click goes there. */
export function Minimap({
  items,
  scroller,
  locate,
  goTo,
  revision,
}: {
  items: readonly MinimapItem[];
  scroller: RefObject<HTMLDivElement | null>;
  /** Where an item's row starts in the scrolled content, whether or not it is mounted. */
  locate(key: string): number | null;
  /** Scrolls to an item's row, mounting it first when it is out of view. */
  goTo(key: string): void;
  /** Changes when row heights are corrected, so the current tick is read again. */
  revision: number;
}) {
  const [active, setActive] = useState<string | null>(null);
  const [hover, setHover] = useState<MinimapItem | null>(null);
  useEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const update = () => {
      const top = element.scrollTop + element.clientHeight / 3;
      let current: string | null = null;
      for (const item of items) {
        if (item.kind === "failed" || item.kind === "ask") continue;
        const at = locate(item.key);
        if (at !== null && at <= top) current = item.key;
      }
      setActive(current);
    };
    update();
    element.addEventListener("scroll", update, { passive: true });
    return () => element.removeEventListener("scroll", update);
  }, [items, scroller, locate, revision]);
  if (items.filter((item) => item.kind !== "failed" && item.kind !== "ask").length < MIN_ITEMS) return null;
  // Hidden until the pointer reaches the edge, so a short thread shows no stray ticks.
  return (
    <div className="minimap-zone" onWheel={(event) => scroller.current?.scrollBy(0, event.deltaY)}>
      <nav
        className="minimap"
        aria-label="Prompts in this thread"
        style={{ height: `min(${(items.length - 1) * SPACING}px, calc(100vh - 18rem))` }}
      >
        {items.map((item, at) =>
          item.kind === "failed" || item.kind === "ask" ? (
            <span
              key={item.key}
              className="minimap-mark"
              data-kind={item.kind}
              style={{ top: `${items.length > 1 ? (at / (items.length - 1)) * 100 : 0}%` }}
              aria-hidden
            />
          ) : (
          <button
            key={item.key}
            type="button"
            aria-label={`Go to prompt: ${item.text.slice(0, 80)}`}
            aria-current={item.key === active || undefined}
            style={{ top: `${items.length > 1 ? (at / (items.length - 1)) * 100 : 0}%` }}
            onPointerEnter={() => setHover(item)}
            onPointerLeave={() => setHover(null)}
            onClick={() => goTo(item.key)}
          />
          ),
        )}
        {hover ? <p className="minimap-preview">{hover.text.slice(0, 160)}</p> : null}
      </nav>
    </div>
  );
}
