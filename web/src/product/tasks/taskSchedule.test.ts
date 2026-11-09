import { expect, it } from "vitest";
import type { ScheduledTask } from "../api/types";
import { scheduleLabel, taskTone } from "./taskSchedule";

const task = (over: Partial<ScheduledTask> = {}): ScheduledTask => ({
  id: "task-1",
  project_id: "p-app",
  thread_id: "t-approval",
  name: "Daily review",
  prompt: "Review the open changes.",
  engine: "claude",
  schedule: {
    kind: "interval",
    every_minutes: 60,
    time: null,
    weekdays: [],
    utc_offset_minutes: 0,
    only_on_success: false,
  },
  enabled: true,
  signature: null,
  has_secret: false,
  filters: [],
  dedupe: null,
  max_age_seconds: null,
  timestamp_path: null,
  next_run_at: null,
  last_run_at: null,
  last_status: "never",
  created_at: "",
  ...over,
});

it("names each schedule kind in words", () => {
  expect(scheduleLabel(task().schedule)).toBe("Every 60 min");
  expect(scheduleLabel({ ...task().schedule, kind: "fixed_time", time: "09:00", weekdays: [] })).toBe("Daily at 09:00");
  expect(scheduleLabel({ ...task().schedule, kind: "fixed_time", time: "09:00", weekdays: [1, 2, 3, 4, 5] })).toBe("Weekdays at 09:00");
  expect(scheduleLabel({ ...task().schedule, kind: "fixed_time", time: "09:00", weekdays: [1, 3] })).toBe("Mon, Wed at 09:00");
  expect(scheduleLabel({ ...task().schedule, kind: "webhook" })).toBe("On webhook");
  expect(scheduleLabel({ ...task().schedule, kind: "after_thread", only_on_success: false })).toBe("After a thread");
  expect(scheduleLabel({ ...task().schedule, kind: "after_thread", only_on_success: true })).toBe("After a thread succeeds");
});

it("maps the stored status to the row tone", () => {
  expect(taskTone({ last_status: "never" })).toBe("idle");
  expect(taskTone({ last_status: "started" })).toBe("ok");
  expect(taskTone({ last_status: "The engine exited with status 1." })).toBe("fail");
});
