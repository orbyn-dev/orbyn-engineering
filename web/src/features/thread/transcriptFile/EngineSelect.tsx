import { Check, ChevronDown } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { EngineStatus } from "../../../api/types";

/** The engines the project allows, the project's default marked, and one chosen for the imported thread. */
export function EngineSelect({
  engines,
  value,
  defaultEngine,
  note,
  disabled,
  onChange,
}: {
  engines: readonly EngineStatus[];
  value: string;
  defaultEngine: string | null;
  note: string | null;
  disabled: boolean;
  onChange(engine: string): void;
}) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const chosen = engines.find((engine) => engine.id === value);

  useEffect(() => {
    if (!open) return;
    const away = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node | null)) setOpen(false);
    };
    document.addEventListener("pointerdown", away, true);
    return () => document.removeEventListener("pointerdown", away, true);
  }, [open]);

  const close = () => {
    setOpen(false);
    button.current?.focus();
  };

  const key = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!open || event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    close();
  };

  return (
    <div className="transcript-engine" onKeyDown={key}>
      <div className="transcript-engine-row">
        <span className="transcript-label">Engine</span>
        <div ref={wrap} className="transcript-engine-picker">
          <button
            ref={button}
            type="button"
            className="transcript-engine-button"
            role="combobox"
            aria-label="Engine"
            aria-haspopup="listbox"
            aria-expanded={open}
            disabled={disabled}
            onClick={() => setOpen((current) => !current)}
          >
            <span>{chosen?.name ?? "Choose an engine"}</span>
            <ChevronDown size={14} aria-hidden />
          </button>
          {open ? (
            <div className="transcript-engine-menu" role="listbox" aria-label="Engine">
              {engines.map((engine) => (
                <button
                  key={engine.id}
                  type="button"
                  role="option"
                  aria-selected={engine.id === value}
                  onClick={() => {
                    onChange(engine.id);
                    close();
                  }}
                >
                  <span className="transcript-engine-check" aria-hidden>
                    {engine.id === value ? <Check size={14} /> : null}
                  </span>
                  <span className="transcript-engine-name">{engine.name}</span>
                  {engine.id === defaultEngine ? <span className="transcript-engine-default">Project default</span> : null}
                </button>
              ))}
            </div>
          ) : null}
        </div>
      </div>
      {note ? <p className="transcript-engine-note">{note}</p> : null}
    </div>
  );
}
