import { PanelRight } from "lucide-react";
import type { RightTab } from "../../../product/state/ui";
import { Button } from "../../../ui/controls";
import { tabOf } from "./tabs";

/** The three choices the empty panel offers. */
const CHOICES: readonly RightTab[] = ["diff", "browser", "tools"];

/** A side panel with no tab open: what it is, how to add one, and the three most used choices. */
export function PanelEmpty({ onOpen }: { onOpen(tab: RightTab): void }) {
  return (
    <div className="panel-empty">
      <PanelRight size={22} aria-hidden />
      <h4>Nothing open</h4>
      {/* Until the left-bar explorer ships (shell PR 1b) there is no file to open from the left bar; 1b switches this to the drawn line. */}
      <span>Add a tab with +.</span>
      <div className="panel-empty-choices">
        {CHOICES.map((id) => {
          const { label, icon: Icon } = tabOf(id);
          return (
            <Button key={id} small icon={<Icon size={14} aria-hidden />} onClick={() => onOpen(id)}>
              {label}
            </Button>
          );
        })}
      </div>
    </div>
  );
}
