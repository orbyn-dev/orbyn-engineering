import { beforeEach, expect, it, vi } from "vitest";
import type { Thread } from "../../../api/types";
import * as transcriptExport from "../../../product/state/transcriptExport";
import { threadMenuItems } from "./threadMenu";

vi.mock("../../../product/state/transcriptExport", () => ({ exportThreadTranscript: vi.fn() }));

const thread: Thread = {
  id: "t-done",
  project_id: "p-app",
  title: "Explain how the event log resumes",
  engine: "claude",
  model: null,
  cwd: "/Users/dev/code/orbyn-engineering-app",
  worktree: null,
  skill_profile: null,
  active_run: null,
  last_run_state: "completed",
  created_at: "2026-11-01T00:00:00Z",
  updated_at: "2026-11-01T00:00:00Z",
  scratch: false,
  lifecycle: {
    pinned_at: null,
    archived_at: null,
    settled_at: null,
    snoozed_until: null,
    unread: false,
    auto_settle: null,
    position: 0,
    parent_thread_id: null,
    relation: null,
    permission_mode: "supervised",
    plan_mode: false,
  },
};

const items = () =>
  threadMenuItems({
    thread,
    branch: "main",
    projectName: "orbyn-engineering-app",
    filterActive: false,
    presets: [],
    onRename: () => undefined,
    onCustomSnooze: () => undefined,
    onDelete: () => undefined,
    onFilter: () => undefined,
  });

beforeEach(() => vi.mocked(transcriptExport.exportThreadTranscript).mockClear());

it("lists Export transcript after Copy and before Project settings, always enabled", () => {
  const all = items();
  const labels = all.map((item) => item.label);
  const copy = labels.indexOf("Copy");
  const exportTranscript = labels.indexOf("Export transcript");
  const settings = labels.indexOf("Project settings");
  expect(copy).toBeGreaterThanOrEqual(0);
  expect(exportTranscript).toBe(copy + 1);
  expect(settings).toBe(exportTranscript + 1);
  expect(all[exportTranscript]?.disabled).not.toBe(true);
});

it("exports the thread from the sidebar row menu", () => {
  const item = items().find((entry) => entry.label === "Export transcript");
  item?.onSelect?.();
  expect(transcriptExport.exportThreadTranscript).toHaveBeenCalledWith(thread);
});
