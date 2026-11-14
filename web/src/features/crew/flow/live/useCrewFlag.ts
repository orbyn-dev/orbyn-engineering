import { useConnection } from "../../../../product/state/connection";

/**
 * Whether the service runs with Crew Preview on, from the info the app already reads at start (`crew_preview`, present only when
 * on). Shell surfaces use this, never a Crew route: with Preview off a Crew route answers 404, which the browser logs as an error.
 * False until the info says so, so a read that is loading or failed shows nothing.
 */
export const useCrewFlag = (): boolean => useConnection((state) => state.crewPreview);
