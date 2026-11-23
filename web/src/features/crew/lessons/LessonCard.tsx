import { useEffect, useState, type ReactNode } from "react";
import { toFailure } from "../../../lib/useAsync";
import { navigate } from "../../../product/state/route";
import { ErrorState, Loading } from "../../../product/ui/states";
import { ConfirmDialog } from "../../../ui/Dialog";
import { Frame } from "../shared/Frame";
import { Ic } from "../shared/parts";
import { lessonsApi } from "./api";
import {
  BRIEF_LESSONS,
  EVERY_REPOSITORY,
  SCOPE_WORD,
  SOURCE_CHIP,
  SOURCE_WORD,
  briefsWord,
  editedWords,
  evidenceLine,
  happenedWords,
  moment,
  usedWords,
} from "./model";
import { takeAdded } from "./saved";
import type { Lesson, LessonDetail } from "./types";
import { UNDO_MS, showLessonUndo } from "./undo";
import type { LessonReader } from "./useLesson";
import { useLesson } from "./useLesson";
import {
  BackToLessons,
  FactsBox,
  FormFoot,
  Intro,
  LeftOutBanner,
  LessonField,
  LessonHead,
  OffBanner,
  PermissionsNote,
  PromoteRefused,
  Saved,
  SecretRefused,
  WriteFailure,
  saveFailureDetail,
} from "./LessonParts";
import "./lessons.css";

/** A write that did not happen, in its own banner: the headline, the detail, and the one action that could retry it. */
interface Failure {
  title: string;
  detail: string;
  retry?: () => void;
}

/**
 * One lesson's card (KC5, part B): the whole lesson as the read carries it, where it came from, how it is used, and the
 * person's own edit, turn off or on, promote, move back and delete. Every act here is a Person route: only a click of the
 * person's sends one. A delete leaves the card at once and the list draws its Undo toast; every other write keeps the card
 * and replaces the lesson with the service's answer.
 */
export function LessonCard({ id, repo, name }: { id: string; repo: string; name: string }) {
  const read = useLesson(repo, id);
  // A service without the lessons routes answers 404 not_found: the card draws nothing at all, as the list does.
  if (read.status === "missing") return null;
  return (
    <Frame sheet live>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <BackToLessons repo={repo} />
        <LessonHead title="Lesson" />
        {read.status === "loading" ? <Loading label="Reading the lesson" rows={3} /> : null}
        {read.status === "error" ? (
          <ErrorState title="Can’t read the lesson" failure={read.failure} onRetry={() => void read.reload()} />
        ) : null}
        {read.status === "ready" ? <Loaded read={read} repo={repo} name={name} /> : null}
        <PermissionsNote />
      </div>
    </Frame>
  );
}

/** The card once its read answered: the view, the edit form, and the two confirms. */
function Loaded({ read, repo, name }: { read: LessonReader & { status: "ready" }; repo: string; name: string }) {
  const detail = read.detail;
  const lesson = detail.lesson;
  const [editing, setEditing] = useState(false);
  // A save's line depends on the read's own answer: a lesson the next brief will not carry does not claim it will.
  const [saved, setSaved] = useState<{ kind: "added" | "edited"; inNextBrief: boolean } | null>(null);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [refused, setRefused] = useState(false);
  const [dialog, setDialog] = useState<"promote" | "move-back" | null>(null);
  const [busy, setBusy] = useState<"toggle" | "delete" | null>(null);
  // The Add page hands the card it just made over here, and the card says so once.
  useEffect(() => {
    if (takeAdded(lesson.id)) setSaved({ kind: "added", inNextBrief: lesson.in_next_brief });
  }, [lesson.id, lesson.in_next_brief]);

  const apply = (next: Lesson) => {
    read.setLesson(next);
    setRefused(false);
    setFailure(null);
  };
  const startEdit = () => {
    setSaved(null);
    setFailure(null);
    setRefused(false);
    setEditing(true);
  };
  const toggle = () => {
    if (busy) return;
    setBusy("toggle");
    setFailure(null);
    setSaved(null);
    void (lesson.state === "disabled" ? lessonsApi.enable(lesson.id, lesson.rev) : lessonsApi.disable(lesson.id, lesson.rev))
      .then(apply)
      .catch((error: unknown) => setFailure({ title: "Can’t save the lesson", detail: saveFailureDetail(toFailure(error)) }))
      .finally(() => setBusy(null));
  };
  const promote = () => {
    setFailure(null);
    // The contract refuses a promotion while the text names a path: the read already says so, so nothing is sent.
    if (lesson.names_path) {
      setRefused(true);
      return;
    }
    setDialog("promote");
  };
  const remove = () => {
    if (busy) return;
    setBusy("delete");
    setFailure(null);
    setSaved(null);
    void lessonsApi
      .remove(lesson.id, lesson.rev)
      .then((answer) => {
        const until = Date.parse(answer.undo_until);
        const deadline = Number.isNaN(until) ? Date.now() + UNDO_MS : until;
        showLessonUndo({ id: answer.id, until: deadline, seconds: Math.max(1, Math.ceil((deadline - Date.now()) / 1000)) });
        navigate({ name: "settings", section: "crew", crewPage: "lessons", repo });
      })
      .catch((error: unknown) => setFailure({ title: "Can’t delete the lesson", detail: toFailure(error).message }))
      .finally(() => setBusy(null));
  };

  if (editing) {
    return (
      <EditLesson
        detail={detail}
        repo={repo}
        name={name}
        onCancel={() => {
          setEditing(false);
          setFailure(null);
        }}
        onSaved={(next) => {
          apply(next);
          setEditing(false);
          setSaved({ kind: "edited", inNextBrief: next.in_next_brief });
        }}
      />
    );
  }
  return (
    <>
      {saved ? <Saved kind={saved.kind} inNextBrief={saved.inNextBrief} /> : null}
      {lesson.state === "disabled" ? <OffBanner /> : null}
      {lesson.left_out_of > 0 ? <LeftOutBanner count={lesson.left_out_of} /> : null}
      {refused ? <PromoteRefused onEdit={startEdit} /> : null}
      {failure ? <WriteFailure title={failure.title} detail={failure.detail} onRetry={failure.retry} /> : null}
      <TheLesson lesson={lesson} />
      <div className="cx-t13row">
        <button type="button" className="cx-ghost" disabled={busy !== null} onClick={startEdit}>
          <Ic n="edit" sm />
          Edit
        </button>
        <button
          type="button"
          className="cx-ghost"
          disabled={busy !== null}
          aria-busy={busy === "toggle" || undefined}
          onClick={toggle}
        >
          <Ic n={lesson.state === "disabled" ? "play" : "pause"} sm />
          {lesson.state === "disabled" ? "Turn on" : "Turn off"}
        </button>
        {lesson.scope === "repo" ? (
          <button type="button" className="cx-ghost" disabled={busy !== null} onClick={promote}>
            <Ic n="up" sm />
            Promote to all repositories
          </button>
        ) : (
          <button type="button" className="cx-ghost" disabled={busy !== null} onClick={() => setDialog("move-back")}>
            <Ic n="down" sm />
            Move back to one repository
          </button>
        )}
        <button type="button" className="cx-ghost" disabled={busy !== null} aria-busy={busy === "delete" || undefined} onClick={remove}>
          <Ic n="trash" sm />
          Delete
        </button>
      </div>
      <WhereItCameFrom detail={detail} repo={repo} name={name} />
      <HowItIsUsed lesson={lesson} />
      {dialog === "promote" ? (
        <PromoteConfirm
          detail={detail}
          name={name}
          onClose={() => setDialog(null)}
          onDone={apply}
        />
      ) : null}
      {dialog === "move-back" ? (
        <MoveBackConfirm detail={detail} repo={repo} name={name} onClose={() => setDialog(null)} onDone={apply} />
      ) : null}
    </>
  );
}

/** The lesson's whole text, with the Off chip when it is turned off. */
function TheLesson({ lesson }: { lesson: Lesson }) {
  return (
    <div className="cx-fbx">
      <h6>
        The lesson
        {lesson.state === "disabled" ? <span className="cx-apchip cx-dim" style={{ verticalAlign: "middle", marginLeft: "6px" }}>Off</span> : null}
      </h6>
      <p style={{ fontSize: "13.5px", lineHeight: 1.5, color: "var(--color-text)" }}>{lesson.text}</p>
    </div>
  );
}

/** The rows above a scope chip: "This repository" with the name, or "All repositories" with the drawn line. */
function AppliesTo({ lesson, repo, name }: { lesson: Lesson; repo: string; name: string }) {
  return (
    <>
      <span className="cx-apchip">{SCOPE_WORD[lesson.scope]}</span>{" "}
      <span style={{ color: "var(--color-textSoft)" }}>
        {lesson.scope === "repo" ? (lesson.repo === repo ? name : lesson.repo) : EVERY_REPOSITORY}
      </span>
    </>
  );
}

/**
 * Where the lesson came from, as the read carries it: its source, the evidence ids only (never their content), what its
 * kind of lesson means, when it was learned, where it applies and whether the person edited it.
 */
function WhereItCameFrom({ detail, repo, name }: { detail: LessonDetail; repo: string; name: string }) {
  const lesson = detail.lesson;
  const rows: (readonly [string, ReactNode])[] = [
    ["Learned from", <span className={SOURCE_CHIP[lesson.source]}>{SOURCE_WORD[lesson.source]}</span>],
  ];
  const evidence = evidenceLine(detail.evidence);
  // Evidence is ids only: a lesson with none (a person's correction, or one added by hand) has no Source row.
  if (evidence) rows.push(["Source", evidence]);
  rows.push(["What happened", happenedWords(lesson.source)]);
  rows.push(["Learned", moment(lesson.created_at)]);
  rows.push(["Applies to", <AppliesTo lesson={lesson} repo={repo} name={name} />]);
  rows.push(["Edited", editedWords(lesson)]);
  return <FactsBox title="Where it came from" rows={rows} column={112} />;
}

/**
 * A lesson's state as the card says it: off, on and in the next brief, or on and out of it for the read's own reason.
 * `in_next_brief` is the contract's answer, so the card never claims briefs a lesson the read says misses the next one.
 */
function stateWords(lesson: Lesson): string {
  if (lesson.state === "disabled") return "Off: stays here and does not go into a brief";
  if (lesson.in_next_brief) return "On: goes into briefs";
  if (lesson.hidden_by_cap) return "On, but over the cap: readable under Hidden, never in a brief";
  if (lesson.skill) return "On, but it became a skill: it leaves briefs while that skill exists";
  return "On, but not in the next brief right now: the repository’s switch, the newest five or the 1500-character cap.";
}

/**
 * How the lesson is used, in the design's words: the state it is in, the briefs it reached, the ones it was left out of,
 * what a worker gets, the two checks on the way in and at start, what never reads it, whether it helped (not measured yet)
 * and where it is kept.
 */
function HowItIsUsed({ lesson }: { lesson: Lesson }) {
  const rows: (readonly [string, ReactNode])[] = [
    ["State", stateWords(lesson)],
    ["Used", usedWords(lesson)],
  ];
  if (lesson.left_out_of > 0) rows.push(["Left out", `${briefsWord(lesson.left_out_of)}. That task started without it`]);
  rows.push([
    "Given to",
    lesson.in_next_brief
      ? `A worker, as text added to its brief after the route is chosen, among at most ${BRIEF_LESSONS} lessons. The text is marked as data and grants no permission`
      : `Not in the next brief, so no worker gets it yet. When one does, it is text added after the route is chosen, among at most ${BRIEF_LESSONS} lessons, marked as data and granting no permission`,
  ]);
  rows.push([
    "Checked",
    "For secrets when it is written, so a lesson with a secret is never saved. Against the answer key when a task starts: a lesson that matches the task’s checks is left out of that brief",
  ]);
  rows.push([
    "Never read by",
    "The router, the score, the verifier or the answer key. Lessons are added after routing, and Crew is built so a lesson cannot change them",
  ]);
  rows.push([
    "Did it help",
    "Not measured. Crew records an EvalHistory sample for each finished task with the lessons it carried; nothing reads these samples yet",
  ]);
  rows.push(["Kept", "On this device only. Nothing is written to your repository"]);
  return <FactsBox title="How a lesson is used" rows={rows} column={112} />;
}

/**
 * The edit form, on the card's own page: the text, the same 300-character count, and Save under it. Saving sends the
 * person's text with the revision they read; a secret the scan refuses keeps the text on the page, with the contract's
 * sentence. What the lesson was learned from and when stay as they are.
 */
function EditLesson({
  detail,
  repo,
  name,
  onCancel,
  onSaved,
}: {
  detail: LessonDetail;
  repo: string;
  name: string;
  onCancel(): void;
  onSaved(lesson: Lesson): void;
}) {
  const lesson = detail.lesson;
  const [text, setText] = useState(lesson.text);
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [refused, setRefused] = useState(false);
  const send = () => {
    if (saving) return;
    const next = text.trim();
    if (next === "" || next === lesson.text) return;
    setSaving(true);
    setFailure(null);
    setRefused(false);
    void lessonsApi
      .edit(lesson.id, { text: next, rev: lesson.rev })
      .then((answer) => onSaved(answer))
      .catch((error: unknown) => {
        const one = toFailure(error);
        setSaving(false);
        if (one.code === "secret_refused") setRefused(true);
        else setFailure({ title: "Can’t save the lesson", detail: saveFailureDetail(one), retry: send });
      });
  };
  return (
    <>
      {refused ? <SecretRefused /> : null}
      {failure ? <WriteFailure title={failure.title} detail={failure.detail} onRetry={failure.retry} /> : null}
      <Intro>Editing changes the lesson’s text. Saving runs the secret scan again. What it was learned from, and when, stay as they are.</Intro>
      <LessonField value={text} onChange={setText} invalid={refused} readOnly={saving} autoFocus />
      <FormFoot
        hint={saving ? "Saving to this device." : "Nothing is saved until you press Save."}
        busy={saving}
        onCancel={onCancel}
        onSave={send}
        saveLabel={saving ? "Saving…" : "Save"}
        saveDisabled={text.trim() === "" || text.trim() === lesson.text}
      />
      <WhereItCameFrom detail={detail} repo={repo} name={name} />
    </>
  );
}

/** "Promote to all repositories?" with what the lesson is and where it would apply. */
function PromoteConfirm({
  detail,
  name,
  onClose,
  onDone,
}: {
  detail: LessonDetail;
  name: string;
  onClose(): void;
  onDone(lesson: Lesson): void;
}) {
  const lesson = detail.lesson;
  return (
    <ConfirmDialog
      title="Promote to all repositories?"
      confirmLabel="Promote lesson"
      className="lessons-dialog"
      onClose={onClose}
      onConfirm={async () => {
        onDone(await lessonsApi.promote(lesson.id, lesson.rev));
      }}
    >
      <p>
        A lesson for all repositories goes into briefs in every repository on this device. Crew never promotes one by
        itself, and it refuses while the text names a path.
      </p>
      <FactsBox
        rows={[
          ["Lesson", lesson.text],
          ["Learned from", SOURCE_WORD[lesson.source]],
          ["Now applies to", `This repository (${name})`],
          ["Would apply to", "All repositories"],
        ]}
      />
    </ConfirmDialog>
  );
}

/** "Move back to one repository?" with what the lesson is and the repository it would apply to. */
function MoveBackConfirm({
  detail,
  repo,
  name,
  onClose,
  onDone,
}: {
  detail: LessonDetail;
  repo: string;
  name: string;
  onClose(): void;
  onDone(lesson: Lesson): void;
}) {
  const lesson = detail.lesson;
  return (
    <ConfirmDialog
      title="Move back to one repository?"
      confirmLabel="Move lesson back"
      className="lessons-dialog"
      onClose={onClose}
      onConfirm={async () => {
        onDone(await lessonsApi.demote(lesson.id, { repo, rev: lesson.rev }));
      }}
    >
      <p>
        The lesson goes into briefs for one repository only, instead of every repository on this device. Briefs in other
        repositories stop getting it. Crew never moves a lesson by itself.
      </p>
      <FactsBox
        rows={[
          ["Lesson", lesson.text],
          ["Learned from", SOURCE_WORD[lesson.source]],
          ["Now applies to", "All repositories"],
          ["Would apply to", `This repository (${name})`],
        ]}
      />
    </ConfirmDialog>
  );
}
