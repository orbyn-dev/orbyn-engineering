import type { ReactNode } from "react";
import "./codemode.css";
import { callDuration } from "../../lib/format";
import { parsed } from "../../product/timeline/toolTrace";
import type { ToolItem } from "../../product/timeline/types";

/**
 * The blocks for `orbyn_code_search` and `orbyn_code_describe`. Their replies are not in the contract as exact shapes yet
 * (docs/code-mode.md gives "ranked tool paths" and "TypeScript declarations ... unknown listed under `unknown`"), so both are
 * read leniently here and fall back to the raw text. Everything is rendered as text.
 */

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const asText = (value: unknown): string | null => (typeof value === "string" && value !== "" ? value : null);

function json(text: string | null): unknown {
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

interface Hit {
  path: string;
  line: string | null;
}

/** Ranked hits from a reply: an array, or an object holding one under `results`, `hits`, `tools` or `paths`. */
export function hitsOf(text: string | null): Hit[] | null {
  const value = json(text);
  const list = Array.isArray(value) ? value : isRecord(value) ? [value.results, value.hits, value.tools, value.paths].find(Array.isArray) : null;
  if (!Array.isArray(list)) return null;
  return list.flatMap((entry): Hit[] => {
    if (typeof entry === "string") return [{ path: entry, line: null }];
    if (!isRecord(entry)) return [];
    const path = asText(entry.path) ?? asText(entry.name);
    return path ? [{ path, line: asText(entry.signature) ?? asText(entry.summary) ?? asText(entry.description) }] : [];
  }).slice(0, 50);
}

/** Declarations and the paths that were not available, from a describe reply. */
export function declarationsOf(text: string | null): { code: string; unknown: string[] } | null {
  const value = json(text);
  if (!isRecord(value)) return null;
  const raw = value.declarations ?? value.declaration;
  const code = Array.isArray(raw) ? raw.filter((part): part is string => typeof part === "string").join("\n\n") : asText(raw);
  const unknown = Array.isArray(value.unknown) ? value.unknown.filter((path): path is string => typeof path === "string") : [];
  return code !== null || unknown.length > 0 ? { code: (code ?? "").slice(0, 64 * 1024), unknown } : null;
}

const argRows = (item: ToolItem): [string, string][] =>
  Object.entries(parsed(item.args)).flatMap(([key, value]) => {
    const shown = typeof value === "string" || typeof value === "number" ? String(value) : Array.isArray(value) ? value.filter((part) => typeof part === "string").join(", ") : null;
    return shown === null ? [] : [[key, shown]];
  });

function Frame({ item, ms, inspect, children }: { item: ToolItem; ms: number | null; inspect: ReactNode; children: ReactNode }) {
  const rows = argRows(item);
  return (
    <div className="cb">
      {rows.length ? (
        <div className="kvs">
          {rows.flatMap(([key, value]) => [
            <b key={`k-${key}`} data-find-skip>{key}</b>,
            <span key={`v-${key}`}>{value}</span>,
          ])}
        </div>
      ) : null}
      {children}
      <div className="cf">
        <span className="tfm" data-find-skip>Read-only tool{ms === null ? "" : ` · ${callDuration(ms)}`}</span>
        {inspect}
      </div>
    </div>
  );
}

export function CodeSearchBlock({ item, ms, inspect }: { item: ToolItem; ms: number | null; inspect: ReactNode }) {
  const hits = hitsOf(item.result);
  return (
    <Frame item={item} ms={ms} inspect={inspect}>
      {hits === null ? (
        item.result ? <pre className="co" tabIndex={0}>{item.result.slice(0, 4_000)}</pre> : null
      ) : (
        <div className="cm-hits">
          <div className="cm-h">{hits.length} {hits.length === 1 ? "tool" : "tools"}</div>
          {hits.length === 0 ? <div className="co mute">No tool matches that.</div> : null}
          {hits.map((hit) => (
            <div key={hit.path} className="cm-hit">
              <b>{hit.path}</b>
              {hit.line ? <span>{hit.line}</span> : null}
            </div>
          ))}
        </div>
      )}
    </Frame>
  );
}

export function CodeDescribeBlock({ item, ms, inspect }: { item: ToolItem; ms: number | null; inspect: ReactNode }) {
  const found = declarationsOf(item.result);
  return (
    <Frame item={item} ms={ms} inspect={inspect}>
      {found === null ? (
        item.result ? <pre className="co" tabIndex={0}>{item.result.slice(0, 4_000)}</pre> : null
      ) : (
        <>
          {found.code ? <pre className="co nowrap" tabIndex={0}>{found.code}</pre> : null}
          {found.unknown.length > 0 ? (
            <div className="cm-sec">
              <div className="cm-h">Not available · {found.unknown.length}</div>
              <div className="co">{found.unknown.map((path) => <span key={path} className="mono">{path}{"\n"}</span>)}</div>
            </div>
          ) : null}
        </>
      )}
    </Frame>
  );
}
