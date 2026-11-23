// The crew tools' answers and the disk guard's numbers. The service's refusal is structured: the
// code names the refusal and free_bytes / floor_gb carry what it measured. The sentence is written
// from those numbers, never read back out of the service's own sentence.
import type { ToolItem } from "../../product/timeline/types";

const GIB = 1024 ** 3;
const MIB = 1024 ** 2;
const START_TOOL = "crew_start";

/** The code `CrewError` carries when the disk guard refused a start. */
export const DISK_LOW = "disk_low";

/** The second line of a low-disk card: what did not happen. */
export const NO_WORKER_RAN = "No worker ran. Nothing was removed to make room.";

/** What a start refused for low disk measured, as JSON numbers; never a path. */
export interface DiskLow {
  free_bytes: number;
  floor_gb: number;
}

const record = (value: unknown): Record<string, unknown> | null =>
  value !== null && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : null;

const parse = (text: string | null | undefined): unknown => {
  if (typeof text !== "string" || text.trim() === "") return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
};

/** Free space as the Rust guard writes it: whole MiB below a GiB, else GiB to one decimal, rounded down. */
export function freeText(bytes: number): string {
  if (bytes < GIB) return `${Math.floor(bytes / MIB)} MB`;
  const tenths = Math.floor((bytes * 10) / GIB);
  return `${Math.floor(tenths / 10)}.${tenths % 10} GB`;
}

/** The floor is a whole number of GB. */
export const floorText = (gb: number): string => `${Math.floor(gb)} GB`;

/** The service's disk sentence, written from the numbers it sent. */
export function diskSentence(freeBytes: number, floorGb: number): string {
  return `Not started: ${freeText(freeBytes)} free, Crew needs ${floorText(floorGb)}. Free space and try again.`;
}

/**
 * A `CrewError` with `code: "disk_low"` and both JSON numbers, or null for every other answer. A
 * sentence is never parsed: a disk_low with an empty or different message still reads as disk_low.
 */
export function diskLowOf(value: unknown): DiskLow | null {
  const body = typeof value === "string" ? record(parse(value)) : record(value);
  if (!body || body.code !== DISK_LOW) return null;
  const free = body.free_bytes;
  const floor = body.floor_gb;
  if (typeof free !== "number" || !Number.isFinite(free) || free < 0) return null;
  if (typeof floor !== "number" || !Number.isFinite(floor) || floor < 0) return null;
  return { free_bytes: free, floor_gb: floor };
}

/** Whether an MCP tool name is `crew_start`, under the prefixes engines add. */
export function isCrewStartTool(name: string): boolean {
  return name.split(/__|[.:]/).filter(Boolean).pop() === START_TOOL;
}

/** The outcome a `crew_start` draft asked for, for the row's "Start task:" label. */
export function crewStartOutcome(args: string | null | undefined): string | null {
  const brief = record(record(parse(args))?.brief);
  const outcome = brief?.outcome;
  return typeof outcome === "string" && outcome.trim() ? outcome.trim() : null;
}

/** The disk refusal a `crew_start` call answered with, or null for another call or another answer. */
export function crewStartDiskLow(item: Pick<ToolItem, "name" | "result">): DiskLow | null {
  return isCrewStartTool(item.name) ? diskLowOf(item.result) : null;
}
