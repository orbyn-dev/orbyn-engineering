import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { toFailure } from "../../../lib/useAsync";
import { Empty, ErrorState, Loading } from "../../../product/ui/states";
import { useVisibleProjects, useWorkspace } from "../../../product/state/workspace";
import { navigate } from "../../../product/state/route";
import { useCrewPreview } from "../autopilot/approvals/useCrew";
import { Frame } from "../shared/Frame";
import { lessonsApi } from "./api";
import { useLessons } from "./useLessons";
import { MAX_TEXT } from "./model";
import { markAdded } from "./saved";
import type { LessonScope } from "./types";
import {
  BackToLessons,
  FormFoot,
  Intro,
  LessonField,
  LessonHead,
  PermissionsNote,
  SecretRefused,
  WriteFailure,
  saveFailureDetail,
} from "./LessonParts";
import "./lessons.css";

/** The page card these screens draw in, so a state before a project is known is not flush to the corner. */
function Card({ children }: { children: ReactNode }) {
  return <Frame sheet live>{children}</Frame>;
}

/**
 * Add a lesson by hand (KC5, part B): the person's own words, up to 300 characters, for this repository or for all of them.
 * Nothing is drawn, and no lessons route is read, while Crew Preview is off or the route names a repository the project
 * list does not show (the contract answers 404 for an unknown repository); with no project at all it draws the empty state.
 * A lessons read that answers 404 not_found (a service without the lessons routes) draws nothing either.
 * Saving is a Person write and only the person's click sends it; a saved lesson opens its card, which says "Saved." once.
 */
export function LessonAddPage({ repo }: { repo?: string }) {
  const on = useCrewPreview();
  const projects = useVisibleProjects();
  const failure = useWorkspace((state) => state.projects.error);
  const refresh = useWorkspace((state) => state.refreshProjects);
  useEffect(() => {
    if (on) void refresh();
  }, [on, refresh]);
  if (!on) return null;
  if (projects === null)
    return (
      <Card>
        {failure ? <ErrorState title="Can’t read the projects" failure={failure} onRetry={() => void refresh()} /> : <Loading label="Reading projects" rows={2} />}
      </Card>
    );
  // A named repository must be one the person sees; only an address that names none falls back to the first project.
  const named = repo !== undefined && repo !== "";
  const project = named ? projects.find((entry) => entry.id === repo) : projects[0];
  if (!project) return named ? null : <Card><Empty title="No project to show" /></Card>;
  return <GatedAdd key={project.id} repo={project.id} name={project.name} />;
}

/** The form is drawn once the repository's lessons read answered 200: a service without the lessons routes answers 404, and nothing is drawn. */
function GatedAdd({ repo, name }: { repo: string; name: string }) {
  const read = useLessons(repo, "all");
  if (read.status === "missing") return null;
  if (read.status === "loading") return <Card><Loading label="Reading lessons" rows={2} /></Card>;
  if (read.status === "error")
    return <Card><ErrorState title="Can’t read the lessons" failure={read.failure} onRetry={() => void read.reload()} /></Card>;
  return <AddLesson repo={repo} name={name} />;
}

/** The one scope choice a hand-written lesson makes: this repository, or every repository on this device. */
const SCOPES: readonly (readonly [LessonScope, string])[] = [
  ["repo", "This repository"],
  ["global", "All repositories"],
];

/** The form: the text, the scope, and Save under it, with the scan's refusals in place. */
function AddLesson({ repo, name }: { repo: string; name: string }) {
  const [text, setText] = useState("");
  const [scope, setScope] = useState<LessonScope>("repo");
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [refused, setRefused] = useState(false);
  // A save the person left the page during still completes; it just does not take them anywhere afterwards.
  const active = useRef(true);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const radios = useRef<(HTMLButtonElement | null)[]>([]);
  // One tab stop for the scope: the arrow keys move the choice and the focus, as a native radio group does.
  const arrow = (event: KeyboardEvent) => {
    const step = event.key === "ArrowDown" || event.key === "ArrowRight" ? 1 : event.key === "ArrowUp" || event.key === "ArrowLeft" ? -1 : 0;
    if (step === 0 || saving) return;
    event.preventDefault();
    const at = SCOPES.findIndex(([value]) => value === scope);
    const to = (at + step + SCOPES.length) % SCOPES.length;
    setScope(SCOPES[to]![0]);
    radios.current[to]?.focus();
  };
  const save = () => {
    if (saving) return;
    const next = text.trim();
    if (next === "" || next.length > MAX_TEXT) return;
    setSaving(true);
    setFailure(null);
    setRefused(false);
    void lessonsApi
      .add({ scope, ...(scope === "repo" ? { repo } : {}), text: next })
      .then((lesson) => {
        if (!active.current) return;
        markAdded(lesson.id);
        navigate({ name: "settings", section: "crew", crewPage: "lesson", id: lesson.id, repo });
      })
      .catch((error: unknown) => {
        if (!active.current) return;
        const one = toFailure(error);
        setSaving(false);
        if (one.code === "secret_refused") setRefused(true);
        else setFailure(saveFailureDetail(one));
      });
  };
  return (
    <Frame sheet live>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <BackToLessons repo={repo} />
        <LessonHead title="Add a lesson" />
        {refused ? <SecretRefused /> : null}
        {failure ? <WriteFailure title="Can’t save the lesson" detail={failure} onRetry={save} /> : null}
        <Intro>
          Write a lesson in your own words, up to 300 characters. It is saved on this device, and a worker gets it as text in
          its brief. Saving scans it for secrets: a lesson that holds one is not saved.
        </Intro>
        <LessonField value={text} onChange={setText} invalid={refused} readOnly={saving} autoFocus />
        <div className="cx-t12fl">
          <span className="cx-t12lb">Applies to</span>
          <span className="cx-segx" role="radiogroup" aria-label="Applies to" style={{ alignSelf: "flex-start" }} onKeyDown={arrow}>
            {SCOPES.map(([value, label], index) => (
              <button
                key={value}
                ref={(node) => {
                  radios.current[index] = node;
                }}
                type="button"
                tabIndex={scope === value ? 0 : -1}
                role="radio"
                aria-checked={scope === value}
                className={scope === value ? "cx-on" : undefined}
                disabled={saving}
                onClick={() => setScope(value)}
              >
                {label}
              </button>
            ))}
          </span>
          <span className="cx-t12lb">
            This repository is {name}, and its lesson may name a path. All repositories names path classes only, never a path.
          </span>
        </div>
        <FormFoot
          hint={saving ? "Saving to this device." : "Nothing is saved until you press Save."}
          busy={saving}
          onCancel={() => navigate({ name: "settings", section: "crew", crewPage: "lessons", repo })}
          onSave={save}
          saveLabel={saving ? "Saving…" : "Save"}
          saveDisabled={text.trim() === "" || text.trim().length > MAX_TEXT}
        />
        <PermissionsNote />
      </div>
    </Frame>
  );
}
