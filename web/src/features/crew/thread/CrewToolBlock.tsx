import { diskSentence, type DiskLow } from "../tool";
import "./crewTool.css";

/**
 * A crew start the service refused for low disk. Its sentence is written from `free_bytes` and
 * `floor_gb`; the service's own message is never read here. The row above carries the "Not started"
 * chip and this block carries the sentence, as the lead thread's frame draws them.
 */
export function CrewToolBlock({ refusal }: { refusal: DiskLow }) {
  return (
    <div className="crew-tool" data-code="disk_low">
      <p className="crew-tool-line">{diskSentence(refusal.free_bytes, refusal.floor_gb)}</p>
    </div>
  );
}
