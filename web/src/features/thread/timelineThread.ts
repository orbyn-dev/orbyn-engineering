import { createContext, use } from "react";

/** Which thread the timeline below it belongs to. */
export const TimelineThread = createContext<string | null>(null);
export const useThreadOfTimeline = () => use(TimelineThread);
