import { describeFrames } from "../shared/test-support";
import { SETTINGS_FRAMES } from "./names";

/** What each frame must show: text taken from its design. */
const SHOWS: Record<string, string[]> = {
  "settings-crew": ["Default budget for a new crew", "High 4 hours · Medium 24 hours · Low no deadline", "Exchanges before the lead is told", "Open in orbyn-engineering"],
};

describeFrames("Settings › Crew frame (Crew Designs v23)", SETTINGS_FRAMES, SHOWS);
