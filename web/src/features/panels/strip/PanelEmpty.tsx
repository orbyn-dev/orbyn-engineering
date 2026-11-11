import { PanelRight } from "lucide-react";
import type { BuiltinTab, RightTab } from "../../../product/state/ui";
import { Button } from "../../../ui/controls";
import { tabOf } from "./tabs";

/** The three choices the empty panel offers. */
const CHOICES: readonly BuiltinTab[] = ["diff", "browser", "tools"];

/** A side panel with no tab open: what it is, how to add one, and the three most used choices. */
export function PanelEmpty({ onOpen, gitless = false }: { onOpen(tab: RightTab): void; gitless?: boolean }) {
  return (
    <div className="panel-empty">
      <PanelRight size={22} aria-hidden />
      <h4>Nothing open</h4>
      <span>Add a tab with +, or open a file from the left bar.</span>
      <div className="panel-empty-choices">
        {CHOICES.map((id) => {
          const { label, icon: Icon } = tabOf(id);
          return (
            <Button key={id} small icon={<Icon size={14} aria-hidden />} disabled={gitless && id === "diff"} onClick={() => onOpen(id)}>
              {label}
            </Button>
          );
        })}
      </div>
      {gitless ? <div className="xg-nogit">Not a Git repository.</div> : null}
    </div>
  );
}
