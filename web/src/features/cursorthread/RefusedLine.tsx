import { CircleAlert, X } from "lucide-react";
import { IconButton } from "../../ui/controls";

/** The red line above the composer when a run start is refused: the app's own failed-send line, with a Dismiss control. */
export function RefusedLine({ text, onDismiss }: { text: string; onDismiss(): void }) {
  return (
    <div className="ct-refused" role="alert">
      <CircleAlert size={16} aria-hidden />
      <span>{text}</span>
      <IconButton label="Dismiss error" onClick={onDismiss}>
        <X size={14} aria-hidden />
      </IconButton>
    </div>
  );
}
