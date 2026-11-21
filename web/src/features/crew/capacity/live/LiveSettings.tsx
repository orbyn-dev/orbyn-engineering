import { useState } from "react";
import { NETWORK_ERROR, type ApiFailure } from "../../../../api/client";
import { toFailure } from "../../../../lib/useAsync";
import type { CapacitySampling } from "../../../../generated/CapacitySampling";
import type { CapacityState } from "../../../../generated/CapacityState";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { useNarrow } from "../../board/useWidth";
import { Frame } from "../../shared/Frame";
import { Ic } from "../../shared/parts";
import { capacityApi } from "./api";
import { changes, notSampled } from "./model";
import { useCapacityState } from "./useCapacity";

/** What the person has typed and not saved yet: the limit, the sampling choice and the pause switch. */
type Draft = { limit: number; sampling: CapacitySampling; pause_enabled: boolean };

/** The pause row's value: what the frame draws when it is on, "Needs a limit" before one is saved, and Off until a person turns it on. */
function pauseValue(state: CapacityState): string {
  if (!state.settings_saved) return "Needs a limit";
  return state.pause_enabled ? "Pause new Low-priority starts" : "Off";
}

/**
 * Why a save failed, in words: a refusal shows the service's own message (for example "Only a person can change this."),
 * and a service that is not there says so. Nothing was saved either way, and the person's values stay on the page.
 */
function saveFailure(failure: ApiFailure): string {
  return failure.code === NETWORK_ERROR
    ? "Check that the Orbyn service is running. Your changes are still here, and nothing was saved."
    : failure.message;
}

/**
 * The limit as the design's stepper: one change fewer or more, and the value reads as a spinbutton, so arrow keys work.
 * It goes down to 0: the service refuses a limit under 1 in its own words, and the page never invents a rule of its own.
 */
function Stepper({ limit, set }: { limit: number; set(limit: number): void }) {
  const step = (by: number) => set(Math.max(0, limit + by));
  return (
    <span className="cx-stepper">
      <button type="button" className="cx-ghost" aria-label="One change fewer" disabled={limit <= 0} onClick={() => step(-1)}>
        <Ic n="minus" sm />
      </button>
      <span
        className="cx-sv"
        role="spinbutton"
        tabIndex={0}
        aria-label="How many changes you can review"
        aria-valuenow={limit}
        aria-valuemin={0}
        onKeyDown={(event) => {
          const by = event.key === "ArrowUp" ? 1 : event.key === "ArrowDown" ? -1 : 0;
          if (by === 0) return;
          event.preventDefault();
          step(by);
        }}
      >
        {limit}
      </span>
      <button type="button" className="cx-ghost" aria-label="One change more" onClick={() => step(1)}>
        <Ic n="plus" sm />
      </button>
      <span className="cx-unit">changes</span>
    </span>
  );
}

/**
 * The three settings as the design's editor rows: the limit stepper, the pause switch and the low-risk choice, with Cancel
 * and Save under them. Nothing is sent until Save, and only a person's own click presses it. A save failure keeps what was
 * entered, says why in one banner and offers Try again with the same values; a save that works puts the page back to its
 * read-only rows, from the service's answer.
 */
function Body({ state, project, apply }: { state: CapacityState; project: string; apply(state: CapacityState): void }) {
  const [draft, setDraft] = useState<Draft | null>(null);
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<ApiFailure | null>(null);
  const editing = draft !== null;
  const sampled = (draft?.sampling ?? state.sampling) === "one_in_four";
  const open = () => {
    setFailure(null);
    setDraft({ limit: state.limit, sampling: state.sampling, pause_enabled: state.pause_enabled });
  };
  const cancel = () => {
    setFailure(null);
    setDraft(null);
  };
  const send = () => {
    if (!draft || saving) return;
    const values = draft;
    setSaving(true);
    setFailure(null);
    void capacityApi
      .saveSettings({ project, limit: values.limit, sampling: values.sampling, pause_enabled: values.pause_enabled })
      .then((saved) => {
        setSaving(false);
        setDraft(null);
        apply(saved);
      })
      .catch((error: unknown) => {
        setSaving(false);
        setFailure(toFailure(error));
      });
  };

  return (
    <>
      {failure ? (
        <div className="cx-cwbn cx-red" style={{ margin: "0", alignItems: "flex-start" }} role="alert">
          <Ic n="alert" sm style={{ marginTop: "1px", color: "var(--color-danger)" }} />
          <span className="cx-why">
            <b>Can’t save review capacity</b>
            <span>{saveFailure(failure)}</span>
          </span>
          <span className="cx-acts">
            <button type="button" className="cx-ghost" disabled={saving} onClick={send}>
              Try again
            </button>
          </span>
        </div>
      ) : null}
      {state.paused ? (
        <div className="cx-cwbn" style={{ margin: "0" }} role="status">
          <Ic n="info" sm />
          <span>
            <b>New low-priority starts are paused: your review queue is over its limit.</b>
            {` ${changes(state.paused.waiting)} ${state.paused.waiting === 1 ? "is" : "are"} waiting for your review and your limit is ${state.paused.limit}. Running work carries on, and tasks you start and High-priority work are not paused.`}
          </span>
        </div>
      ) : null}
      {!state.settings_saved && !editing ? (
        <div className="cx-cwbn" style={{ margin: "0" }} role="status">
          <Ic n="info" sm />
          <span>
            <b>No limit is set.</b>
            {` ${state.waiting} High and Medium ${state.waiting === 1 ? "change is" : "changes are"} waiting for your review now. Crew keeps starting work whatever the queue holds.`}
          </span>
          <span className="cx-acts">
            <button type="button" className="cx-btn" onClick={open}>
              Set a limit
            </button>
          </span>
        </div>
      ) : null}
      <div>
        <div className="cx-setrow">
          <div>
            <b>How many changes you can review</b>
            <small>
              {state.settings_saved
                ? "Crew counts the High and Medium changes that merged and are waiting for you. Low-risk changes are batched, have no deadline and don’t count."
                : "Not set. Until you set it, Crew never pauses a start because of your review queue. Crew counts the High and Medium changes that merged and are waiting for you; Low-risk changes don’t count."}
            </small>
          </div>
          {draft ? (
            <Stepper limit={draft.limit} set={(limit) => setDraft({ ...draft, limit })} />
          ) : state.settings_saved ? (
            <span style={{ display: "flex", gap: "14px", alignItems: "center", flexWrap: "wrap" }}>
              <span className="cx-sv">{changes(state.limit)}</span>
              <button type="button" className="cx-ghost cx-sm" onClick={open}>
                Change the limit
              </button>
            </span>
          ) : (
            <span className="cx-sv">Not set</span>
          )}
        </div>
        <div className="cx-setrow">
          <div>
            <b>When the queue passes it</b>
            <small>
              Crew doesn’t begin new Low-priority tasks it started itself. Tasks you start, High-priority work and everything already running carry on.
              {state.settings_saved ? " Paused starts begin by themselves when the queue is back at the limit or under it." : ""}
            </small>
          </div>
          {draft ? (
            <label className="cx-toggle">
              <input
                type="checkbox"
                role="switch"
                checked={draft.pause_enabled}
                onChange={(event) => setDraft({ ...draft, pause_enabled: event.target.checked })}
              />
              <span>Pause new Low-priority starts</span>
            </label>
          ) : (
            <span className="cx-sv cx-ro">{pauseValue(state)}</span>
          )}
        </div>
        <div className="cx-setrow">
          <div>
            <b>Low-risk changes</b>
            <small>
              {state.settings_saved
                ? "Review every low-risk batch (the default), or review a sample. A sample is sized to the batch. The changes not sampled are listed in the digest."
                : "Review every low-risk batch, or review a sample sized to the batch. The changes not sampled are listed in the digest."}
            </small>
          </div>
          {draft ? (
            <div className="cx-choices" role="radiogroup" aria-label="Low-risk changes">
              {([
                ["off", "Review every batch"],
                ["one_in_four", "Review a sample"],
              ] as const).map(([value, label]) => (
                <label className="cx-choice" key={value}>
                  <input
                    type="radio"
                    name="crew-capacity-sampling"
                    value={value}
                    checked={draft.sampling === value}
                    onChange={() => setDraft({ ...draft, sampling: value })}
                  />
                  {label}
                </label>
              ))}
            </div>
          ) : (
            <span className="cx-sv">{sampled ? "Review a sample" : state.settings_saved ? "Review every batch" : "Review every batch · default"}</span>
          )}
        </div>
        <div className="cx-setrow">
          <div>
            <b>Review coverage</b>
            <small>When Crew merges it records which lines an agent wrote as a git note on the merge commit. The map counts the lines no person has reviewed.</small>
          </div>
          <span className="cx-sv cx-ro">Recorded at merge</span>
        </div>
      </div>
      {draft ? (
        <div className="cx-editfoot">
          <span className="cx-hint">Nothing is sent until you press Save.</span>
          <span className="cx-acts">
            <button type="button" className="cx-ghost" disabled={saving} onClick={cancel}>
              Cancel
            </button>
            <button type="button" className="cx-btn" disabled={saving} onClick={send}>
              Save
            </button>
          </span>
        </div>
      ) : null}
      {!editing && sampled ? (
        state.plan.length === 0 ? (
          <Empty title="No sampling plan to show" />
        ) : (
          <div className="cx-fbx">
            <h6>The sampling plan</h6>
            <p>One in four, rounded up, of the low-risk merges in a batch. Which ones is decided after they merge and is not given to the worker.</p>
            <div className="cx-t15plan">
              <div className="cx-h">Changes in the batch</div>
              <div className="cx-h">Sampled for review</div>
              <div className="cx-h">Not sampled</div>
              {state.plan.flatMap((row) => [
                <div key={`${row.from}-a`}>{`${row.from} to ${row.to}`}</div>,
                <div key={`${row.from}-b`}>{row.sampled}</div>,
                <div key={`${row.from}-c`}>{notSampled(row)}</div>,
              ])}
            </div>
          </div>
        )
      ) : null}
      {state.settings_saved ? (
        <div className="cx-fbx">
          <h6>What the limit does and doesn’t do</h6>
          <div className="cx-fkv">
            <span>Does</span>
            <b>Pauses new Low-priority starts while the queue is over the limit, and says so on the Crew page</b>
            <span>Doesn’t</span>
            <b>Stop running work, stop a merge, or pause a task you started or any High-priority task</b>
          </div>
        </div>
      ) : null}
      <p style={{ margin: "2px 0 0", display: "flex", gap: "6px", alignItems: "flex-start", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.45 }}>
        <Ic n="info" sm />
        <span>Workers run as you, with your machine’s permissions.</span>
      </p>
    </>
  );
}

/**
 * Crew Designs v77, `capacity-settings` and its saved-off, paused, loading, error and no-plan states, `capacity-settings-unset` and the banner of `capacity-over`,
 * on the real capacity read; the editor frames `capacity-settings-editor`, `-editor-narrow` and `-editor-error` on the Person-only settings write. Read-only until a
 * person opens the editor: a load, a refresh or a retry of a read never sends a write, and only Save does. The sheet stacks its rows and its footer under the
 * design's 640 px window, measured on the sheet itself.
 */
export function LiveSettings({ project }: { project: string }) {
  const read = useCapacityState(project);
  const [ref, narrow] = useNarrow();
  const state = read.data;
  return (
    <Frame sheet live>
      <div ref={ref} className={`cx-cwsheet${narrow ? " cx-narrow" : ""}`} style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Review capacity
          <span className="cx-cwpv">Preview</span>
        </h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          Agents can merge faster than a person can read. Say how many changes you can review, and Crew slows the work it starts on its own before the queue gets ahead of you. It never stops a merge, and nothing here changes what you can review.
        </p>
        {read.status === "loading" ? <Loading label="Reading review capacity" rows={3} /> : null}
        {read.status === "error" ? <ErrorState title="Can’t read review capacity" failure={read.error} onRetry={() => void read.reload()} /> : null}
        {state && read.status === "ready" ? <Body state={state} project={project} apply={(saved) => read.setData(saved)} /> : null}
      </div>
    </Frame>
  );
}
