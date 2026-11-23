import { useEffect, useState } from "react";
import { navigate, routeHref } from "../../../../product/state/route";
import { useUi } from "../../../../product/state/ui";
import { ConfirmDialog } from "../../../../ui/Dialog";
import { Ic } from "../../shared/parts";
import type { Lesson } from "../types";
import { useLessons } from "../useLessons";
import { skillProposalsApi } from "./api";
import { takeSkillAdded } from "./marks";
import {
  ALL_PROJECTS,
  AVAILABLE,
  DEFAULT_SELECTION_NOTE,
  DISMISS_NOTE,
  DISMISSED_TOAST,
  NOTHING_ADDED,
  SCAN_NOTE,
  SKILLS_LOCAL,
  TEMPLATE_NOTE,
  activeLessons,
  basedOnWords,
  firstLines,
  lessonsOnly,
  lessonsWord,
  moment,
  proposalName,
  skillText,
  sourceLessons,
  staleWords,
} from "./model";
import type { SkillProposal } from "./types";
import { openProposals, useProposals } from "./useProposals";
import "./proposals.css";

/** The source chip's word, as the lessons list names the same source. */
const SOURCE_WORD: Record<Lesson["source"], string> = {
  fingerprint: "Repeated failure",
  escape: "Confirmed escape",
  revert: "Revert",
  person: "Your correction",
};

/**
 * The skill proposals of one repository, on the Lessons page (KC9): drawn above the lessons groups, always present with its
 * count once the service answers the proposals read, a "No proposals" box when there are none, and one card per open
 * proposal. Approve asks first and writes the proposed text as it stands; Dismiss asks and takes the proposal away. Both are
 * Person routes, so only the person's own click sends one. A service without the routes answers 404 and the section is not
 * drawn at all, as the whole lessons page is not then.
 */
export function ProposalsSection({
  project,
  name,
  lessons: pageLessons,
  onLessonsChanged,
}: {
  project: string;
  name: string;
  /** The lessons list the page already read, under the All filter; absent when the page shows the Hidden filter. */
  lessons?: readonly Lesson[];
  /** The page's own lessons are read again after an approval, because its lessons now carry "Now a skill". */
  onLessonsChanged?: () => void;
}) {
  const proposals = useProposals(project);
  const [added, setAdded] = useState<{ name: string; lessons: number } | null>(null);
  // The review page leaves after a write and hands the skill it added over here, to be said once.
  useEffect(() => {
    const justAdded = takeSkillAdded();
    if (justAdded) setAdded(justAdded);
  }, []);
  // Under the Hidden filter the page's list is not the whole repository: the section reads the whole one itself.
  if (pageLessons === undefined) return <OwnLessons project={project} name={name} proposals={proposals} added={added} onSetAdded={setAdded} onLessonsChanged={onLessonsChanged} />;
  if (proposals.status === "missing") return null;
  if (proposals.status !== "ready") return null;
  return (
    <SectionBody
      project={project}
      name={name}
      proposals={proposals}
      rows={pageLessons}
      added={added}
      onSetAdded={setAdded}
      onLessonsChanged={onLessonsChanged}
    />
  );
}

/** The section on the Hidden filter: the whole repository's lessons are read here, so a proposal's counts are right. */
function OwnLessons({
  project,
  name,
  proposals,
  added,
  onSetAdded,
  onLessonsChanged,
}: {
  project: string;
  name: string;
  proposals: ReturnType<typeof useProposals>;
  added: { name: string; lessons: number } | null;
  onSetAdded(mark: { name: string; lessons: number } | null): void;
  onLessonsChanged?: () => void;
}) {
  const lessons = useLessons(project, "all");
  if (proposals.status === "missing" || lessons.status === "missing") return null;
  if (proposals.status !== "ready" || lessons.status !== "ready") return null;
  return (
    <SectionBody
      project={project}
      name={name}
      proposals={proposals}
      rows={lessons.list.lessons}
      added={added}
      onSetAdded={onSetAdded}
      onLessonsChanged={onLessonsChanged}
    />
  );
}

/** The section once both reads answered: the added line, the heading, the cards and the two confirms. */
function SectionBody({
  project,
  name,
  proposals,
  rows,
  added,
  onSetAdded,
  onLessonsChanged,
}: {
  project: string;
  name: string;
  proposals: Extract<ReturnType<typeof useProposals>, { status: "ready" }>;
  rows: readonly Lesson[];
  added: { name: string; lessons: number } | null;
  onSetAdded(mark: { name: string; lessons: number } | null): void;
  onLessonsChanged?: () => void;
}) {
  const [asked, setAsked] = useState<{ kind: "approve" | "dismiss"; id: string } | null>(null);
  const open = openProposals(proposals.list.proposals);
  // The confirm always holds the proposal as the last read has it, so a retry after a refused write carries the new `rev`;
  // a proposal that is no longer open closes the confirm.
  const live = asked ? open.find((one) => one.id === asked.id) : undefined;
  const confirming = asked && live ? { kind: asked.kind, proposal: live } : null;
  const setConfirming = (next: { kind: "approve" | "dismiss"; proposal: SkillProposal } | null) =>
    setAsked(next ? { kind: next.kind, id: next.proposal.id } : null);
  const approve = async (proposal: SkillProposal) => {
    try {
      const answer = await skillProposalsApi.approve(proposal.id, { text: proposal.text, rev: proposal.rev });
      onSetAdded({ name: answer.skill?.name ?? skillText(proposal.text).name ?? proposalName(proposal), lessons: answer.lessons.length });
      await proposals.reload();
      onLessonsChanged?.();
    } catch (failure) {
      // The confirm stays open with the service's own words; a stale answer is read again behind it.
      void proposals.reload();
      throw failure;
    }
  };
  const dismiss = async (proposal: SkillProposal) => {
    try {
      await skillProposalsApi.dismiss(proposal.id, proposal.rev);
      useUi.getState().pushToast(DISMISSED_TOAST);
      await proposals.reload();
    } catch (failure) {
      void proposals.reload();
      throw failure;
    }
  };
  return (
    <>
      {added ? (
        <div className="cx-cwbn cx-green" role="status" style={{ margin: "0" }}>
          <Ic n="ok" sm />
          <span>
            <b>Added to skills-local.</b>{" "}
            <a
              href={routeHref({ name: "skills", skill: added.name })}
              style={{ color: "inherit", textDecoration: "underline", textUnderlineOffset: "2px" }}
            >
              {added.name}
            </a>{" "}
            is listed on the Skills page as Personal. {AVAILABLE} The {lessonsWord(added.lessons)} it came from stay here and
            stop going into briefs while the skill exists.
          </span>
        </div>
      ) : null}
      <h5 className="cx-t10h5">
        Skill proposals
        <span className="cx-t10r" style={{ textDecoration: "none" }}>
          {open.length}
        </span>
      </h5>
      {open.length === 0 ? (
        <div className="cx-fbx" role="status">
          <h6>No proposals</h6>
          <p style={{ margin: 0, color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
            Crew proposes a skill when 3 or more active lessons of one repository share a source, a check kind and a path
            class. You read it and approve it; nothing is added to skills-local on its own.
          </p>
        </div>
      ) : (
        open.map((proposal) => (
          <ProposalCard
            key={proposal.id}
            proposal={proposal}
            project={project}
            lessons={rows}
            onReview={() =>
              navigate({ name: "settings", section: "crew", crewPage: "skill-proposal", repo: project, id: proposal.id })
            }
            onApprove={() => setConfirming({ kind: "approve", proposal })}
            onDismiss={() => setConfirming({ kind: "dismiss", proposal })}
          />
        ))
      )}
      {confirming?.kind === "approve" ? (
        <ApproveConfirm
          proposal={confirming.proposal}
          project={project}
          onClose={() => setConfirming(null)}
          onConfirm={() => approve(confirming.proposal)}
        />
      ) : null}
      {confirming?.kind === "dismiss" ? (
        <DismissConfirm
          proposal={confirming.proposal}
          name={name}
          onClose={() => setConfirming(null)}
          onConfirm={() => dismiss(confirming.proposal)}
        />
      ) : null}
    </>
  );
}

/** The lines a card previews of the text, and the label that says so. */
const previewOf = (lines: number): { shown: number; label: string } =>
  lines > 8 ? { shown: 8, label: `first 8 of ${lines} lines` } : { shown: lines, label: `${lines} lines` };

/** One proposal: its name and one line, what it is based on, the text as proposed, and the person's three acts. */
function ProposalCard({
  proposal,
  project,
  lessons,
  onReview,
  onApprove,
  onDismiss,
}: {
  proposal: SkillProposal;
  project: string;
  lessons: readonly Lesson[];
  onReview(): void;
  onApprove(): void;
  onDismiss(): void;
}) {
  const text = skillText(proposal.text);
  const title = text.name ?? "Proposed skill";
  const [shown, setShown] = useState(true);
  const count = proposal.lessons.length;
  const active = activeLessons(proposal, lessons);
  // A proposal whose pattern lost a lesson draws only the lessons still active: the others are gone from this pattern.
  const sources = sourceLessons(proposal, lessons).filter((one) => proposal.can_approve || one.state === "active");
  const shownCount = proposal.can_approve ? count : active;
  const preview = previewOf(text.lines);
  // The design bolds only the leading sentence of the stale line: "Based on 2 active lessons now: needs 3."
  const stale = staleWords(proposal, lessons);
  const stop = stale.indexOf(". ");
  const staleLead = stop < 0 ? stale : stale.slice(0, stop + 1);
  const staleRest = stop < 0 ? "" : stale.slice(stop + 1);
  return (
    <div className="cx-ppcard" role="region" aria-label={`Proposed skill ${title}`}>
      <div className="cx-pphead">
        <h6>Proposed skill</h6>
        <span className="cx-apchip cx-proposed">Proposed</span>
        <span className="cx-ppwhen">Proposed {moment(proposal.created_at)}</span>
      </div>
      {text.name ? <div className="cx-ppname">{text.name}</div> : null}
      {text.description ? <div className="cx-ppdesc">{text.description}</div> : null}
      {proposal.can_approve ? null : (
        <div className="cx-fbx cx-amber" role="status" style={{ flexDirection: "row", gap: "8px", alignItems: "flex-start" }}>
          <Ic n="alert" sm />
          <span style={{ color: "var(--color-text)", fontSize: "12.5px" }}>
            <b className="cx-pplead">{staleLead}</b>
            {staleRest}
          </span>
        </div>
      )}
      <div className="cx-ppfkv">
        <span>Based on</span>
        <div className="cx-ppbased">
          <b className={proposal.can_approve ? "cx-ppbasedline" : undefined}>
            {proposal.can_approve ? (
              basedOnWords(proposal, lessons)
            ) : (
              <>
                {active} active {lessonsOnly(active)} now{" "}
                <span style={{ color: "var(--color-muted)" }}>{`(${count} when it was proposed)`}</span>
              </>
            )}
          </b>
          {sources.length > 0 ? (
            <button type="button" className="cx-pptoggle" aria-expanded={shown} onClick={() => setShown((open) => !open)}>
              <Ic n="down" sm />
              <span>
                {shown ? "Hide" : "Show"} the {lessonsWord(shownCount)}
              </span>
            </button>
          ) : null}
          {shown && sources.length > 0 ? (
            <div className="cx-pprows">
              {sources.map((lesson) => (
                <div className="cx-pprow" key={lesson.id}>
                  <span className="cx-apchip" style={{ flex: "none" }}>
                    {SOURCE_WORD[lesson.source]}
                  </span>
                  <span className="cx-pprowtext">
                    {lesson.text}
                    <small>Learned {moment(lesson.created_at)}</small>
                  </span>
                  <button
                    type="button"
                    className="cx-ghost cx-sm"
                    aria-label="Open this lesson"
                    onClick={() =>
                      navigate({ name: "settings", section: "crew", crewPage: "lesson", repo: project, id: lesson.id })
                    }
                  >
                    Open
                  </button>
                </div>
              ))}
            </div>
          ) : null}
        </div>
        <span>Applies to</span>
        <b>
          {ALL_PROJECTS} <span style={{ color: "var(--color-muted)" }}>· {DEFAULT_SELECTION_NOTE}</span>
        </b>
        <span>Would be saved in</span>
        <b>
          {SKILLS_LOCAL} <span style={{ color: "var(--color-muted)" }}>(device-local)</span>
        </b>
      </div>
      <div className="cx-pppreview">
        <span className="cx-pplabel">
          Proposed text (SKILL.md), {preview.label}
        </span>
        <div className="cx-pptext" role="textbox" aria-multiline="true" aria-readonly="true" aria-label="Proposed skill text">
          {firstLines(proposal.text, preview.shown)}
        </div>
      </div>
      <div className="cx-ppnotes">
        <span>{NOTHING_ADDED}</span>
        <span>{TEMPLATE_NOTE}</span>
        <span>{SCAN_NOTE}</span>
      </div>
      <div className="cx-ppacts">
        <button type="button" className="cx-btn" onClick={onReview}>
          <Ic n="doc" sm />
          Review
        </button>
        <button type="button" className="cx-ghost" disabled={!proposal.can_approve} onClick={onApprove}>
          Approve
        </button>
        <button type="button" className="cx-ghost cx-ppdismiss" onClick={onDismiss}>
          Dismiss
        </button>
      </div>
    </div>
  );
}

/** Approve asks first: the proposal has not been read in full here, so its text is what the person already saw. */
function ApproveConfirm({
  proposal,
  project,
  onClose,
  onConfirm,
}: {
  proposal: SkillProposal;
  project: string;
  onClose(): void;
  onConfirm(): Promise<void>;
}) {
  const text = skillText(proposal.text);
  const title = text.name ?? "Proposed skill";
  return (
    <ConfirmDialog
      title={`Add ${title} to skills-local?`}
      confirmLabel="Add to skills-local"
      className="lessons-dialog"
      onClose={onClose}
      onConfirm={onConfirm}
    >
      <p>
        Available to engines through Orbyn's skills tools. Applies to all your projects. Added to the Default selection.
        Crew saves the skill in skills-local on this device and lists it on the Skills page. It writes nothing to any
        repository. You have not opened the full text yet.
      </p>
      <div className="cx-fbx">
        <div className="cx-ppfkv">
          <span>Skill</span>
          <b style={{ fontFamily: "var(--font-mono)", fontSize: "12px" }}>{title}</b>
          <span>Will be saved in</span>
          <b>skills-local (device-local)</b>
          <span>Applies to</span>
          <b>All your projects</b>
          <span>Default selection</span>
          <b>The skill joins it</b>
          <span>Text</span>
          <b>
            As proposed, {text.lines} lines.{" "}
            <button
              type="button"
              className="cx-pptoggle"
              style={{ display: "inline" }}
              onClick={() =>
                navigate({ name: "settings", section: "crew", crewPage: "skill-proposal", repo: project, id: proposal.id })
              }
            >
              <span>Read the full text first</span>
            </button>
          </b>
        </div>
      </div>
    </ConfirmDialog>
  );
}

/** Dismiss asks first; the lessons it came from stay as they are. */
function DismissConfirm({
  proposal,
  name,
  onClose,
  onConfirm,
}: {
  proposal: SkillProposal;
  name: string;
  onClose(): void;
  onConfirm(): Promise<void>;
}) {
  const title = skillText(proposal.text).name ?? "Proposed skill";
  return (
    <ConfirmDialog title="Dismiss this proposal?" confirmLabel="Dismiss" className="lessons-dialog" onClose={onClose} onConfirm={onConfirm}>
      <p>{DISMISS_NOTE}</p>
      <div className="cx-fbx">
        <div className="cx-ppfkv">
          <span>Skill</span>
          <b style={{ fontFamily: "var(--font-mono)", fontSize: "12px" }}>{title}</b>
          <span>Based on</span>
          <b>
            {lessonsWord(proposal.lessons.length)} · This repository ({name})
          </b>
        </div>
      </div>
    </ConfirmDialog>
  );
}
