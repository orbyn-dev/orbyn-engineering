import type { CapacityState } from "../../../../generated/CapacityState";
import { Empty, ErrorState, Loading } from "../../../../product/ui/states";
import { Frame } from "../../shared/Frame";
import { Btn, Ic } from "../../shared/parts";
import { Planned, WHY } from "../Planned";
import { changes, notSampled } from "./model";
import { useCapacityState } from "./useCapacity";

/** The pause row's value: what the frame draws when it is on, "Needs a limit" before one is saved, and Off until a person turns it on. */
function pauseValue(state: CapacityState): string {
  if (!state.settings_saved) return "Needs a limit";
  return state.pause_enabled ? "Pause new Low-priority starts" : "Off";
}

/**
 * Crew Designs v77, `capacity-settings` and its saved-off, paused, loading, error and no-plan states, `capacity-settings-unset` and the banner of `capacity-over`, on the real
 * capacity read. Read only: the editor for the limit, the sampling and the pause switch is Planned, so the two buttons that
 * would open it are drawn disabled beside a Planned pill, and nothing on this screen sends a write.
 */
export function LiveSettings({ project }: { project: string }) {
  const read = useCapacityState(project);
  const state = read.data;
  return (
    <Frame sheet live>
      <div className="cx-cwsheet" style={{ maxWidth: "760px" }}>
        <h3 style={{ margin: "0", fontSize: "16px", fontWeight: 500, color: "var(--color-text)" }}>
          Crew › Review capacity
          <span className="cx-cwpv">Preview</span>
        </h3>
        <p style={{ margin: "0", color: "var(--color-muted)", fontSize: "12.5px", lineHeight: 1.5 }}>
          Agents can merge faster than a person can read. Say how many changes you can review, and Crew slows the work it starts on its own before the queue gets ahead of you. It never stops a merge, and nothing here changes what you can review.
        </p>
        {read.status === "loading" ? <Loading label="Reading review capacity" rows={3} /> : null}
        {read.status === "error" ? <ErrorState title="Can’t read review capacity" failure={read.error} onRetry={() => void read.reload()} /> : null}
        {state && read.status === "ready" ? <Body state={state} /> : null}
      </div>
    </Frame>
  );
}

function Body({ state }: { state: CapacityState }) {
  const sampled = state.sampling === "one_in_four";
  return (
    <>
      {state.paused ? (
        <div className="cx-cwbn" style={{ margin: "0" }} role="status">
          <Ic n="info" sm />
          <span>
            <b>New low-priority starts are paused: your review queue is over its limit.</b>
            {` ${changes(state.paused.waiting)} ${state.paused.waiting === 1 ? "is" : "are"} waiting for your review and your limit is ${state.paused.limit}. Running work carries on, and tasks you start and High-priority work are not paused.`}
          </span>
        </div>
      ) : null}
      {!state.settings_saved ? (
        <div className="cx-cwbn" style={{ margin: "0" }} role="status">
          <Ic n="info" sm />
          <span>
            <b>No limit is set.</b>
            {` ${state.waiting} High and Medium ${state.waiting === 1 ? "change is" : "changes are"} waiting for your review now. Crew keeps starting work whatever the queue holds.`}
          </span>
          <span className="cx-acts">
            <Planned why={WHY.editor} />
            <Btn className="cx-btn">Set a limit</Btn>
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
          {state.settings_saved ? (
            <span style={{ display: "flex", gap: "14px", alignItems: "center", flexWrap: "wrap" }}>
              <span className="cx-sv">{changes(state.limit)}</span>
              <span style={{ display: "inline-flex", gap: "6px", alignItems: "center" }}>
                <Planned why={WHY.editor} />
                <Btn className="cx-ghost" style={{ height: "26px", fontSize: "12px", padding: "0 10px" }}>Change the limit</Btn>
              </span>
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
          <span className="cx-sv cx-ro">{pauseValue(state)}</span>
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
          <span className="cx-sv">{sampled ? "Review a sample" : state.settings_saved ? "Review every batch" : "Review every batch · default"}</span>
        </div>
        <div className="cx-setrow">
          <div>
            <b>Review coverage</b>
            <small>When Crew merges it records which lines an agent wrote as a git note on the merge commit. The map counts the lines no person has reviewed.</small>
          </div>
          <span className="cx-sv cx-ro">Recorded at merge</span>
        </div>
      </div>
      {sampled ? (
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
      <div className="cx-fbx">
        <h6>What the limit does and doesn’t do</h6>
        <div className="cx-fkv">
          <span>Does</span>
          <b>Pauses new Low-priority starts while the queue is over the limit, and says so on the Crew page</b>
          <span>Doesn’t</span>
          <b>Stop running work, stop a merge, or pause a task you started or any High-priority task</b>
        </div>
      </div>
      <p style={{ margin: "2px 0 0", display: "flex", gap: "6px", alignItems: "flex-start", color: "var(--color-muted)", fontSize: "12px", lineHeight: 1.45 }}>
        <Ic n="info" sm />
        <span>Workers run as you, with your machine’s permissions.</span>
      </p>
    </>
  );
}
