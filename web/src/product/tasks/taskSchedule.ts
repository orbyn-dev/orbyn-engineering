import type { ScheduledTask } from "../../api/types";

export const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** "Daily at 09:00", "Every 30 min": what a task does, without its state. */
export function scheduleLabel(schedule: ScheduledTask["schedule"]): string {
  switch (schedule.kind) {
    case "interval": {
      const minutes = schedule.every_minutes;
      if (minutes === null) return "On an interval";
      return `Every ${minutes} min`;
    }
    case "fixed_time": {
      const time = schedule.time ?? "";
      const weekdays = schedule.weekdays ?? [];
      const days =
        weekdays.length === 0
          ? "Daily"
          : weekdays.length === 5 && weekdays.every((day) => day >= 1 && day <= 5)
            ? "Weekdays"
            : weekdays.map((day) => WEEKDAYS[day]).filter(Boolean).join(", ");
      return time ? `${days} at ${time}` : days;
    }
    case "webhook":
      return "On webhook";
    case "after_thread":
      return schedule.only_on_success ? "After a thread succeeds" : "After a thread";
    default:
      return schedule.kind;
  }
}

export type TaskTone = "idle" | "ok" | "fail";

/** `last_status` is "never", "started", or the reason the last firing failed. */
export function taskTone(task: Pick<ScheduledTask, "last_status">): TaskTone {
  if (task.last_status === "never") return "idle";
  if (task.last_status === "started") return "ok";
  return "fail";
}
