import { PERIODS, type Period } from "./contract";

const KEY = "orbyn.crew-dashboard";
/** The project value kept for All projects. A project id never starts with `*`. */
export const ALL = "*";

export interface Choice {
  /** A project id, or `ALL`; `null` when none was kept. */
  project: string | null;
  days: Period | null;
}

/** The project and period a person last chose. Browser storage can be absent or hold anything, so a bad value is "none kept". */
export function readChoice(): Choice {
  try {
    const raw: unknown = JSON.parse(window.localStorage.getItem(KEY) ?? "null");
    if (typeof raw !== "object" || raw === null) return { project: null, days: null };
    const { project, days } = raw as Record<string, unknown>;
    return {
      project: typeof project === "string" && project.length > 0 && project.length <= 200 ? project : null,
      days: PERIODS.find((period) => period === days) ?? null,
    };
  } catch {
    return { project: null, days: null };
  }
}

export function writeChoice(choice: { project: string; days: Period }): void {
  try {
    window.localStorage.setItem(KEY, JSON.stringify(choice));
  } catch {
    /* storage is off or full: the choice is simply not remembered */
  }
}
