import { useAsync } from "../../../../lib/useAsync";
import { escapeApi } from "./api";
import type { EscapeState } from "../../../../generated/EscapeState";

/** One project's escape log, optionally of one state. A read, never a write; a retry reads again. */
export const useEscapeLog = (project: string, state?: EscapeState) =>
  useAsync(`crew-escapes:${project}:${state ?? "all"}`, (signal) => escapeApi.log(project, state, signal));

/** What Crew's scan found and nobody has decided. */
export const usePossible = (project: string) => useAsync(`crew-escapes-possible:${project}`, (signal) => escapeApi.possible(project, signal));

/** One escape and its trail. */
export const useEscapeDetail = (id: string) => useAsync(`crew-escape:${id}`, (signal) => escapeApi.detail(id, signal));

/** The constants the log reads, as stored on this device. No project: one document for the whole device. */
export const useEscapeThresholds = () => useAsync(`crew-escapes-thresholds`, (signal) => escapeApi.thresholds(signal));
