import { Folder, MessageSquare } from "lucide-react";
import type { KeyboardEvent } from "react";
import { useUi, type LeftBarView } from "../../../product/state/ui";

/** Below this sidebar width the switch is two icons with the same two states. */
const NARROW_SWITCH = 204;

const VIEWS: { id: LeftBarView; label: string }[] = [
  { id: "threads", label: "Threads" },
  { id: "files", label: "Files" },
];

/** Threads | Files, at the top of the Projects section. The last choice is remembered for each person. */
export function LeftBarSwitch() {
  const view = useUi((state) => state.leftBarView);
  const narrow = useUi((state) => state.sidebarWidth < NARROW_SWITCH);
  const setPrefs = useUi((state) => state.setPrefs);
  const onKey = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    const next = VIEWS[(VIEWS.findIndex((one) => one.id === view) + 1) % VIEWS.length]!.id;
    setPrefs({ leftBarView: next });
    requestAnimationFrame(() => document.querySelector<HTMLElement>(`.lbsw [data-view="${next}"]`)?.focus());
  };
  return (
    <div className="lbsw" data-narrow={narrow || undefined} role="tablist" aria-label="Left bar view" onKeyDown={onKey}>
      {VIEWS.map(({ id, label }) => (
        <button
          key={id}
          type="button"
          className="lbt"
          role="tab"
          data-view={id}
          aria-selected={view === id}
          aria-label={label}
          title={label}
          tabIndex={view === id ? 0 : -1}
          onClick={() => setPrefs({ leftBarView: id })}
        >
          {narrow ? id === "threads" ? <MessageSquare size={14} aria-hidden /> : <Folder size={14} aria-hidden /> : label}
        </button>
      ))}
    </div>
  );
}
