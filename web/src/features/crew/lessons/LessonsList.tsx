import { useState } from "react";
import { NETWORK_ERROR, type ApiFailure } from "../../../api/client";
import type { Project } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { Button } from "../../../ui/controls";
import { navigate } from "../../../product/state/route";
import type { Lesson, LessonFilter } from "./types";
import { dayOf } from "../autopilot/approvals/ledger";
import { ProjectChip } from "../escape/live/parts";
import { Frame } from "../shared/Frame";
import { Ic } from "../shared/parts";
import { lessonsApi } from "./api";
import { useLessons, type LessonsReady } from "./useLessons";
import { Loading } from "../../../product/ui/states";
import "./lessons.css";

/** The two caps the contract fixes: 50 lessons per repository and 20 for all of them. */
const REPO_CAP = 50;
const GLOBAL_CAP = 20;
/** The brief cap the contract fixes, drawn while the list is still being read (the read carries the live values). */
const DEFAULT_BRIEF_CAP = { lessons: 5, chars: 1500 } as const;

const SOURCE_WORD: Record<Lesson["source"], string> = {
  fingerprint: "Repeated failure",
  escape: "Escape",
  revert: "Revert",
  person: "Your correction",
};

const SCOPE_WORD: Record<Lesson["scope"], string> = { repo: "This repository", global: "All repositories" };

/**
 * Crew Designs, the lessons list (KC5): the repository's introduction, the "data, not rules" banner, the
 * per-repository switch, the filter chips and the two groups (this repository, then all repositories). A read
 * only: this screen sends no write but the switch, and only a person's own click sends that. Edit, disable and
 * delete sit on a lesson's own card (part B); every row here only opens it.
 */
export function LessonsList({
  project,
  name,
  projects,
  onPick,
}: {
  project: string;
  name: string;
  projects: readonly Project[];
  onPick(id: string): void;
}) {
  const [filter, setFilter] = useState<LessonFilter>("all");
  const read = useLessons(project, filter);
  // A service without the lessons routes answers 404 not_found: nothing at all is drawn, not an empty list.
  if (read.status === "missing") return null;
  const cap = read.status === "ready" ? read.list.brief_cap : DEFAULT_BRIEF_CAP;
  return (
    <Frame live>
      <div className="cx-cwpg">
        <div className="cx-t14pg">
          <div className="cx-t10hd">
            <b>Lessons</b>
            <span className="cx-cwpv">Preview</span>
            <span className="cx-rtag cx-lo">This device only</span>
            <span className="cx-t10sp">
              <ProjectChip name={name} projects={projects} current={project} onPick={onPick} />
            </span>
          </div>
          <p className="cx-lsintro">
            What Crew has learned on this device. A lesson comes from a failure that repeated, an escape or a revert
            you confirmed, or something you typed as a correction, or you add one by hand. You can read, edit, turn off,
            promote, move back and delete each one.
          </p>
          <div className="cx-cwbn cx-lsbanner">
            <b>Lessons are data, not rules.</b>
            <span>
              A worker gets at most {cap.lessons} of them as text added to its brief after the route is chosen. A lesson
              that matches a task’s checks is left out of that brief. Crew is built so the router, the score, the
              verifier, the answer key, a lease, a budget and the checks never read one.
            </span>
          </div>
          {read.status === "loading" ? <Loading label="Reading lessons" rows={3} /> : null}
          {read.status === "error" ? <ReadError failure={read.failure} onRetry={() => void read.reload()} /> : null}
          {read.status === "ready" ? (
            <Ready read={read} project={project} name={name} filter={filter} onFilter={setFilter} />
          ) : null}
        </div>
      </div>
    </Frame>
  );
}

/** The one red banner of an unreadable list: nothing was changed here, and the person may try again or copy the error. */
function ReadError({ failure, onRetry }: { failure: ApiFailure; onRetry: () => void }) {
  const detail = failure.code === NETWORK_ERROR ? failure.message : `${failure.code}: ${failure.message}`;
  const copy = () => {
    try {
      void navigator.clipboard?.writeText(detail).catch(() => undefined);
    } catch {
      /* no clipboard here: the text stays visible below */
    }
  };
  return (
    <div className="cx-cwbn cx-red" style={{ margin: "0" }} role="alert">
      <Ic n="alert" sm />
      <span>
        <b>Can’t read the lessons</b>
        <span style={{ display: "block" }}>Nothing was changed. {detail}</span>
      </span>
      <span className="cx-acts">
        <button type="button" className="cx-ghost" onClick={copy}>
          Copy the error
        </button>
        <button type="button" className="cx-btn" onClick={onRetry}>
          Try again
        </button>
      </span>
    </div>
  );
}

/** The switch's own row: the click sends the one PUT, the switch is busy while it does, and a failure keeps the old value. */
function SwitchRow({ read, project, name }: { read: LessonsReady; project: string; name: string }) {
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<ApiFailure | null>(null);
  const list = read.list;
  const save = async (use_lessons: boolean) => {
    setSaving(true);
    setFailure(null);
    try {
      const answer = await lessonsApi.setUse({ repo: project, use_lessons });
      read.setList({ ...list, use_lessons: answer.use_lessons });
    } catch (error) {
      // Nothing is flipped until the service answers, so a refusal leaves the value the read carried.
      setFailure(toFailure(error));
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="cx-setrow">
      <div>
        <b>Use lessons in {name}</b>
        <small>
          When it is on, a brief for this repository gets its active lessons first, then the ones for all repositories,
          newest first, at most {list.brief_cap.lessons} and {list.brief_cap.chars.toLocaleString("en-US")} characters.
          Turning it off stops both from reaching its briefs; Crew keeps learning and writing lessons here.
        </small>
        {failure ? (
          <small role="alert" style={{ color: "var(--color-danger)" }}>
            Can’t save the setting: {failure.message}
          </small>
        ) : null}
      </div>
      <label className="cx-toggle">
        <input
          type="checkbox"
          role="switch"
          aria-label={`Use lessons in ${name}`}
          checked={list.use_lessons}
          disabled={saving}
          aria-busy={saving || undefined}
          onChange={(event) => void save(event.target.checked)}
        />
        <span>{saving ? "Saving…" : list.use_lessons ? "On" : "Off"}</span>
      </label>
    </div>
  );
}

/** One lesson: its text, where it came from, when it was learned, what it reached, and the state it is in. */
function LessonRow({ lesson, project, hidden }: { lesson: Lesson; project: string; hidden: boolean }) {
  const used = lesson.last_used_at
    ? `Used in ${lesson.uses} ${lesson.uses === 1 ? "brief" : "briefs"} · last ${dayOf(lesson.last_used_at)}`
    : "Not used yet";
  return (
    <div className="cx-lsrow">
      <div className="cx-lsmain">
        <b>{lesson.text}</b>
        <div className="cx-lsmeta">
          <span>{SOURCE_WORD[lesson.source]}</span>
          <span>·</span>
          <span>Learned {dayOf(lesson.created_at)}</span>
          <span>·</span>
          <span>{used}</span>
        </div>
        <div className="cx-lstags">
          <span className="cx-apchip">{SCOPE_WORD[lesson.scope]}</span>
          {lesson.state === "disabled" ? <span className="cx-apchip cx-dim">Off</span> : null}
          {hidden ? <small className="cx-lsnote">Hidden: over the limit, never used.</small> : null}
        </div>
      </div>
      <Button
        small
        onClick={() => navigate({ name: "settings", section: "crew", crewPage: "lesson", id: lesson.id, repo: project })}
      >
        Open
      </Button>
    </div>
  );
}

/** One scope's group, with its count out of the contract's cap and its own sentence about where it goes. */
function Group({
  title,
  sub,
  cap,
  lessons,
  project,
  hidden,
}: {
  title: string;
  sub: string;
  cap: number;
  lessons: readonly Lesson[];
  project: string;
  hidden: boolean;
}) {
  return (
    <div className="cx-lsgroup">
      <h5 className="cx-t10h5">
        {title}
        <span className="cx-lscount">
          {lessons.length} of {cap}
        </span>
      </h5>
      <p className="cx-lssub">{sub}</p>
      {lessons.length > 0 ? (
        <div className="cx-lsrows">
          {lessons.map((lesson) => (
            <LessonRow key={lesson.id} lesson={lesson} project={project} hidden={hidden} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

/** The list and the controls that change it, once the read answered. The switch never hides a row: off keeps every one readable. */
function Ready({
  read,
  project,
  name,
  filter,
  onFilter,
}: {
  read: LessonsReady;
  project: string;
  name: string;
  filter: LessonFilter;
  onFilter(filter: LessonFilter): void;
}) {
  const list = read.list;
  const repoLessons = list.lessons.filter((lesson) => lesson.scope === "repo");
  const globalLessons = list.lessons.filter((lesson) => lesson.scope === "global");
  const add = () => navigate({ name: "settings", section: "crew", crewPage: "lesson-add", repo: project });
  return (
    <>
      <SwitchRow read={read} project={project} name={name} />
      <div className="cx-lsfilters">
        <button
          type="button"
          aria-pressed={filter === "all"}
          className={filter === "all" ? "cx-lschip cx-on" : "cx-lschip"}
          onClick={() => onFilter("all")}
        >
          All lessons
        </button>
        <button
          type="button"
          aria-pressed={filter === "hidden"}
          className={filter === "hidden" ? "cx-lschip cx-on" : "cx-lschip"}
          onClick={() => onFilter("hidden")}
        >
          Hidden (over 50) · {list.counts.hidden}
        </button>
        <span className="cx-lsadd">
          <Button tone="primary" onClick={add}>
            Add a lesson
          </Button>
        </span>
      </div>
      {list.lessons.length === 0 ? (
        filter === "hidden" ? (
          <div className="cx-cwstate" style={{ margin: "32px auto 0" }}>
            <b>No hidden lessons</b>
          </div>
        ) : (
          <div className="cx-cwstate" style={{ margin: "32px auto 0" }}>
            <b>Nothing learned yet</b>
            <p>
              Nothing has been learned on this device yet. A lesson is written by a failure that repeats (2 tasks of
              one repository within 60 days), an escape or a revert you confirm, or a correction you type. You can also
              add one by hand.
            </p>
          </div>
        )
      ) : (
        <>
          <Group
            title={`This repository · ${name}`}
            sub={`Go into briefs for work in ${name} only. A lesson here may name a path of the repository.`}
            cap={REPO_CAP}
            lessons={repoLessons}
            project={project}
            hidden={filter === "hidden"}
          />
          <Group
            title="All repositories"
            sub="Go into briefs for work in every repository on this device. A lesson here names path classes, never a path."
            cap={GLOBAL_CAP}
            lessons={globalLessons}
            project={project}
            hidden={filter === "hidden"}
          />
        </>
      )}
    </>
  );
}
