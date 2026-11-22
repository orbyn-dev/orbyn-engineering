import { useEffect, useRef, useState } from "react";
import { ApiFailure } from "../../api/client";
import { Button, Field } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { libraryApi } from "./api";
import type { Param, ProfileGrant, Prompt, Workflow } from "./contract";
import { capped, confirmFromFailure, isOneLine, plain, startValues, valueProblems, valuesBody, type RunConfirmation } from "./model";
import { RunConfirm } from "./RunConfirm";

export type UseMode = "run" | "terminal" | "composer" | "copy";

const FAILURE = new Map<string, string>([
  ["folder_untrusted", "This folder is not trusted yet. Trust it first, then run the command."],
  ["desktop_required", "Running a command needs the Orbyn desktop app."],
  ["not_found", "That item is gone. Reload the library."],
  ["invalid_input", "A value is not valid for this item. Check the fields."],
]);

/** The words on the main button of each mode. */
const VERB: Record<UseMode, string> = { run: "Run", terminal: "Insert into the terminal line", composer: "Insert into the composer", copy: "Copy" };

/**
 * One form for using a workflow or a prompt (frames `workflow-run`, `workflow-run-error`, `workflow-insert`): a field for each parameter,
 * and the exact text the service says it will use, with every value quoted for the shell by the service. The page never builds the
 * text itself and never runs it. The main button stays off until the service has rendered the text for the values on screen (the
 * latest request wins; an older answer is dropped), and until every required parameter has a valid value.
 *
 * Run is the one Person write: one click, one request, and a second click while it is in flight does nothing. Insert into the terminal
 * line writes the text over the existing terminal socket with no newline, so nothing runs until the person presses Enter there; a
 * command of more than one line is refused for that mode, because a newline in a terminal is Enter. Parameter values here are plain
 * text, not secrets: an environment value never comes into this form.
 */
export function UseDialog({ item, kind, mode, where, onRun, onInsert, onClose }: {
  item: Workflow | Prompt;
  kind: "workflow" | "prompt";
  mode: UseMode;
  /** Where a run goes: the thread whose checkout is open. */
  where: { thread_id: string };
  onRun(terminalId: string): void;
  /** For the three modes that do not call the service again: the rendered text goes where the person chose. Returns a note to show, or null. */
  onInsert(mode: Exclude<UseMode, "run">, text: string): Promise<string | null> | string | null;
  onClose(): void;
}) {
  const params: Param[] = item.params;
  const [values, setValues] = useState<Record<string, string>>(() => startValues(params));
  const [text, setText] = useState<{ for: string; text: string } | null>(null);
  const [renderFailure, setRenderFailure] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<RunConfirmation | null>(null);
  const turn = useRef(0);
  const problems = valueProblems(params, values);
  const key = JSON.stringify(valuesBody(params, values));

  // The service's answer for these values. Latest wins: a slow answer for older values is dropped, never shown.
  useEffect(() => {
    if (problems.size > 0) { setText(null); setRenderFailure(null); return; }
    const mine = ++turn.current;
    const controller = new AbortController();
    const timer = setTimeout(() => {
      const body = valuesBody(params, values);
      const asked = kind === "workflow"
        ? libraryApi().renderWorkflow(item.id, body, controller.signal).then((answer) => answer.command)
        : libraryApi().renderPrompt(item.id, body, controller.signal).then((answer) => answer.text);
      asked.then(
        (answer) => { if (mine === turn.current) { setText({ for: key, text: typeof answer === "string" ? answer : "" }); setRenderFailure(null); } },
        (error) => { if (mine === turn.current && !(error instanceof DOMException)) { setText(null); setRenderFailure(error instanceof ApiFailure ? plain(error.message, 160) : "Couldn’t build the text."); } },
      );
    }, 200);
    return () => { clearTimeout(timer); controller.abort(); };
  }, [key, item.id, kind]); // eslint-disable-line react-hooks/exhaustive-deps -- the values are in `key`

  const rendered = text && text.for === key ? text.text : null;
  const shown = rendered === null ? null : capped(rendered, kind === "workflow" ? 4200 : 33_000);
  const oneLine = rendered !== null && isOneLine(rendered);
  const blocked = mode === "terminal" && rendered !== null && !oneLine;
  const ready = rendered !== null && !blocked && problems.size === 0 && !busy && done === null;

  // `ack` is only ever what the service named on the 409 just before, handed back by the sheet; it is never kept.
  const go = async (ack: readonly string[] = []) => {
    if (pending.current || !ready || rendered === null) return;
    pending.current = true;
    setBusy(true);
    setFailure(null);
    try {
      if (mode === "run") {
        const answer = await libraryApi().runWorkflow(item.id, valuesBody(params, values), where, ack as ProfileGrant[]);
        onRun(plain(answer.terminal_id, 60));
        setDone("Started in a terminal. Its output is there.");
      } else {
        const note = await onInsert(mode, rendered);
        if (note) setDone(note);
        else onClose();
      }
    } catch (error) {
      const asked = mode === "run" && error instanceof ApiFailure && error.code === "confirm_required" ? confirmFromFailure(error.detail) : null;
      if (asked) setConfirm(asked);
      else if (mode === "run" && error instanceof ApiFailure && error.code === "confirm_required") setFailure("This run needs a confirmation this app can’t show, so nothing was started.");
      else setFailure(error instanceof ApiFailure ? (FAILURE.get(error.code) ?? `Couldn’t finish. ${plain(error.message, 160)}`) : "Couldn’t finish. Something went wrong.");
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const set = (name: string, value: string) => setValues((held) => ({ ...held, [name]: value }));
  return (
    <Dialog
      title={`${mode === "run" ? "Run" : kind === "prompt" ? "Insert" : "Use"} “${plain(item.name, 40)}”`}
      busy={busy}
      onClose={onClose}
      className="lib-dialog"
      footer={<>
        <Button onClick={onClose} disabled={busy}>{done ? "Close" : "Cancel"}</Button>
        {done ? null : <Button tone="primary" busy={busy} disabled={!ready} onClick={() => void go()}>{VERB[mode]}</Button>}
      </>}
    >
      <form className="lib-form" onSubmit={(event) => { event.preventDefault(); void go(); }}>
        {params.length === 0 ? <p className="field-hint">This {kind} has no parameters.</p> : <p className="field-hint">Fill in the parameters.</p>}
        {params.map((param, at) => (
          <Field key={param.name} label={plain(param.label || param.name, 40)} error={problems.get(param.name)}>
            {(props) => param.kind === "choice" ? (
              <select {...props} className="input" value={Object.hasOwn(values, param.name) ? values[param.name] : ""} data-dialog-focus={at === 0 || undefined} onChange={(event) => set(param.name, event.target.value)}>
                {param.required ? null : <option value="">Not set</option>}
                {(param.choices ?? []).map((choice) => <option key={choice} value={choice}>{plain(choice, 60)}</option>)}
              </select>
            ) : (
              <input {...props} type="text" className="input" autoComplete="off" spellCheck={false} value={Object.hasOwn(values, param.name) ? values[param.name] : ""} data-dialog-focus={at === 0 || undefined} onChange={(event) => set(param.name, event.target.value)} />
            )}
          </Field>
        ))}
        <div className="field">
          <span className="lib-label">{kind === "workflow" ? "Command that will run" : "Text that will be inserted"}</span>
          {shown ? <pre className="lib-pre" data-testid="rendered">{shown.text}{shown.cut ? "…" : ""}</pre> : <p className="field-hint">{problems.size > 0 ? "The text can’t be built until every parameter is valid." : renderFailure ?? "Building the text…"}</p>}
          {renderFailure && shown === null && problems.size === 0 ? <p className="field-error" role="alert">{renderFailure}</p> : null}
        </div>
        {kind === "workflow" && mode === "run" ? <p className="field-hint">It starts in a terminal for this thread’s checkout, like typing it there. Any environment sets it lists are applied by the run, and their values stay hidden.</p> : null}
        {mode === "terminal" ? <p className="field-hint">It is typed into the terminal line without Enter. Nothing runs until you press Enter there.</p> : null}
        {blocked ? <p className="field-error" role="alert">This command has more than one line, so it can’t be typed into the terminal line without running. Use Run…, or copy it.</p> : null}
        {failure ? <p className="field-error" role="alert">{failure}</p> : null}
        {done ? <p className="lib-done" role="status">{done}</p> : null}
      </form>
      {confirm ? <RunConfirm name={item.name} confirmation={confirm} onRun={(grants) => { setConfirm(null); void go(grants); }} onClose={() => setConfirm(null)} /> : null}
    </Dialog>
  );
}
