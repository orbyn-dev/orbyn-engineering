// Settings › Crew › Path classes (Crew Designs frames `classes-*`): what `.orbyn/crew.json` says about which engines may take which
// paths, read at the commit a task would start from. Read-only: Orbyn never writes the file, so there is no Save, Add or Delete, and
// the service sends no lock flag, no sensitivity label and no worktree owner, so none is drawn.
import { useEffect, useMemo, useRef, useState } from "react";
import { toFailure, useAsync } from "../../../../lib/useAsync";
import { useRoute } from "../../../../product/state/route";
import { useWorkspace } from "../../../../product/state/workspace";
import type { ApiFailure } from "../../../../api/client";
import type { CrewClassEngine } from "../../../../generated/CrewClassEngine";
import type { CrewClassRule } from "../../../../generated/CrewClassRule";
import type { CrewClassTest } from "../../../../generated/CrewClassTest";
import type { GuardTest } from "../../../../generated/GuardTest";
import type { GuardVerdict } from "../../../../generated/GuardVerdict";
import type { CrewClassTestPath } from "../../../../generated/CrewClassTestPath";
import { Button } from "../../../../ui/controls";
import { ErrorState, Loading } from "../../../../product/ui/states";
import { crewApi } from "./api";
import { GUARD_WORKERS } from "./copy";
import { Chip } from "./parts";

const INTRO =
  "Path classes sort files by sensitivity and list the engines allowed for each. They are blocking rules: Crew applies them before it scores any engine, and every routing decision is in the ledger. A task that touches two classes can use the engines both classes allow. They are stored in .orbyn/crew.json in this repo, and that file is the authority. Orbyn reads it and never writes it: a change is made in the file’s editor and is a normal reviewed change to the repo, with no separate owner role. Agents can never edit the file (it is on the always-denied list).";

/** The most paths one test takes (the service refuses more). */
export const TEST_PATHS = 10;
const DEBOUNCE_MS = 300;

/** An engine as the service names it, by its id. One this machine does not have is kept in the file and said so. */
export const engineName = (engine: CrewClassEngine) => (engine.on_this_machine ? engine.id : `${engine.id} (not on this machine)`);

/** The engines a rule allows, in words: none, or the list. A path in no rule has any signed-in engine. */
export const enginesText = (engines: CrewClassEngine[] | null | undefined) => (engines === null || engines === undefined ? "Any signed-in engine" : engines.length === 0 ? "No engine" : engines.map(engineName).join(", "));

/** The project the rules are read for: the project of the checkout Settings is showing, else the first project, named on the panel. */
export function useClassProject(): { id: string; name: string } | null {
  const route = useRoute();
  const projects = useWorkspace((state) => state.projects.data);
  const chosen = route.name === "settings" && route.selection?.kind === "checkout" ? route.selection.project_id : null;
  const project = (chosen ? projects?.find((one) => one.id === chosen) : undefined) ?? projects?.[0];
  return project ? { id: project.id, name: project.name } : null;
}

function Patterns({ paths }: { paths: string[] }) {
  return <span className="ap-glyphs">{paths.map((path, at) => <code key={`${at}-${path}`}>{path}</code>)}</span>;
}

function Table({ view, selected, onSelect }: { view: { built_in: string; rules: CrewClassRule[]; problems: string[] }; selected: string | null; onSelect(name: string): void }) {
  return (
    <div className="ap-targets" role="table" aria-label="Path classes">
      <div className="ap-class-row ap-head" role="row">
        <span role="columnheader">Class</span>
        <span role="columnheader">Paths</span>
        <span role="columnheader">Allowed engines</span>
      </div>
      {view.rules.map((rule, at) => (
        <div key={`${at}-${rule.name}`} className={`ap-class-row${selected === rule.name ? " ap-sel" : ""}`} role="row">
          <span role="cell"><button type="button" className="ap-open" aria-pressed={selected === rule.name} onClick={() => onSelect(rule.name)}><b>{rule.name}</b></button></span>
          <span role="cell"><Patterns paths={rule.paths} /></span>
          <span role="cell" data-label="Allowed">{enginesText(rule.engines)}</span>
        </div>
      ))}
      <div className="ap-class-row" role="row">
        <span role="cell"><b>{view.built_in}</b><small>Built in</small></span>
        <span role="cell">Any path not matched above</span>
        {/* With a file that can't be used every engine is blocked, built-in class included. */}
        <span role="cell" data-label="Allowed">{view.problems.length ? "No engine" : "Any signed-in engine"}</span>
      </div>
    </div>
  );
}

function Detail({ rule, at, onClose }: { rule: CrewClassRule; at: number; onClose(): void }) {
  const ref = useRef<HTMLElement>(null);
  useEffect(() => ref.current?.focus(), [rule.name]);
  const engines = enginesText(rule.engines);
  return (
    <aside ref={ref} tabIndex={-1} className="ap-drawer ap-class-detail" aria-label={`The class ${rule.name}`} onKeyDown={(event) => { if (event.key === "Escape") { event.stopPropagation(); onClose(); } }}>
      <header className="ap-drawer-head"><h3>The class {rule.name}</h3></header>
      <div className="ap-drawer-body">
        <dl className="ap-facts">
          <div><dt>Name</dt><dd>{rule.name}</dd></div>
          <div><dt>Where</dt><dd>Rule {at} in .orbyn/crew.json</dd></div>
          <div><dt>Paths</dt><dd><Patterns paths={rule.paths} /></dd></div>
          <div><dt>Allowed engines</dt><dd>{engines}</dd></div>
          <div><dt>Reason</dt><dd>{rule.reason}</dd></div>
        </dl>
        <p className="ap-muted">Orbyn reads this file and never writes it. To change the class, open the file in the editor: the change is a normal reviewed change to the repo, and Orbyn reads it again when it is saved.</p>
      </div>
      <footer className="ap-drawer-foot"><Button small tone="quiet" onClick={onClose}>Close</Button></footer>
    </aside>
  );
}

function Problems({ problems, onCheck }: { problems: string[]; onCheck(): void }) {
  return (
    <>
      <div className="ap-banner ap-banner-am" role="alert">
        <p><b>Orbyn can’t use .orbyn/crew.json yet</b><br />It found {problems.length} {problems.length === 1 ? "problem" : "problems"}. Until they are fixed, every engine is blocked by the rule “crew.json invalid”, nothing starts, and a task that needs routing waits in Needs you.</p>
        <span className="ap-banner-acts"><Button small onClick={onCheck}>Check again</Button></span>
      </div>
      <div className="ap-targets" role="table" aria-label="What is wrong in the file">
        <div className="ap-destroy-row ap-head" role="row"><span role="columnheader">What is wrong</span></div>
        {problems.map((problem, at) => <div key={`${at}-${problem}`} className="ap-class-problem" role="row"><span role="cell">{problem}</span></div>)}
      </div>
      <p className="ap-muted">Orbyn reads the file and never writes it. Fix the rules in the editor, then Check again; nothing is changed until the file is valid, and a mistake never silently becomes a rule.</p>
    </>
  );
}

/** One path's answer, and what the Route card says of it. */
function Answer({ path }: { path: CrewClassTestPath }) {
  if (!path.valid) {
    return (
      <>
        <dl className="ap-facts"><div><dt>Path</dt><dd>{path.path}</dd></div><div><dt>Problem</dt><dd>{path.problem}</dd></div></dl>
        <p className="ap-muted">{path.route_card}</p>
      </>
    );
  }
  return (
    <>
      <dl className="ap-facts">
        <div><dt>Path</dt><dd>{path.path}</dd></div>
        <div><dt>Matches</dt><dd>{path.classes.length ? path.classes.map((one) => `${one.name}, by ${one.pattern}`).join("; ") : "Everything else"}</dd></div>
        <div><dt>Allowed</dt><dd>{enginesText(path.allowed_engines)}</dd></div>
        <div><dt>Logged as</dt><dd>A routing decision, in the ledger, with the rule</dd></div>
      </dl>
      <p className="ap-muted">{path.route_card}</p>
    </>
  );
}

function TestBox({ project, onResult }: { project: string; onResult(has: boolean): void }) {
  const [text, setText] = useState("");
  const [state, setState] = useState<{ status: "idle" } | { status: "error"; failure: ApiFailure } | { status: "ready"; test: CrewClassTest }>({ status: "idle" });
  const paths = useMemo(() => text.split("\n").map((one) => one.trim()).filter(Boolean).slice(0, TEST_PATHS), [text]);
  const key = paths.join("\n");
  useEffect(() => {
    if (!key) {
      setState({ status: "idle" });
      return;
    }
    const abort = new AbortController();
    const timer = setTimeout(() => {
      crewApi.classesTest(project, key.split("\n"), abort.signal).then(
        (test) => setState({ status: "ready", test }),
        (error: unknown) => { if (!abort.signal.aborted) setState({ status: "error", failure: toFailure(error) }); },
      );
    }, DEBOUNCE_MS);
    return () => { clearTimeout(timer); abort.abort(); };
  }, [project, key]);
  useEffect(() => onResult(state.status === "ready"), [state.status, onResult]);
  const results = state.status === "ready" ? state.test.paths : [];
  return (
    <section className="ap-box">
      <h4>Which class does this path match?</h4>
      <textarea className="ap-search ap-class-input" aria-label="Paths to test, one per line" placeholder="auth/redirect.ts" rows={2} autoComplete="off" spellCheck={false} value={text} onChange={(event) => setText(event.target.value)} />
      {state.status === "error" ? <ErrorState title="Can't test the path" failure={state.failure} onRetry={() => setText((now) => `${now} `)} /> : null}
      {results.map((path, at) => <Answer key={`${at}-${path.path}`} path={path} />)}
    </section>
  );
}

const VERDICT: Record<GuardVerdict, string> = {
  not_destructive: "Not a destructive command the guard knows",
  local: "Destructive, but every target is local",
  exempt: "Not guarded: listed in safe_targets",
  guarded: "Guarded: it needs a second key, or you",
};

/** What the guard says about a command. It runs nothing and changes nothing. */
function CommandBox() {
  const [text, setText] = useState("");
  const [state, setState] = useState<{ status: "idle" } | { status: "error"; failure: ApiFailure } | { status: "ready"; test: GuardTest }>({ status: "idle" });
  const command = text.trim();
  useEffect(() => {
    if (!command) {
      setState({ status: "idle" });
      return;
    }
    const abort = new AbortController();
    const timer = setTimeout(() => {
      crewApi.guardTest(command, abort.signal).then(
        (test) => setState({ status: "ready", test }),
        (error: unknown) => { if (!abort.signal.aborted) setState({ status: "error", failure: toFailure(error) }); },
      );
    }, DEBOUNCE_MS);
    return () => { clearTimeout(timer); abort.abort(); };
  }, [command]);
  const test = state.status === "ready" ? state.test : null;
  return (
    <section className="ap-box">
      <h4>Command to test</h4>
      <textarea className="ap-search ap-class-input" aria-label="Command to test" placeholder='psql "$DATABASE_URL" -c "DROP TABLE x"' rows={2} autoComplete="off" spellCheck={false} value={text} onChange={(event) => setText(event.target.value)} />
      {state.status === "error" ? <ErrorState title="Can't test the command" failure={state.failure} onRetry={() => setText((now) => `${now} `)} /> : null}
      {test ? (
        <dl className="ap-facts">
          <div><dt>Result</dt><dd>{VERDICT[test.verdict]}</dd></div>
          {test.category ? <div><dt>Category</dt><dd>{test.category}</dd></div> : null}
          {test.matched_by ? <div><dt>Matched by</dt><dd>{test.matched_by}</dd></div> : null}
          {test.target ? <div><dt>Target</dt><dd>{test.target}</dd></div> : null}
          {test.matched_entry ? <div><dt>Listed in</dt><dd>{test.matched_entry.list}_targets: {test.matched_entry.pattern}</dd></div> : null}
        </dl>
      ) : null}
    </section>
  );
}

function Loaded({ project }: { project: { id: string; name: string } }) {
  const read = useAsync(`crew-classes:${project.id}`, (signal) => crewApi.classes(project.id, signal));
  const [selected, setSelected] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [answered, setAnswered] = useState(false);
  if (read.status === "loading") return <Loading label="Reading the path classes" rows={4} />;
  if (read.status === "error") return <ErrorState title="Can't read the path classes" failure={read.error} onRetry={read.reload} />;
  const view = read.data;
  const rule = view.rules.find((one) => one.name === selected) ?? null;
  const broken = view.problems.length > 0;
  const frame = broken ? "classes-error" : rule ? "classes-edit" : answered ? "classes-test" : "classes-list";
  return (
    <section className="ap-set" aria-labelledby="ap-classes-title" data-frame={frame}>
      <header className="ap-set-head">
        <h2 id="ap-classes-title">Crew › Path classes</h2>
        <Chip tone="plain">Preview</Chip>
      </header>
      <p className="ap-muted">Read for the project {project.name}.</p>
      <p className="ap-muted">{INTRO}</p>
      {view.working_copy_note ? <div className="ap-banner ap-banner-plain" role="note"><p>{view.working_copy_note}</p></div> : null}
      {broken ? <Problems problems={view.problems} onCheck={read.reload} /> : null}
      <div className="ap-class-split">
        <div className="ap-class-main">
          <Table view={view} selected={rule?.name ?? null} onSelect={(name) => setSelected(name === selected ? null : name)} />
          <div className="ap-class-acts"><Button small onClick={() => setTesting((now) => !now)} aria-expanded={testing}>Test a path</Button></div>
          {testing ? <><TestBox project={project.id} onResult={setAnswered} /><CommandBox /></> : null}
        </div>
        {rule ? <Detail rule={rule} at={view.rules.indexOf(rule) + 1} onClose={() => setSelected(null)} /> : null}
      </div>
      <p className="ap-muted">{GUARD_WORKERS}</p>
    </section>
  );
}

/** Settings › Crew › Path classes. Nothing is drawn while there is no project to read the file for. */
export function PathClasses() {
  const project = useClassProject();
  return project ? <Loaded project={project} /> : null;
}
