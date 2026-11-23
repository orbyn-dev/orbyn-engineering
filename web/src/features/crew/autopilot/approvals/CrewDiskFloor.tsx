// Settings › Crew › the free-space floor (Crew Designs KC4, frames `kc4-low-disk-floor-setting`,
// `-floor-error-empty`, `-floor-error-text`, `-floor-error-zero`, `-floor-error-large`,
// `-floor-saved`, `-floor-save-failed`, `-floor-narrow`). A whole number of GB from 1 to 500, typed
// in a plain text field. A refused value sends nothing; a valid one saves the preference and the
// service's answer is what the row shows.
import { useEffect, useState } from "react";
import { queryPreferences, updatePreferences } from "../../../../api/preferences";
import { toFailure, useAsync } from "../../../../lib/useAsync";
import { ErrorState, Loading } from "../../../../product/ui/states";
import { Button } from "../../../../ui/controls";
import type { ScopedPreferenceRow } from "../../../../generated/ScopedPreferenceRow";
import "./crewDiskFloor.css";

const KEY = "crew_min_free_gb";
const TITLE = "Free disk space Crew needs";
const DESCRIPTION = "Crew refuses to start a worker when the disk that holds the lead’s folder has less than this free. It only refuses: it deletes nothing to make room. A whole number of GB, from 1 to 500. Default 5 GB.";
const INVALID = "Enter a number from 1 to 500.";
const DEFAULT = 5;
const MIN = 1;
const MAX = 500;

/** A whole number in the drawn range, or null when the typed text is not one. */
function wholeGb(text: string): number | null {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const value = Number(trimmed);
  return Number.isSafeInteger(value) && value >= MIN && value <= MAX ? value : null;
}

const valueOf = (row: ScopedPreferenceRow | undefined): number => {
  const value = row?.value;
  return typeof value === "number" && Number.isFinite(value) ? value : DEFAULT;
};

/**
 * Settings › Crew's free-space floor. A draft stays in the box through a refused save; only the
 * service's answer becomes the stored value, and `revision` follows that answer so a second save
 * compares against what the service holds.
 */
export function CrewDiskFloor() {
  const read = useAsync("crew-disk-floor", (signal) => queryPreferences({ scope: {}, keys: [KEY] }, signal), { keep: true });
  const [stored, setStored] = useState<number | null>(null);
  const [draft, setDraft] = useState("");
  const [revision, setRevision] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const row = read.data?.rows.find((entry) => entry.key === KEY);
  useEffect(() => {
    if (stored !== null || !row) return;
    setStored(valueOf(row));
    setDraft(String(valueOf(row)));
    setRevision(row.revision);
    setVersion(read.data?.catalog.version ?? null);
  }, [read.data, row, stored]);
  if (read.status === "loading") return <Loading label="Loading settings" rows={1} />;
  if (read.status === "error") return <ErrorState title="Couldn't load settings" failure={read.error} onRetry={read.reload} />;
  if (!row) return null;
  if (stored === null) return <Loading label="Loading settings" rows={1} />;
  const next = wholeGb(draft);
  const invalid = next === null;
  const changed = next !== null && next !== stored;
  const canSave = !busy && (changed || failed !== null);
  const save = async () => {
    if (!canSave || next === null || revision === null || version === null) return;
    setBusy(true);
    setFailed(null);
    setSaved(false);
    try {
      const answer = await updatePreferences({
        scope: {},
        membership_version: version,
        if_match: { [KEY]: revision },
        edits: { [KEY]: { op: "set", value: next } },
      });
      const storedRow = answer.rows.find((entry) => entry.key === KEY);
      const value = valueOf(storedRow);
      // The service's answer is what the row shows: its value, revision and catalog version.
      setStored(value);
      setDraft(String(value));
      setRevision(storedRow?.revision ?? revision);
      setVersion(answer.catalog.version);
      setSaved(true);
    } catch (error) {
      // The service's own message stays verbatim, and the typed value stays in the box.
      setFailed(toFailure(error).message);
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="setting-group" aria-label={TITLE}>
      <div className="setting-list">
        <div className="setting-line" id="crew-free-disk-space" tabIndex={-1}>
          <div className="setting-line-text">
            <div className="setting-line-heading">
              <span className="setting-line-title">{TITLE}</span>
            </div>
            <span className="setting-line-desc">{DESCRIPTION}</span>
          </div>
          <div className="setting-line-control">
            <div className="crew-floor-control" aria-busy={busy}>
              <input
                className="crew-floor-field"
                type="text"
                inputMode="numeric"
                aria-label={TITLE}
                placeholder="Enter GB"
                value={draft}
                disabled={busy}
                onChange={(event) => {
                  setDraft(event.target.value);
                  setSaved(false);
                  setFailed(null);
                }}
              />
              <span className="crew-floor-unit">GB</span>
              <Button small disabled={!canSave} onClick={() => void save()}>Save</Button>
              {saved ? <span className="crew-floor-saved" role="status">Saved</span> : null}
            </div>
          </div>
        </div>
      </div>
      {invalid || failed ? <p className="field-error crew-floor-error" role="alert">{failed ?? INVALID}</p> : null}
    </section>
  );
}
