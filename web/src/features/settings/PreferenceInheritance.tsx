import { useId, useLayoutEffect, useRef, useState } from "react";
import type { PreferenceView } from "../../api/types";
import type { ScopedPreferenceMember } from "../../generated/ScopedPreferenceMember";
import type { ScopedPreferenceOverride } from "../../generated/ScopedPreferenceOverride";
import { Check, Layers } from "lucide-react";
import { Button, IconButton } from "../../ui/controls";
import { usePopover } from "../../ui/usePopover";
import type { PreferenceRow } from "./preferenceTypes";

function displayValue(row: PreferenceRow, value: unknown): string {
  const choice = row.options?.find((option) => Object.is(row.parse ? row.parse(option.value) : option.value, value));
  if (choice) return choice.label;
  if (typeof value === "boolean") return value ? "On" : "Off";
  if (value === null || value === undefined) return "Not set";
  if (value === "") return row.key === "default_model" || row.key === "default_engine" ? "Automatic" : "Empty";
  if (typeof value === "string" || typeof value === "number") return String(value);
  if (Array.isArray(value)) return `${value.length} ${value.length === 1 ? "item" : "items"}`;
  return "Custom";
}

/** A reference-style provenance popover, kept inside the owning modal's top layer. */
export function PreferenceInheritance({ row, view, global, members, overrides, mixed, disabled, onResetOverrides, onOpenOverride }: {
  row: PreferenceRow; view: PreferenceView; global: PreferenceView;
  members?: ScopedPreferenceMember[]; overrides?: ScopedPreferenceOverride[];
  mixed?: boolean; disabled?: boolean;
  onResetOverrides?(ids: string[]): void;
  onOpenOverride?(id: string): void;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const trigger = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const popover = usePopover({ kind: "fields", root: panel, active: open, trigger: () => trigger.current, onClose: () => setOpen(false) });
  const summary = mixed ? "Mixed" : view.source === "project" ? "Set for this project" : view.source === "global" ? "From this device" : view.source === "file" ? `From ${view.project_file?.path ?? "project file"}` : "Default";
  useLayoutEffect(() => {
    if (!open || !panel.current) return;
    const element = panel.current;
    const place = () => {
      const anchor = trigger.current?.getBoundingClientRect();
      if (!anchor) return;
      const bounds = element.getBoundingClientRect();
      element.style.left = `${Math.max(8, Math.min(anchor.left, window.innerWidth - bounds.width - 8))}px`;
      const below = anchor.bottom + 4;
      element.style.top = `${Math.max(8, Math.min(below + bounds.height <= window.innerHeight - 8 ? below : anchor.top - bounds.height - 4, window.innerHeight - bounds.height - 8))}px`;
    };
    place();
    element.focus({ preventScroll: true });
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => { window.removeEventListener("resize", place); window.removeEventListener("scroll", place, true); };
  }, [open]);
  const layersFor = (view: PreferenceView, environmentOnly: boolean) => [
    ...(!environmentOnly ? [{ label: "Project", value: view.source === "project" ? displayValue(row, view.value) : "Inherits", active: view.source === "project" }] : []),
    { label: "Environment", value: global.source === "global" ? displayValue(row, global.value) : "Inherits", active: view.source === "global" },
    ...(view.project_file ? [{ label: view.project_file.path, value: displayValue(row, view.project_file.value), active: view.source === "file" }] : []),
    { label: "Default", value: displayValue(row, view.default), active: view.source === "default" },
  ];
  const chains = members ? members.length ? members : [{ project_id: null, name: null, view: global }] : [{ project_id: "project", name: null, view }];
  return (
    <div>
      <div ref={trigger}>
        <IconButton label={summary} data-overridden={view.source === "project" || undefined} aria-expanded={open}
          aria-controls={open ? id : undefined} aria-haspopup="dialog" onClick={() => setOpen(!open)}>
          <Layers size={12} aria-hidden />
        </IconButton>
      </div>
      {open ? (
        <div ref={panel} id={id} className="setting-inheritance-popup" role="dialog" aria-label={row.title} tabIndex={-1}>
          {chains.map((chain) => <section key={chain.project_id ?? "environment"}>
            {chains.length > 1 ? <p>{chain.name}</p> : null}
            <ol>
              {layersFor(chain.view, chain.project_id === null).map((layer) => (
                <li key={layer.label} aria-current={layer.active || undefined}>
                  <span>{layer.label}</span>
                  <span className="setting-inheritance-value">{layer.value}{layer.active ? <Check size={12} aria-hidden /> : null}</span>
                </li>
              ))}
            </ol>
          </section>)}
          {overrides?.length ? <div className="setting-overrides">
            <div><span>Overridden by</span>
              <Button small tone="quiet" disabled={disabled} onClick={() => onResetOverrides?.(overrides.map((entry) => entry.project_id))}>
                Reset {overrides.length === 1 ? "it" : "all"}
              </Button>
            </div>
            {overrides.map((override) => <div key={override.project_id}>
              <Button small tone="quiet" onClick={() => { popover.dismiss("select"); onOpenOverride?.(override.project_id); }}>{override.name}</Button>
              <span>{displayValue(row, override.value)}</span>
            </div>)}
          </div> : null}
        </div>
      ) : null}
    </div>
  );
}
