import { useEffect, useId, useLayoutEffect, useRef, type ReactNode } from "react";
import { useOpenState } from "./openState";

const DURATION_MS = 200;

/**
 * A 200 ms height disclosure driven by `--collapsible-panel-height`. Native
 * <details> can't animate on WebKit, which the desktop shell uses. The content
 * stays mounted while closed, so its state and scroll survive.
 */
export function Collapsible({
  summary,
  children,
  className,
  summaryClassName,
  panelClassName,
  defaultOpen = false,
  persistId,
  open: controlled,
  onOpenChange,
  ...rest
}: {
  summary: ReactNode;
  children: ReactNode;
  className?: string;
  summaryClassName?: string;
  panelClassName?: string;
  defaultOpen?: boolean;
  /** Remembers the open state under this id, per row and thread, while its row is out of the page. */
  persistId?: string;
  open?: boolean;
  onOpenChange?(open: boolean): void;
  "data-live"?: boolean;
}) {
  const [internal, setInternal] = useOpenState(persistId ?? null, defaultOpen);
  const open = controlled ?? internal;
  const panelId = useId();
  const panel = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const first = useRef(true);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useLayoutEffect(() => {
    const element = panel.current;
    if (!element) return;
    if (first.current) {
      first.current = false;
      element.hidden = !open;
      return;
    }
    clearTimeout(timer.current);
    element.hidden = false;
    const height = `${element.scrollHeight}px`;
    element.style.setProperty("--collapsible-panel-height", height);
    element.toggleAttribute("data-collapsed", open);
    void element.offsetHeight;
    element.toggleAttribute("data-collapsed", !open);
    timer.current = setTimeout(() => {
      element.style.removeProperty("--collapsible-panel-height");
      element.removeAttribute("data-collapsed");
      element.hidden = !open;
    }, DURATION_MS);
  }, [open]);

  useEffect(() => () => clearTimeout(timer.current), []);

  useEffect(() => {
    const element = root.current;
    const reveal = () => { setInternal(true); onOpenChange?.(true); };
    element?.addEventListener("reveal-setting", reveal);
    return () => element?.removeEventListener("reveal-setting", reveal);
  }, [onOpenChange, setInternal]);

  return (
    <div ref={root} className={className} {...rest}>
      <button
        type="button"
        className={summaryClassName}
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => {
          setInternal(!open);
          onOpenChange?.(!open);
        }}
      >
        {summary}
      </button>
      <div ref={panel} id={panelId} className={`collapsible-panel ${panelClassName ?? ""}`}>
        {children}
      </div>
    </div>
  );
}
