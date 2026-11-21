import { History, Sparkles, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { toFailure } from "../../../../lib/useAsync";
import { useUi } from "../../../../product/state/ui";
import { useWorkspace } from "../../../../product/state/workspace";
import { controlsOf } from "../terminalControls";
import { completeAt, applyCompletion } from "./completions";
import { historyOf, recordCommand, searchHistory, suggestion } from "./history";
import { tokenize } from "./highlight";
import { routeLine, type Mode } from "./route";
import { sendPrompt, takeBack, type Sent } from "./sendPrompt";
import { useEngineTarget, usePathListing } from "./useInputContext";

const HOLD_LINES = 4;
const HOLD_CHARS = 2000;
const UNDO_MS = 10_000;
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
  const [searching, setSearching] = useState<string | null>(null);
  const [searchAt, setSearchAt] = useState(0);
  const [sent, setSent] = useState<{ text: string; to: Sent } | null>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const sending = useRef(false);
  const thread = useWorkspace((state) => state.threads[threadId]);
  const pushToast = useUi((state) => state.pushToast);
  const engine = useEngineTarget(threadId);

  const history = historyOf(keyOf);
  const full = held === null ? line : line === "" ? held : `${held}\n${line}`;
  const route = useMemo(() => routeLine(mode, full, engine, keepInShell), [mode, full, engine, keepInShell]);
  const tokens = useMemo(() => (route.target === "engine" ? [] : tokenize(line)), [route.target, line]);

  const wantDir = useMemo(() => {
    const word = line.slice(0, cursor).split(/\s/).pop() ?? "";
    return word.includes("/") ? word.slice(0, word.lastIndexOf("/") + 1).replace(/^\.\//, "") : "";
  }, [line, cursor]);
  const listDir = usePathListing(threadId, wantDir);
  const completion = useMemo(
    () => (held !== null || searching !== null || dismissed || route.target === "engine" ? null : completeAt(line, cursor, { history: history.map((entry) => entry.command), listDir }, explicit)),
    [line, cursor, held, searching, dismissed, route.target, history, listDir, explicit],
  );
  const ghost = mode !== "engine" && cursor === line.length && held === null ? suggestion(history, line) : "";
  const found = useMemo(() => (searching === null ? [] : searchHistory(history, searching)), [searching, history]);

  // A new thread or terminal starts clean; the timer ends the Undo offer.
  useEffect(() => {
    if (!sent) return;
    const timer = setTimeout(() => setSent(null), UNDO_MS);
    return () => clearTimeout(timer);
  }, [sent]);

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
    if (!sent) return;
    const back = sent;
    setSent(null);
    try {
      await takeBack(back.to);
      edit(back.text);
      field.current?.focus();
      void useWorkspace.getState().refreshThread(threadId);
    } catch (failure) {
      pushToast(`Couldn't take it back. ${toFailure(failure).message}`);
    }
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
      setSearching("");
      setSearchAt(0);
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

  const onSearchKey = (event: KeyboardEvent<HTMLInputElement>) => {
    const row = found[searchAt];
    if (event.key === "Escape") {
      event.preventDefault();
      setSearching(null);
      field.current?.focus();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (found.length > 0) setSearchAt((searchAt + (event.key === "ArrowDown" ? 1 : found.length - 1)) % found.length);
    } else if ((event.key === "Enter" || event.key === "Tab") && row) {
      // The history row is the real command; the masked text is only what is shown.
      event.preventDefault();
      setSearching(null);
      edit(row.entry.command);
      field.current?.focus();
    }
  };

  return (
    <div className="term-input" data-target={route.target}>
      {sent ? (
        <div className="term-input-undo" role="status">
          <Sparkles size={14} aria-hidden />
          <span>Sent to {engine.name} as a prompt.</span>
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
        <button type="button" className="text-action term-input-history" onClick={() => { setSearching(""); setSearchAt(0); }}>
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
        <div className="term-input-pop" role="dialog" aria-label="Search history">
          <div className="term-input-scope">
            <span>This shell</span>
            <input autoFocus aria-label="Search history" value={searching} onChange={(event) => { setSearching(event.target.value); setSearchAt(0); }} onKeyDown={onSearchKey} />
          </div>
          {found.length === 0 ? <p className="term-input-none">{history.length === 0 ? "Nothing run from this editor yet." : "No match."}</p> : null}
          <ul role="listbox" aria-label="History">
            {found.map((row, at) => (
              <li key={row.entry.command} role="option" aria-selected={at === searchAt}>
                <code>
                  {row.match ? (
                    <>
                      {row.shown.slice(0, row.match[0])}
                      <b>{row.shown.slice(row.match[0], row.match[1])}</b>
                      {row.shown.slice(row.match[1])}
                    </>
                  ) : (
                    row.shown
                  )}
                </code>
              </li>
            ))}
          </ul>
        </div>
      ) : completion ? (
        <ul className="term-input-pop" role="listbox" aria-label="Completions">
          {completion.items.map((item, at) => (
            <li key={`${item.kind}:${item.insert}`} role="option" aria-selected={at === picked} onMouseDown={(event) => { event.preventDefault(); insert(at); }}>
              <code>{item.label}</code>
              <span>{item.detail}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <p className="term-input-note" role="status" aria-live="polite">
        {route.note}
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
