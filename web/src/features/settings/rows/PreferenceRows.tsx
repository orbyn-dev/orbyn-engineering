import type { ProjectScopeCatalog } from "../../../generated/ProjectScopeCatalog";
import { request } from "../../../api/client";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { ErrorState, Loading } from "../../../product/ui/states";
import { Collapsible } from "../../../ui/Collapsible";
import { ChevronRight } from "lucide-react";
import { useSettingsScope } from "./SettingsScopeContext";
import { ScopedRows } from "./ScopedPreferenceRows";
import type { PreferenceRow } from "./preferenceTypes";

/** Settings most people never need, closed until asked for. */
export function Advanced({ children }: { children: React.ReactNode }) {
  return (
    <Collapsible
      summaryClassName="settings-folded-head"
      summary={
        <>
          <ChevronRight size={14} className="chev" aria-hidden />
          <span>Advanced</span>
        </>
      }
    >
      {children}
    </Collapsible>
  );
}

/** Rows backed by the service's preferences. A failed save keeps the saved value. */
export function PreferenceGroups({
  groups,
  projectId,
}: {
  groups: { title: string; rows: PreferenceRow[] }[];
  /** Reads and writes what this project sets over the global value. */
  projectId?: string;
}) {
  const scope = useSettingsScope();
  if (projectId) return <ProjectPreferenceGroups key={projectId} groups={groups} projectId={projectId} />;
  const query = scope?.active ? scope.query : {};
  return <ScopedRows key={JSON.stringify(query)} groups={groups} query={query} />;
}

function ProjectPreferenceGroups({ groups, projectId }: {
  groups: { title: string; rows: PreferenceRow[] }[];
  projectId: string;
}) {
  const catalog = useAsync(`project-preference-scope:${projectId}`, (signal) =>
    request<ProjectScopeCatalog>("POST", "/api/settings/project-scopes", {}, signal), { keep: true });
  if (catalog.status === "loading") return <Loading label="Loading settings" rows={3} />;
  if (catalog.status === "error") return <ErrorState title="Couldn't load settings" failure={catalog.error} onRetry={catalog.reload} />;
  const group = catalog.data.groups.find((candidate) => candidate.members.some((member) => member.project_id === projectId));
  if (!group) return <ErrorState title="Couldn't load settings" failure={toFailure(new Error("This settings scope is unavailable."))} onRetry={catalog.reload} />;
  const query = { selection: { kind: "checkout" as const, group_key: group.key, project_id: projectId } };
  return <ScopedRows key={JSON.stringify(query)} groups={groups} query={query} inheritance />;
}
