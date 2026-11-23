import { useEffect, useState, type ReactNode } from "react";
import { Loader2 } from "lucide-react";
import { NETWORK_ERROR, type ApiFailure } from "../../../../api/client";
import { toFailure } from "../../../../lib/useAsync";
import { navigate, routeHref } from "../../../../product/state/route";
import { useVisibleProjects, useWorkspace } from "../../../../product/state/workspace";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { useUi } from "../../../../product/state/ui";
import { useCrewPreview } from "../../autopilot/approvals/useCrew";
import { Frame } from "../../shared/Frame";
import { Ic } from "../../shared/parts";
import { markSkillAdded } from "./marks";
import {
  ALL_PROJECTS,
  APPROVING_BANNER,
  DEFAULT_SELECTION_NOTE,
  REVIEW_BANNER,
  REVIEW_FOOT,
  REVIEW_NOTE,
  REVIEW_NOTE_BUSY,
  SKILLS_LOCAL,
  SKILL_SECRET_REFUSED,
  lessonsWord,
  proposalName,
  skillText,
} from "./model";
import { skillProposalsApi } from "./api";
import { useProposals } from "./useProposals";
import "./proposals.css";

/** The page card these screens draw in, so a state before a project is known is not flush to the corner. */
function Card({ children }: { children: ReactNode }) {
  return <Frame sheet live>{children}</Frame>;
}

const back = (repo: string) =>
  routeHref({ name: "settings", section: "crew", crewPage: "lessons", repo });

/**
 * One proposal's review page (KC9): the whole SKILL.md in an editable box, the facts around it, and Approve under it. It is
 * opened from a proposal's Review, on the Lessons page or on the Skills page's Proposed card. Nothing is drawn, and no route
 * is read, while Crew Preview is off or the address names no proposal; a proposal that is no longer open draws nothing, as
 * the read has no such row to describe.
 */
export function SkillProposalPage({ id, repo }: { id?: string; repo?: string }) {
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
  const project = projects.find((entry) => entry.id === repo) ?? projects[0];
  if (!project) return <Card><Empty title="No project to show" /></Card>;
  if (!id) return <Card><Empty title="No proposal to show" /></Card>;
  return <ProposalReview key={`${project.id}:${id}`} id={id} repo={project.id} />;
}

/** The review form, once the proposal read answered: the text, the two confirm states and the write's four answers. */
function ProposalReview({ id, repo }: { id: string; repo: string }) {
  const proposals = useProposals(repo);
  const [text, setText] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<{ secret: boolean; detail: string } | null>(null);
  const proposal = proposals.status === "ready" ? proposals.list.proposals.find((one) => one.id === id && one.state === "open") : undefined;
  // The person's own words start from the text the read carried, once it has answered.
  useEffect(() => {
    if (proposal && text === null) setText(proposal.text);
  }, [proposal, text]);
  if (proposals.status === "missing") return null;
  const head = (
    <>
      <BackToLessons repo={repo} />
      <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
        Crew › Proposed skill
        <span className="cx-cwpv">Preview</span>
      </h3>
    </>
  );
  if (proposals.status === "loading")
    return (
      <Frame sheet live>
        <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
          {head}
          <Loading label="Reading the proposal" rows={3} />
        </div>
      </Frame>
    );
  if (proposals.status === "error")
    return (
      <Frame sheet live>
        <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
          {head}
          <ErrorState title="Can’t read the proposal" failure={proposals.failure} onRetry={() => void proposals.reload()} />
        </div>
      </Frame>
    );
  if (!proposal) return null;
  const parsed = skillText(proposal.text);
  const title = parsed.name ?? proposalName(proposal);
  const padding = text ?? proposal.text;
  const lines = padding.split("\n").length;
  const send = async () => {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    try {
      const answer = await skillProposalsApi.approve(proposal.id, { text: padding, rev: proposal.rev });
      markSkillAdded(answer.skill?.name ?? title, answer.lessons.length);
      navigate({ name: "settings", section: "crew", crewPage: "lessons", repo });
    } catch (error) {
      const one = toFailure(error);
      // A proposal that moved on is read again by the list it came from: the person goes back to it, told why.
      if (one.code === "stale" || one.code === "not_open") {
        useUi.getState().pushToast(one.message);
        navigate({ name: "settings", section: "crew", crewPage: "lessons", repo });
        return;
      }
      setBusy(false);
      setFailure({ secret: one.code === "secret_refused", detail: refusal(one) });
    }
  };
  return (
    <Frame sheet live>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        {head}
        {failure?.secret ? (
          <SecretRefused />
        ) : failure ? (
          <div className="cx-cwbn cx-red" role="alert" style={{ margin: "0", alignItems: "flex-start" }}>
            <Ic n="alert" sm style={{ marginTop: "1px", color: "var(--color-danger)" }} />
            <span className="cx-why">
              <b>Can’t add the skill</b>
              <span>{failure.detail}</span>
            </span>
            <span className="cx-acts">
              <button type="button" className="cx-ghost" onClick={() => void send()}>
                Try again
              </button>
            </span>
          </div>
        ) : busy ? (
          <div className="cx-cwbn" role="status" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>{APPROVING_BANNER}</span>
          </div>
        ) : (
          <div className="cx-cwbn" role="status" style={{ margin: "0" }}>
            <Ic n="info" sm />
            <span>
              <b>Nothing is installed or enabled until you press Approve.</b>{" "}
              {REVIEW_BANNER.replace("Nothing is installed or enabled until you press Approve. ", "")}
            </span>
          </div>
        )}
        <div className="cx-fbx">
          <h6>The proposal</h6>
          <div className="cx-ppfkv" style={{ gridTemplateColumns: "120px minmax(0, 1fr)" }}>
            {parsed.name ? (
              <>
                <span>Name</span>
                <b style={{ fontFamily: "var(--font-mono)", fontSize: "12px" }}>{parsed.name}</b>
              </>
            ) : null}
            {parsed.description ? (
              <>
                <span>What it does</span>
                <b>{parsed.description}</b>
              </>
            ) : null}
            <span>Based on</span>
            <b>{lessonsWord(proposal.lessons.length)}</b>
            <span>Applies to</span>
            <b>
              {ALL_PROJECTS} <span style={{ color: "var(--color-muted)" }}>· {DEFAULT_SELECTION_NOTE}</span>
            </b>
            <span>Will be saved in</span>
            <b>
              {SKILLS_LOCAL} <span style={{ color: "var(--color-muted)" }}>(device-local)</span>
            </b>
          </div>
        </div>
        <div className="cx-t12fl">
          <span className="cx-t12lb">Skill text (SKILL.md){busy ? "" : ". You can edit it."}</span>
          <textarea
            className={["cx-pptext", "cx-input", busy ? "cx-readonly" : ""].filter(Boolean).join(" ")}
            aria-label="Skill text"
            aria-multiline="true"
            aria-readonly={busy || undefined}
            readOnly={busy}
            value={padding}
            onChange={(event) => setText(event.target.value)}
          />
          <span className="cx-t12lb" style={{ textAlign: "right" }}>
            {lines} lines · {padding.length} characters
          </span>
        </div>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          {busy ? REVIEW_NOTE_BUSY : REVIEW_NOTE}
        </p>
        <div className="cx-ppfoot">
          {busy ? (
            <span role="status" style={{ display: "inline-flex", alignItems: "center", gap: "8px", color: "var(--color-textSoft)", fontSize: "12px" }}>
              <Loader2 size={14} className="spin" aria-hidden />
              Adding {title} to skills-local.
            </span>
          ) : (
            <span className="cx-hint">{REVIEW_FOOT}</span>
          )}
          <span className="cx-acts">
            <button
              type="button"
              className="cx-ghost"
              disabled={busy}
              onClick={() => navigate({ name: "settings", section: "crew", crewPage: "lessons", repo })}
            >
              Cancel
            </button>
            <button type="button" className="cx-btn" disabled={busy} aria-busy={busy || undefined} onClick={() => void send()}>
              {busy ? "Adding…" : "Approve"}
            </button>
          </span>
        </div>
        <p style={{ margin: "2px 0 0", display: "flex", gap: "6px", alignItems: "flex-start", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.45 }}>
          <Ic n="info" sm />
          <span>Workers run as you, with your machine’s permissions.</span>
        </p>
      </div>
    </Frame>
  );
}

/** The way back above the title. */
function BackToLessons({ repo }: { repo: string }) {
  return (
    <div style={{ display: "flex" }}>
      <a
        className="cx-t10r"
        href={back(repo)}
        style={{ marginLeft: "0", whiteSpace: "nowrap", display: "inline-flex", alignItems: "center", gap: "2px", color: "inherit" }}
      >
        <Ic n="left" sm />
        <span style={{ textDecoration: "underline", textUnderlineOffset: "2px" }}>Back to Lessons</span>
      </a>
    </div>
  );
}

/** The scan's refusal, in the design's exact sentence. */
function SecretRefused() {
  const [lead, ...rest] = SKILL_SECRET_REFUSED.split(": ");
  return (
    <div className="cx-cwbn cx-red" role="alert" style={{ margin: "0", alignItems: "flex-start" }}>
      <Ic n="alert" sm style={{ marginTop: "1px", color: "var(--color-danger)" }} />
      <span>
        <b>{lead}:</b> {rest.join(": ")}
      </span>
    </div>
  );
}

/** What a failed approval says: the service's own words, then the drawn tail that nothing was added. */
function refusal(failure: ApiFailure): string {
  return failure.code === NETWORK_ERROR
    ? "Check that the Orbyn service is running. Nothing was added. Your text is still here and the proposal is still open."
    : `${failure.message} Nothing was added. Your text is still here and the proposal is still open.`;
}
