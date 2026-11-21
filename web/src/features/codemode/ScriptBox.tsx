import { useState } from "react";
import "./codemode.css";
import { LIMITS, scriptLines, scriptMeta, shortHash, sizeLabel } from "./model";

const EXCERPT_LINES = 5;

const num = (at: number, width: number) => String(at + 1).padStart(width, "0");

/**
 * A script as text: a few lines closed, the whole script in a box that scrolls when opened. A script is untrusted input, so
 * every line is a React text child: no markup, no links, nothing run. At most `LIMITS.scriptPage` lines are in the page at a
 * time; a longer script opens a page at a time, so a 64 KiB script never builds an unbounded tree.
 */
export function ScriptBox({ script, sha, defaultOpen = false }: { script: string; sha?: string | null; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const [pages, setPages] = useState(1);
  const lines = scriptLines(script);
  const meta = scriptMeta(script);
  const width = Math.max(2, String(lines.length).length);
  const shown = open ? lines.slice(0, pages * LIMITS.scriptPage) : lines.slice(0, EXCERPT_LINES);
  const more = open ? lines.length - shown.length : 0;
  const canOpen = lines.length > EXCERPT_LINES;
  const big = meta.bytes >= 1024;
  const openLabel = big ? `Show all ${meta.lines} lines · ${sizeLabel(meta.bytes)} of 64 KiB` : `Show all ${meta.lines} lines`;
  return (
    <div className="cm-sc">
      <div className="cm-sh">
        <span>{meta.header}</span>
        {sha ? <span className="cm-id" title={sha}>sha256 {shortHash(sha)}</span> : null}
      </div>
      <pre className={`cm-sb ${open ? "cm-scr" : lines.length > EXCERPT_LINES ? "cm-cap" : ""}`.trim()} tabIndex={open ? 0 : undefined} aria-label="Script">
        {shown.map((line, at) => (
          <span key={at} className="cm-line">
            <span className="p" aria-hidden>{num(at, width)}  </span>
            {line}
            {"\n"}
          </span>
        ))}
      </pre>
      {more > 0 ? (
        <div className="cm-sf">
          <button type="button" onClick={() => setPages((count) => count + 1)}>
            Show {Math.min(more, LIMITS.scriptPage)} more lines
          </button>
        </div>
      ) : null}
      {canOpen ? (
        <div className="cm-sf">
          <button type="button" aria-expanded={open} onClick={() => { setOpen(!open); setPages(1); }}>
            {open ? "Hide script" : openLabel}
          </button>
        </div>
      ) : null}
    </div>
  );
}
