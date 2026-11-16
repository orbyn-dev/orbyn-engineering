import { Ellipsis } from "lucide-react";
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { Menu } from "../../../../ui/Menu";
import { Presence } from "../../../../ui/Presence";
import "./flow-live.css";

interface Tab {
  id: string;
  label: string;
}

/** The room the ⋯ takes at the end of the strip, and the strip's side padding, in px. */
const MORE = 34;
const PAD = 40;
const GAP = 2;

/**
 * The Crew view's tab list with the ⋯ overflow rule (Crew Designs, crew-tabs-overflow-menu): the tabs that fit show, the active one
 * is always among them, and the ⋯ at the end holds the rest. The app measures the real widths, so what shows depends on the font
 * and the width. Choosing a tab from the menu selects it and it takes a visible slot.
 */
export function CrewTabs<T extends Tab>({ tabs, active, onSelect, base }: { tabs: readonly T[]; active: T["id"]; onSelect(id: T["id"]): void; base: string }) {
  const strip = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const more = useRef<HTMLButtonElement>(null);
  const widths = useRef<Record<string, number>>({});
  const [room, setRoom] = useState(Infinity);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);

  const measure = useCallback(() => {
    const element = strip.current;
    if (!element) return;
    for (const tab of element.querySelectorAll<HTMLElement>('[role="tab"]')) widths.current[tab.dataset.tab ?? ""] = tab.offsetWidth;
    // No layout (a test environment, a hidden window): nothing is measured, so nothing overflows.
    setRoom(element.clientWidth || Infinity);
  }, []);
  useLayoutEffect(() => {
    measure();
    const element = strip.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(measure);
    watch.observe(element);
    return () => watch.disconnect();
  }, [measure, tabs]);

  const width = (id: string) => widths.current[id] ?? 0;
  const all = tabs.reduce((sum, tab) => sum + width(tab.id) + GAP, 0);
  const shown = new Set<string>(tabs.map((tab) => tab.id));
  if (all > room - PAD) {
    // The active tab first, then the others in their order, while they fit beside the ⋯.
    shown.clear();
    shown.add(active);
    let used = width(active) + GAP + MORE;
    for (const tab of tabs) {
      if (shown.has(tab.id) || used + width(tab.id) + GAP > room - PAD) continue;
      shown.add(tab.id);
      used += width(tab.id) + GAP;
    }
  }
  const hidden = tabs.filter((tab) => !shown.has(tab.id));
  const visible = tabs.filter((tab) => shown.has(tab.id));

  // The ⋯ may disappear once a tab is chosen from its menu: focus goes to the chosen tab, never to the page.
  const chooseFromMenu = (id: T["id"]) => {
    onSelect(id);
    setTimeout(() => document.getElementById(`${base}-${id}`)?.focus(), 0);
  };

  const keys = (event: React.KeyboardEvent<HTMLElement>) => {
    const at = visible.findIndex((tab) => `${base}-${tab.id}` === (document.activeElement as HTMLElement | null)?.id);
    if (at < 0) return;
    const next = event.key === "ArrowRight" ? (at + 1) % visible.length : event.key === "ArrowLeft" ? (at - 1 + visible.length) % visible.length : event.key === "Home" ? 0 : event.key === "End" ? visible.length - 1 : null;
    if (next === null) return;
    event.preventDefault();
    const target = visible[next]!;
    document.getElementById(`${base}-${target.id}`)?.focus();
    onSelect(target.id);
  };

  return (
    <div ref={strip} className="ap-tabs">
      <div ref={list} className="fv-tablist" role="tablist" aria-label="Crew" onKeyDown={keys}>
        {tabs.map((tab) => {
          const off = !shown.has(tab.id);
          return (
            <button
              key={tab.id}
              type="button"
              role="tab"
              id={`${base}-${tab.id}`}
              data-tab={tab.id}
              aria-controls={`${base}-panel`}
              aria-selected={active === tab.id}
              tabIndex={active === tab.id && !off ? 0 : -1}
              className={off ? "fv-tab-off" : undefined}
              aria-hidden={off || undefined}
              onClick={() => onSelect(tab.id)}
            >
              {tab.label}
            </button>
          );
        })}
      </div>
      {hidden.length > 0 ? (
        <button
          ref={more}
          type="button"
          className="fv-more-views"
          aria-label="More Crew views"
          aria-haspopup="menu"
          aria-expanded={menu !== null}
          onClick={() => {
            const box = more.current?.getBoundingClientRect();
            setMenu(menu ? null : { x: box?.left ?? 0, y: (box?.bottom ?? 0) + 4 });
          }}
        >
          <Ellipsis size={14} aria-hidden />
        </button>
      ) : null}
      <Presence>
        {menu && hidden.length > 0 ? (
          <Menu x={menu.x} y={menu.y} label="More Crew views" items={hidden.map((tab) => ({ label: tab.label, onSelect: () => chooseFromMenu(tab.id) }))} onClose={() => setMenu(null)} />
        ) : null}
      </Presence>
    </div>
  );
}
