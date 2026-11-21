// Settings › Crew › Daily digest, from the design's Settings › Crew frame `settings-crew` (Crew Designs). The frame
// draws the row's label, help text and the value `17:00` as a plain value; it draws no time control, so the smallest
// designed-consistent control is a time field where that value sits (the same native field Settings › Scheduled tasks
// uses). The value is `{time, utc_offset_minutes}` exactly as a scheduled task stores it, saved with the offset the page
// was in, and it is a Person write. Nothing is sent until a person saves: with no offset stored the service cannot
// resolve "17:00 local", and it sends nothing rather than early.
import { useRef, useState } from "react";
import { toFailure, useAsync } from "../../../../lib/useAsync";
import type { AutopilotDigestSchedule } from "../../../../generated/AutopilotDigestSchedule";
import { crewApi } from "./api";

const DEFAULT_TIME = "17:00";

/** Minutes east of UTC at this page right now; `|| 0` keeps UTC from becoming `-0`. */
const pageOffset = () => -new Date().getTimezoneOffset() || 0;

function Row() {
  const read = useAsync("crew-digest-schedule", (signal) => crewApi.digestSchedule(signal));
  const [saved, setSaved] = useState<AutopilotDigestSchedule | null>(null);
  const [draft, setDraft] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const inflight = useRef(false);
  const [failed, setFailed] = useState<string | null>(null);
  const stored = saved ?? read.data ?? null;
  const shown = stored?.time ?? DEFAULT_TIME;
  const draftTime = draft ?? shown;
  // The same time is still worth saving when the stored offset is missing (the 17:00 default has never been turned
  // on) or differs from the page's now (a daylight-saving change). An untouched field on blur writes nothing.
  const offsetStale = stored !== null && stored.utc_offset_minutes !== pageOffset();
  const save = (next: string, deliberate = false) => {
    if (!next || inflight.current) return;
    if (next === shown && !(offsetStale && (draft !== null || deliberate))) return;
    inflight.current = true;
    setBusy(true);
    setFailed(null);
    // The stored answer is the only thing shown after a save: a write that fails leaves the old time and says why.
    crewApi.setDigestSchedule({ time: next, utc_offset_minutes: pageOffset() }).then(
      (view) => {
        setSaved(view);
        setDraft(null);
      },
      (error: unknown) => {
        setFailed(toFailure(error).message);
        setDraft(null);
      },
    ).finally(() => {
      inflight.current = false;
      setBusy(false);
    });
  };
  return (
    <section className="dg-row" aria-label="Daily digest">
      <div>
        <b>Daily digest</b>
        <small>What merged, what is waiting for your review and what was reverted, sent once a day in your local time.</small>
        {stored && stored.utc_offset_minutes === null ? (
          <small>Nothing is sent until you save a time: this is the first place the app learns your local time zone.</small>
        ) : null}
      </div>
      <input
        className="dg-time"
        type="time"
        aria-label="Daily digest time"
        aria-busy={busy || undefined}
        disabled={read.status === "loading"}
        value={draftTime}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => save(draftTime)}
        onKeyDown={(event) => {
          if (event.key === "Enter") save(draftTime, true);
        }}
      />
      {failed ? (
        <p role="alert" className="dg-error">
          {failed}
        </p>
      ) : null}
      {read.status === "error" && !stored ? (
        <p role="alert" className="dg-error">
          {read.error.message}
        </p>
      ) : null}
    </section>
  );
}

/** Settings › Crew › Daily digest. Preview-gated with the rest of the Crew routes. */
export function DigestTime() {
  return <Row />;
}
