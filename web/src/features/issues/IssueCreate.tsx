import { ChevronDown, Plus } from "lucide-react";
import { useId, useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { canSendPersonKey } from "../../api/personKey";
import { useAsync } from "../../lib/useAsync";
import { navigate } from "../../product/state/route";
import { Button, Field } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { issuesApi, markNotOffered, useApiVersion, useCreateRefused } from "./api";
import { TRACKER_NAMES, type TrackerKind, type TrackerRow } from "./contract";
import { BODY_MAX, byteLength, createProblem, outcomeUnknown, routeAbsent } from "./edit";
import { Person } from "./IssuePerson";
import { Picker, type PickChoice } from "./IssueEdit";
import { reasonOf } from "./model";
import { PlannedControl } from "./Planned";

/**
 * The New issue control of the Issues page bar and the dialog it opens (issue-write-new, -new-template, -new-refused). One button
 * in the page bar, no new navigation; one dialog with the tracker and scope, the tracker's template when it offers any, the title,
 * the description, and the label and assignee pickers when this tracker offers candidates. Create issue is one
 * `POST /api/issues/item/create`: one call creates one issue, and nothing here sends it twice.
 *
 * - The create write is Person-class, so a browser tab with no desktop shell is not offered the button at all.
 * - A create whose outcome is unknown (the service's own "possibly created" answer) closes the dialog on nothing but Cancel and
 *   Check the list: the page re-reads the list and the same draft is never sent again. Other refusals keep the draft exactly as
 *   it stands and Create issue sends it once more on the next click.
 * - A first "no such route" puts the New issue control back to its Planned pill for the life of the client.
 * - The dialog reads the templates and candidates only while it is open: the templates decide whether the Template row is drawn
 *   (GitHub, Forgejo and Azure DevOps answer none), and the candidates decide whether the label and assignee rows are (only GitHub
 *   builds them; GitLab, Forgejo, Azure DevOps and Linear answer 422 "… issue edits arrive in a later update.").
 */

/** The New issue button of the page bar and its dialog. `preferred` is the list's current tracker, when that one is connected. */
export function NewIssue({ trackers, preferred, onCheckList }: { trackers: TrackerRow[]; preferred: TrackerKind | null; onCheckList(): void }) {
  const refused = useCreateRefused();
  const [open, setOpen] = useState(false);
  const rows = trackers.filter((row) => row.connected && (row.scope ?? "") !== "");
  // A Person-class write: where the page has no shell to send the key, the control is not offered. No connected scope, no dialog.
  if (!canSendPersonKey() || rows.length === 0) return null;
  if (refused) {
    return <PlannedControl label="New issue" what="Creating issues" className="btn btn-primary"><Plus size={14} aria-hidden />New issue</PlannedControl>;
  }
  return (
    <>
      <Button tone="primary" icon={<Plus size={14} aria-hidden />} onClick={() => setOpen(true)}>New issue</Button>
      {open ? (
        <NewIssueDialog
          rows={rows}
          preferred={preferred}
          onClose={() => setOpen(false)}
          onCheckList={() => { setOpen(false); onCheckList(); }}
        />
      ) : null}
    </>
  );
}

const scopeOf = (row: TrackerRow): string => row.scope ?? "";
const badgeOf = (tracker: TrackerKind): string => TRACKER_NAMES[tracker].slice(0, 2).toUpperCase();

/** Whether a candidates read means this tracker has no label and people list (`422 unsupported`) or no route at all. */
const noCandidates = (failure: ApiFailure): boolean => failure.status === 422 || routeAbsent(failure.status, failure.code, failure.message);

function NewIssueDialog({ rows, preferred, onClose, onCheckList }: { rows: TrackerRow[]; preferred: TrackerKind | null; onClose(): void; onCheckList(): void }) {
  const version = useApiVersion();
  const [picked, setPicked] = useState<TrackerRow>(() => rows.find((row) => row.tracker === preferred) ?? rows[0]!);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [labels, setLabels] = useState<string[]>([]);
  const [assignees, setAssignees] = useState<{ login: string; id: string }[]>([]);
  const [template, setTemplate] = useState("");
  const [trackerMenu, setTrackerMenu] = useState<{ x: number; y: number } | null>(null);
  const [templateMenu, setTemplateMenu] = useState<{ x: number; y: number } | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The service's own sentence when a create's outcome is unknown: it is shown, and only Check the list and Cancel remain.
  const [unknown, setUnknown] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const locked = useRef(false);
  const ids = useId();
  const scope = scopeOf(picked);

  // The two reads the dialog needs to know what this tracker offers, run while it is open and never with the list.
  const templates = useAsync(`issues:templates:${version}:${picked.tracker}:${scope}`, (signal) => issuesApi().templates({ tracker: picked.tracker, scope }, signal));
  const candidates = useAsync(`issues:candidates:${version}:${picked.tracker}:${scope}`, (signal) => issuesApi().candidates({ tracker: picked.tracker, scope }, signal));
  const offersTemplates = templates.status === "ready" && templates.data.length > 0;
  // Hidden only when the service says this tracker cannot fill them; a read that failed for another reason keeps the rows, and the
  // picker itself says what failed and offers Retry.
  const offersPickers = candidates.status === "ready" || (candidates.status === "error" && !noCandidates(candidates.error));

  const titleProblem = title.trim() === "" ? null : createProblem({ title });
  const bodyProblem = byteLength(body) > BODY_MAX ? `The description is over ${BODY_MAX / 1024} KiB.` : null;
  const ready = title.trim() !== "" && titleProblem === null && bodyProblem === null;

  const pickTracker = (row: TrackerRow) => {
    if (row.tracker === picked.tracker) return;
    setPicked(row);
    setTemplate("");
    setLabels([]);
    setAssignees([]);
    setError(null);
    setUnknown(null);
  };
  const pickTemplate = (id: string) => {
    const before = (templates.status === "ready" ? templates.data : []).find((one) => one.id === template)?.body ?? "";
    setTemplate(id);
    const chosen = (templates.status === "ready" ? templates.data : []).find((one) => one.id === id);
    // A template fills a missing description only: text the person typed is never replaced, and its labels are never merged in.
    if (chosen && (body === "" || body === before)) setBody(chosen.body);
  };
  const checkList = () => {
    // The one answer that can mean the issue exists: the list is re-read, and this draft is never sent again.
    onCheckList();
  };
  const submit = async () => {
    if (busy || unknown !== null || locked.current || !ready) return;
    locked.current = true;
    setBusy(true);
    setError(null);
    try {
      const created = await issuesApi().create({
        tracker: picked.tracker,
        scope,
        title,
        ...(body !== "" ? { body } : {}),
        ...(labels.length > 0 ? { labels } : {}),
        ...(assignees.length > 0 ? { assignees: assignees.map((who) => who.id) } : {}),
        ...(template !== "" ? { template } : {}),
      });
      onClose();
      navigate({ name: "issues", tracker: created.tracker, scope: created.scope, key: created.key });
    } catch (failure) {
      if (outcomeUnknown(failure)) {
        // The service's own sentence is the screen's: it says the issue was possibly created, so nothing re-sends the draft.
        setUnknown(reasonOf(failure));
      } else if (failure instanceof ApiFailure && routeAbsent(failure.status, failure.code, failure.message)) {
        markNotOffered("create");
        onClose();
      } else {
        setError(`${reasonOf(failure)} Nothing changed.`);
      }
    } finally {
      locked.current = false;
      setBusy(false);
    }
  };

  const choose = (kind: "labels" | "assignees", save: (pick: PickChoice) => boolean | Promise<boolean>, current: string[]) => (
    <Picker
      kind={kind}
      target={{ tracker: picked.tracker, scope }}
      current={current}
      mine={{ labels: [], assignees: [] }}
      offered
      // A create whose outcome is unknown has no write left to make: the picks stay on screen and no picker opens.
      busy={busy || unknown !== null}
      label={kind === "labels" ? "Choose labels" : "Choose assignees"}
      heading={kind === "labels" ? "Choose labels" : "Choose assignees"}
      save={save}
    />
  );

  return (
    <Dialog title="New issue" className="iss-dialog" busy={busy} onClose={onClose} footer={unknown !== null ? (
      <>
        <Button onClick={onClose} disabled={busy}>Cancel</Button>
        <Button tone="primary" onClick={checkList}>Check the list</Button>
      </>
    ) : (
      <>
        <Button onClick={onClose} disabled={busy}>Cancel</Button>
        <Button tone="primary" busy={busy} disabled={!ready} onClick={() => void submit()}>Create issue</Button>
      </>
    )}>
      <div className="iss-new">
        <div className="field">
          <span className="iss-new-label">Tracker</span>
          <button type="button" className="iss-new-select" aria-label={`Tracker: ${TRACKER_NAMES[picked.tracker]} · ${scope}`} aria-haspopup="menu" aria-expanded={trackerMenu !== null} disabled={busy || unknown !== null} onClick={(event) => {
            const rect = event.currentTarget.getBoundingClientRect();
            setTrackerMenu(trackerMenu ? null : { x: rect.left, y: rect.bottom + 4 });
          }}>
            <span className="iss-badge" aria-hidden>{badgeOf(picked.tracker)}</span>
            <span className="iss-new-select-value">{TRACKER_NAMES[picked.tracker]} · {scope}</span>
            <ChevronDown size={12} aria-hidden />
          </button>
        </div>
        {offersTemplates ? (
          <Field label="Template" hint="Fills the description if it is empty.">
            {(props) => (
              <button
                {...props}
                type="button"
                className="iss-new-select"
                aria-label={`Template: ${(templates.status === "ready" ? templates.data : []).find((one) => one.id === template)?.name ?? "Blank"}`}
                aria-haspopup="menu"
                aria-expanded={templateMenu !== null}
                disabled={busy || unknown !== null}
                onClick={(event) => {
                  const rect = event.currentTarget.getBoundingClientRect();
                  setTemplateMenu(templateMenu ? null : { x: rect.left, y: rect.bottom + 4 });
                }}
              >
                <span className="iss-new-select-value">{(templates.status === "ready" ? templates.data : []).find((one) => one.id === template)?.name ?? "Blank"}</span>
                <ChevronDown size={12} aria-hidden />
              </button>
            )}
          </Field>
        ) : null}
        <Field label="Title" error={titleProblem}>
          {(props) => <input {...props} data-dialog-focus type="text" placeholder="A one-line title" value={title} disabled={busy || unknown !== null} onChange={(event) => setTitle(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") void submit(); }} />}
        </Field>
        <Field label="Description" error={bodyProblem}>
          {(props) => <textarea {...props} rows={6} placeholder="Add a description." value={body} disabled={busy || unknown !== null} onChange={(event) => setBody(event.target.value)} />}
        </Field>
        {offersPickers ? (
          <div className="field" role="group" aria-labelledby={`${ids}-labels`}>
            <span className="iss-new-label" id={`${ids}-labels`}>Labels</span>
            <div className="iss-new-chosen">
              {labels.length === 0 ? <span className="iss-quiet">None</span> : labels.map((name) => <span key={name} className="iss-label">{name}</span>)}
              {choose("labels", (choice) => { setLabels(choice.labels ?? []); return true; }, labels)}
            </div>
          </div>
        ) : null}
        {offersPickers ? (
          <div className="field" role="group" aria-labelledby={`${ids}-assignees`}>
            <span className="iss-new-label" id={`${ids}-assignees`}>Assignees</span>
            <div className="iss-new-chosen">
              {assignees.length === 0 ? <span className="iss-quiet">None</span> : assignees.map((who) => <Person key={who.login} login={who.login} />)}
              {choose("assignees", (choice) => { setAssignees(choice.assignees ?? []); return true; }, assignees.map((who) => who.login))}
            </div>
          </div>
        ) : null}
        {unknown !== null ? <p className="iss-new-error" role="alert">{unknown}</p> : null}
        {error ? <p className="iss-new-error" role="alert">{error}</p> : null}
      </div>
      <Presence>
        {trackerMenu ? (
          <Menu
            x={trackerMenu.x}
            y={trackerMenu.y}
            label="Tracker"
            minWidth={240}
            onClose={() => setTrackerMenu(null)}
            items={rows.map((row) => ({
              label: `${TRACKER_NAMES[row.tracker]} · ${scopeOf(row)}`,
              selected: row.tracker === picked.tracker,
              leadCheck: true,
              onSelect: () => pickTracker(row),
            }))}
          />
        ) : null}
      </Presence>
      <Presence>
        {templateMenu ? (
          <Menu
            x={templateMenu.x}
            y={templateMenu.y}
            label="Template"
            minWidth={220}
            onClose={() => setTemplateMenu(null)}
            items={[
              { label: "Blank", selected: template === "", leadCheck: true, onSelect: () => pickTemplate("") },
              ...(templates.status === "ready" ? templates.data : []).map((one) => ({
                label: one.name,
                selected: template === one.id,
                leadCheck: true,
                onSelect: () => pickTemplate(one.id),
              })),
            ]}
          />
        ) : null}
      </Presence>
    </Dialog>
  );
}
