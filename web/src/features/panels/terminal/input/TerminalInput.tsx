import { History, Sparkles, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { api } from "../../../../api/client";
import { toFailure } from "../../../../lib/useAsync";
import { routeHref } from "../../../../product/state/route";
import { useUi } from "../../../../product/state/ui";
import { useWorkspace } from "../../../../product/state/workspace";
import { controlsOf } from "../terminalControls";
import { useReportedCommands } from "../shellOutcomes";
import { completeAt, applyCompletion, resolveDir } from "./completions";
import { HistoryPop, type HistoryScope } from "./HistoryPop";
import { historyAcross, recordCommand, recordExitCode, suggestion, useHistory } from "./history";
import { tokenize } from "./highlight";
import { ProposalCard, useProposal } from "./ProposalCard";
import { routeLine, type Mode } from "./route";
import { sendPrompt, takeBack, type Sent } from "./sendPrompt";
import { readEngineTarget, useEngineTarget, usePathListing, useShellDir } from "./useInputContext";

const HOLD_LINES = 4;
const HOLD_CHARS = 2000;
const UNDO_MS = 10_000;
/** A listing that is read quickly never flashes a row. */
const LOADING_AFTER_MS = 400;
const MODES: { mode: Mode; label: string }[] = [
  { mode: "suggest", label: "Suggest" },
  { mode: "shell", label: "Shell" },
  { mode: "engine", label: "Engine" },
];

/** The line an editor holds: typed text, or a long paste kept aside until it is reviewed and run. */
const isLong = (text: string) => text.split("\n").length > HOLD_LINES || text.length > HOLD_CHARS;

/**
 * The editor under a terminal: a real multi-line field with colour, history search and completions (W2), and a
 * local check that says where Enter will send the line, the shell or the thread's engine (W3). The decision is
 * always on screen, and an unclear line goes to the shell.
 */
export function TerminalInput({ threadId, keyOf }: { threadId: string; keyOf: string }) {
  const [mode, setMode] = useState<Mode>("suggest");
  const [line, setLine] = useState("");
  const [cursor, setCursor] = useState(0);
  const [held, setHeld] = useState<string | null>(null);
  const [reviewing, setReviewing] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const [keepInShell, setKeepInShell] = useState(false);
  const [picked, setPicked] = useState(-1);
  const [dismissed, setDismissed] = useState(false);
  const [explicit, setExplicit] = useState(false);
  const [searching, setSearching] = useState<HistoryScope | null>(null);
  const [sent, setSent] = useState<{ text: string; to: Sent } | null>(null);
  const [askedRun, setAskedRun] = useState<string | null>(null);
  const [dismissedProposal, setDismissedProposal] = useState<string | null>(null);
  const [slow, setSlow] = useState(false);
  const field = useRef<HTMLTextAreaElement>(null);
  const sending = useRef(false);
  const undoing = useRef(false);
  const retrying = useRef(false);
  const ranProposal = useRef<string | null>(null);
  const thread = useWorkspace((state) => state.threads[threadId]);
  const projectThreads = useWorkspace((state) => state.threads);
  const pushToast = useUi((state) => state.pushToast);
  const engine = useEngineTarget(threadId);
  const offered = useProposal(threadId, askedRun);

  const history = useHistory(keyOf);
  /**
   * How the shell said the lines it finished ended, from its own marks. Each report is given to the oldest
   * line of this terminal still unknown that carries the same command, so a row shows a result only when the
   * shell reported one. A shell that emits no marks reports nothing and the rows stay as they were.
   */
  const reported = useReportedCommands(keyOf);
  useEffect(() => {
    for (const command of reported) {
      if (command.exitCode !== null) recordExitCode(keyOf, command.command, command.exitCode);
    }
  }, [reported, keyOf]);
  const full = held === null ? line : line === "" ? held : `${held}\n${line}`;
  const route = useMemo(() => routeLine(mode, full, engine, keepInShell), [mode, full, engine, keepInShell]);
  const tokens = useMemo(() => (route.target === "engine" ? [] : tokenize(line)), [route.target, line]);

  const word = useMemo(() => line.slice(0, cursor).split(/\s/).pop() ?? "", [line, cursor]);
  // Relative paths start from the folder the shell is in, when it has said (OSC 7) and it is inside the worktree.
  const cwd = useShellDir(threadId, keyOf);
  const wantDir = resolveDir(cwd, word.includes("/") ? word.slice(0, word.lastIndexOf("/") + 1) : "");
  const { listDir, state: listing } = usePathListing(threadId, wantDir);
  const completion = useMemo(
    () => (held !== null || searching !== null || dismissed || route.target === "engine" ? null : completeAt(line, cursor, { history: history.map((entry) => entry.command), listDir, cwd }, explicit)),
    [line, cursor, held, searching, dismissed, route.target, history, listDir, cwd, explicit],
  );
  const ghost = mode !== "engine" && cursor === line.length && held === null ? suggestion(history, line) : "";
  const projectIds = useMemo(() => {
    const project = projectThreads[threadId]?.project_id;
    return Object.values(projectThreads).filter((entry) => entry.project_id === project).map((entry) => entry.id);
  }, [projectThreads, threadId]);
  // Only paths inside the worktree are listed, so only those can be waited for.
  const waitingOnPath = completion === null && searching === null && held === null && !dismissed && route.target !== "engine" && word.includes("/") && !word.startsWith("/") && !word.startsWith("~") && listing !== "idle";

  // A new thread or terminal starts clean; the timer ends the Undo offer.
  useEffect(() => {
    if (!sent) return;
    const timer = setTimeout(() => setSent(null), UNDO_MS);
    return () => clearTimeout(timer);
  }, [sent]);

  useEffect(() => {
    if (!waitingOnPath || listing !== "loading") return void setSlow(false);
    const timer = setTimeout(() => setSlow(true), LOADING_AFTER_MS);
    return () => clearTimeout(timer);
  }, [waitingOnPath, listing]);

  const edit = (value: string, at = value.length) => {
    setLine(value);
    setCursor(at);
    setPicked(-1);
    setDismissed(false);
    setExplicit(false);
    setConfirmed(false);
    setKeepInShell(false);
  };
  const clear = () => {
    edit("");
    setHeld(null);
    setReviewing(false);
  };

  const runInShell = () => {
    const controls = controlsOf(keyOf);
    if (!controls) return;
    controls.write(`${full.replace(/\n/g, "\r")}\r`);
    recordCommand(keyOf, full);
    clear();
  };

  const sendToEngine = async () => {
    if (sending.current || !thread) return;
    sending.current = true;
    const text = full.trim();
    try {
      const to = await sendPrompt(thread, text);
      setSent({ text: full, to });
      setAskedRun(to.kind === "run" ? to.id : null);
      clear();
      void useWorkspace.getState().refreshThread(threadId);
    } catch (failure) {
      // The text stays where it is, so nothing is lost and nothing ran.
      pushToast(`Couldn't send it. ${toFailure(failure).message}`);
    } finally {
      sending.current = false;
    }
  };

  const undo = async () => {
    if (!sent || undoing.current) return;
    undoing.current = true;
    const back = sent;
    const stopped = back.to.kind === "run";
    setSent(null);
    try {
      await takeBack(back.to);
      if (stopped) setAskedRun(null);
      pushToast(`${stopped ? "Stopped the run that started" : "Took it out of the queue"}. Your text is back in the line.`);
      edit(back.text);
      field.current?.focus();
      void useWorkspace.getState().refreshThread(threadId);
    } catch (failure) {
      // It was not taken back, so the offer stays and nothing is claimed.
      setSent(back);
      pushToast(`${stopped ? "Couldn't stop the run" : "Couldn't take it out of the queue"}. ${toFailure(failure).message}`);
    } finally {
      undoing.current = false;
    }
  };

  const signIn = async () => {
    try {
      await navigator.clipboard.writeText(engine.signInCommand ?? "");
      pushToast(`Copied the ${engine.name} sign-in command. Run it in a terminal.`);
    } catch {
      pushToast("Couldn't copy the command.");
    }
  };

  /** Checks the engine once and sends the line only if it is now reachable and needs no second Enter. */
  const tryAgain = async () => {
    if (retrying.current) return;
    retrying.current = true;
    try {
      await api.refreshEngines();
      await useWorkspace.getState().refreshEngines();
      const fresh = readEngineTarget(threadId);
      const next = routeLine(mode, full, fresh, keepInShell);
      if (next.target === "blocked") pushToast(`${fresh.name} still can't be reached.`);
      else if (next.target === "engine" && !next.confirm) await sendToEngine();
    } catch (failure) {
      pushToast(`Couldn't check ${engine.name}. ${toFailure(failure).message}`);
    } finally {
      retrying.current = false;
    }
  };

  const runProposal = () => {
    if (!offered || offered.proposal.secret || ranProposal.current === offered.key) return;
    const controls = controlsOf(keyOf);
    if (!controls) return void pushToast("This terminal isn't open, so nothing was run.");
    ranProposal.current = offered.key;
    controls.write(`${offered.proposal.command}\r`);
    recordCommand(keyOf, offered.proposal.command);
    setDismissedProposal(offered.key);
  };
  const editProposal = () => {
    if (!offered || offered.proposal.secret) return;
    setDismissedProposal(offered.key);
    edit(offered.proposal.command);
    field.current?.focus();
  };

  const submit = () => {
    if (full.trim() === "" || route.target === "blocked") return;
    if (held !== null && !reviewing) return void setReviewing(true);
    if (route.target === "engine") {
      if (route.confirm && !confirmed) return void setConfirmed(true);
      void sendToEngine();
    } else runInShell();
  };

  const insert = (index: number) => {
    const item = completion?.items[index];
    if (!completion || !item) return;
    const next = applyCompletion(line, completion, item);
    edit(next.line, next.cursor);
    requestAnimationFrame(() => field.current?.setSelectionRange(next.cursor, next.cursor));
  };

  const onKey = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing) return;
    const meta = event.metaKey || event.ctrlKey;
    if (meta && event.key === "/") {
      event.preventDefault();
      setMode(MODES[(MODES.findIndex((entry) => entry.mode === mode) + 1) % MODES.length]?.mode ?? "suggest");
    } else if (event.ctrlKey && event.key.toLowerCase() === "r") {
      event.preventDefault();
      setSearching(event.shiftKey ? "project" : "shell");
    } else if (meta && event.key.toLowerCase() === "z" && sent && line === "") {
      event.preventDefault();
      void undo();
    } else if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (picked >= 0 && completion) insert(picked);
      else submit();
    } else if (event.key === "Tab" && !event.shiftKey) {
      event.preventDefault();
      if (completion) insert(Math.max(picked, 0));
      else if (!explicit) setExplicit(true);
    } else if (event.key === "ArrowRight" && ghost && field.current?.selectionStart === line.length) {
      event.preventDefault();
      edit(line + ghost);
    } else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && completion) {
      event.preventDefault();
      const count = completion.items.length;
      const down = event.key === "ArrowDown";
      setPicked(picked < 0 ? (down ? 0 : count - 1) : (picked + (down ? 1 : -1) + count) % count);
    } else if (event.key === "Escape") {
      if (completion) setDismissed(true);
      else if (route.target === "engine" && mode === "suggest") setKeepInShell(true);
      else if (held !== null && reviewing) setReviewing(false);
    } else if (event.key === "Backspace" && line === "" && held !== null) {
      event.preventDefault();
      setHeld(null);
      setReviewing(false);
    }
  };

  return (
    <div className="term-input" data-target={route.target}>
      {sent ? (
        <div className="term-input-undo" role="status">
          <Sparkles size={14} aria-hidden />
          <span>
            {sent.to.kind === "run" ? `Sent to ${engine.name} as a prompt. Undo stops the run that started.` : "Queued behind the current run. Undo takes it out of the queue."}
          </span>
          <button type="button" className="text-action" onClick={() => void undo()}>
            Undo
          </button>
        </div>
      ) : null}
      <div className="term-input-head">
        <div role="group" aria-label="Where Enter sends the line" className="term-input-modes">
          {MODES.map((entry) => (
            <button key={entry.mode} type="button" aria-pressed={mode === entry.mode} onClick={() => setMode(entry.mode)}>
              {entry.label}
            </button>
          ))}
        </div>
        <span className="term-input-chip" data-target={route.target}>
          {route.target === "engine" ? `Prompt to ${engine.name}` : route.target === "blocked" ? "Not sent" : "Shell"}
        </span>
        <button type="button" className="text-action term-input-history" onClick={() => setSearching("shell")}>
          <History size={14} aria-hidden /> History
        </button>
      </div>
      {held !== null ? (
        <div className="term-input-held">
          <span>
            Pasted {held.split("\n").length} lines, {held.length} characters
          </span>
          <button type="button" aria-label="Remove the pasted text" className="text-action" onClick={() => { setHeld(null); setReviewing(false); }}>
            <X size={14} aria-hidden />
          </button>
          <pre>{reviewing ? held : held.split("\n").slice(0, 2).join("\n")}</pre>
          {reviewing ? <p>Press Enter again to run all of it.</p> : null}
        </div>
      ) : null}
      <div className="term-input-edit" data-engine={route.target === "engine" ? "true" : undefined}>
        <span className="term-input-prompt" aria-hidden>
          {route.target === "engine" ? <Sparkles size={14} /> : "$"}
        </span>
        <div className="term-input-stack">
          <pre className="term-input-paint" aria-hidden>
            {tokens.map((token, at) => (
              <span key={at} data-kind={token.kind}>
                {token.text}
              </span>
            ))}
            <span className="term-input-ghost">{ghost}</span>
            {"\n"}
          </pre>
          <textarea
            ref={field}
            aria-label="Terminal input"
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            rows={Math.min(6, Math.max(1, line.split("\n").length))}
            value={line}
            placeholder="Type a command, or ask a question"
            onChange={(event) => edit(event.target.value, event.target.selectionStart)}
            onSelect={(event) => setCursor(event.currentTarget.selectionStart)}
            onKeyDown={onKey}
            onPaste={(event) => {
              const text = event.clipboardData.getData("text");
              if (!isLong(text)) return;
              event.preventDefault();
              setHeld(held === null ? text : `${held}\n${text}`);
              setReviewing(false);
            }}
          />
        </div>
      </div>
      {searching !== null ? (
        <HistoryPop
          shell={history}
          project={historyAcross(projectIds)}
          scope={searching}
          onPick={(command) => { setSearching(null); edit(command); field.current?.focus(); }}
          onClose={() => { setSearching(null); field.current?.focus(); }}
        />
      ) : completion ? (
        <ul className="term-input-pop" role="listbox" aria-label="Completions">
          {completion.items.map((item, at) => (
            <li key={`${item.kind}:${item.insert}`} role="option" aria-selected={at === picked} onMouseDown={(event) => { event.preventDefault(); insert(at); }}>
              <code>{item.label}</code>
              <span>{item.detail}</span>
            </li>
          ))}
        </ul>
      ) : waitingOnPath && (slow || listing === "failed") ? (
        <div className="term-input-pop term-input-wait" role="status">
          <p>{listing === "failed" ? "Couldn't read this folder" : "Reading files…"}</p>
          <span>Enter runs the line as typed</span>
        </div>
      ) : null}
      {offered && offered.key !== dismissedProposal ? (
        <ProposalCard engine={engine.name} proposal={offered.proposal} busy={ranProposal.current === offered.key} onRun={runProposal} onEdit={editProposal} onCancel={() => setDismissedProposal(offered.key)} />
      ) : null}
      <p className="term-input-note" role="status" aria-live="polite">
        {route.note}
        {route.target === "blocked" && route.why === "offline" ? (
          <button type="button" className="text-action" onClick={() => void tryAgain()}>
            Try again
          </button>
        ) : null}
        {route.target === "blocked" && route.why === "signed-out" ? (
          engine.signInCommand ? (
            <button type="button" className="text-action" onClick={() => void signIn()}>
              Sign in
            </button>
          ) : (
            <a className="text-action" href={routeHref({ name: "settings", section: "providers" })}>
              Sign in
            </a>
          )
        ) : null}
        {route.target === "blocked" && route.canRunInShell && route.why !== "secret" ? (
          <button type="button" className="text-action" onClick={() => { setMode("shell"); }}>
            Run in the shell instead
          </button>
        ) : null}
        <span className="term-input-hints">
          {route.target === "engine" ? "Enter sends" : "Enter runs"} · Shift+Enter new line · Tab complete · Ctrl+R history · Ctrl+/ mode
        </span>
      </p>
    </div>
  );
}
