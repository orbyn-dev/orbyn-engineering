import { Copy, Layers, Settings, SquarePen } from "lucide-react";
import type { ProjectGroupingMode } from "../../../generated/ProjectGroupingMode";
import { navigate } from "../../../state/route";
import type { MenuItem } from "../../../ui/Menu";
import { copyText } from "./threadActions";
import type { ProjectFolder } from "./groups";

const GROUPING_LABELS: Record<ProjectGroupingMode, string> = {
  repository: "Repository",
  repository_path: "Repository path",
  separate: "Separate",
};

/** What the folder menu reads from the sidebar: the grouping preferences and the way to change one. */
export interface GroupingContext {
  projectGroupingOverrides: Record<string, ProjectGroupingMode>;
  projectGrouping: ProjectGroupingMode;
  setGroupingOverride(projectId: string, mode: ProjectGroupingMode | null): void;
}

export function folderMenuItems(
  folder: ProjectFolder,
  { projectGroupingOverrides, projectGrouping, setGroupingOverride }: GroupingContext,
): MenuItem[] {
  const project = folder.project;
  const groupingItems: MenuItem[] = folder.projects.map((member) => ({
    label: member.name,
    children: [
      {
        label: "Default grouping",
        selected: projectGroupingOverrides[member.id] === undefined,
        onSelect: () => setGroupingOverride(member.id, null),
      },
      ...(Object.entries(GROUPING_LABELS) as [ProjectGroupingMode, string][]).map(([mode, label]) => ({
        label,
        selected: (projectGroupingOverrides[member.id] ?? projectGrouping) === mode,
        onSelect: () => setGroupingOverride(member.id, mode),
      })),
    ],
  }));
  const overridden = folder.projects.some((member) => projectGroupingOverrides[member.id]);
  return [
    {
      label: "New thread",
      icon: <SquarePen size={14} aria-hidden />,
      onSelect: () => navigate({ name: "new", projectId: project.id }),
    },
    {
      label: "Copy path",
      icon: <Copy size={14} aria-hidden />,
      onSelect: () => void copyText(project.path, "Copied path"),
    },
    {
      label: "Project settings",
      icon: <Settings size={14} aria-hidden />,
      onSelect: () => navigate({ name: "settings", section: "projects" }),
    },
    ...(folder.projects.length > 1 || overridden
      ? [
          {
            label: "Grouping",
            icon: <Layers size={14} aria-hidden />,
            separatorBefore: true,
            children: groupingItems,
          },
        ]
      : []),
  ];
}
