import { beforeEach, expect, it, vi } from "vitest";
import type { Project } from "../../../api/types";
import { setTranscriptPicker } from "../../../product/state/transcriptImport";
import { folderMenuItems } from "./folderMenu";
import type { ProjectFolder } from "./groups";

const project: Project = {
  id: "p-app",
  name: "orbyn-engineering-app",
  path: "/Users/dev/code/orbyn-engineering-app",
  trusted: true,
  is_git: true,
  managed: false,
  scratch: false,
  created_at: "2026-11-01T00:00:00Z",
};

const folder: ProjectFolder = { key: project.id, label: project.name, project, projects: [project], threads: [] };
const context = { projectGroupingOverrides: {}, projectGrouping: "separate" as const, setGroupingOverride: () => undefined };

beforeEach(() => setTranscriptPicker(null));

it("lists Import transcript… after New thread and before the rest of the folder menu", () => {
  const labels = folderMenuItems(folder, context).map((item) => item.label);
  expect(labels.slice(0, 4)).toEqual(["New thread", "Import transcript…", "Copy path", "Project settings"]);
});

it("opens the transcript picker for the folder's project", () => {
  const pick = vi.fn();
  setTranscriptPicker(pick);
  const item = folderMenuItems(folder, context).find((entry) => entry.label === "Import transcript…");
  item?.onSelect?.();
  expect(pick).toHaveBeenCalledWith(project);
});
