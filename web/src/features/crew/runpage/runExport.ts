// The Run page of a Crew task (W11): a static HTML page and a short Markdown comment, both built here, on this device, from
// the Flow task read the app already has. Nothing is uploaded and nothing is written to a route. Every piece of text is
// untrusted (a task name, a brief, a finding's claim): it is cleaned (control characters, key-shaped words and machine paths
// hidden, length capped) and then escaped for the format it goes into.
import type { FlowTaskDetail } from "../../../generated/FlowTaskDetail";
import { cellOf, decidedBy, plain } from "../flow/live/model";
import { checkWord, findingStatusWord } from "../map/model";

/** The most characters of one cleaned text, by where it is shown. */
const CAP = { title: 120, line: 300, short: 60 } as const;
/** The most findings, gates and files the page lists. */
const LIST_CAP = 50;
/** The longest PR comment. The forge's own limit is far above it. */
export const RUN_COMMENT_CAP = 4000;

/** Control, zero-width, line-separator and bidi-override characters: each becomes a space. */
const isInvisible = (code: number): boolean =>
  code <= 0x1f || (code >= 0x7f && code <= 0x9f) || (code >= 0x200b && code <= 0x200f) || code === 0x2028 || code === 0x2029
  || (code >= 0x202a && code <= 0x202e) || (code >= 0x2066 && code <= 0x2069) || code === 0xfeff;
const plainChars = (text: string): string => Array.from(text, (char) => (isInvisible(char.codePointAt(0)!) ? " " : char)).join("");
const KNOWN_KEY = /(?:\b(?:sk|pk|rk)-[A-Za-z0-9_-]{16,}|\bgh[pousr]_[A-Za-z0-9]{20,}|\bgithub_pat_\w{20,}|\bxox[abprs]-[\w-]{10,}|\bAKIA[0-9A-Z]{16}\b|\bAIza[\w-]{30,}|\beyJ[\w-]{10,}\.[\w-]{10,}\.[\w-]{5,})/;
const KEY_PAIR = /^([\w.-]*(?:key|token|secret|passw(?:or)?d|pwd|credential|authorization)[\w.-]*)[=:](.+)$/i;
const MACHINE_PATH = /^(?:~|file:|[A-Za-z]:[\\/]|\\\\)|\/(?:Users|home|private|var|tmp|root|opt|mnt)\/|^\/[^/\s]+\/\S*/;

const HIDDEN = "[hidden]";
const PATH_HIDDEN = "[path hidden]";

function hideWord(word: string): string {
  const pair = KEY_PAIR.exec(word);
  if (pair) return `${pair[1]}=${HIDDEN}`;
  if (KNOWN_KEY.test(word)) return HIDDEN;
  if (MACHINE_PATH.test(word)) return PATH_HIDDEN;
  if (word.length >= 32 && /^[A-Za-z0-9+/_=-]+$/.test(word) && /\d/.test(word) && /[A-Za-z]/.test(word) && !/^[0-9a-f]+$/i.test(word) && !word.includes("/")) return HIDDEN;
  return word;
}

/** Text as the page may show it: one line, no control characters, no key-shaped word, no path on this machine, at most `cap` long. */
export function cleanText(text: string, cap: number): string {
  const one = plainChars(text).replace(/\bBearer\s+\S+/gi, `Bearer ${HIDDEN}`).replace(/\s+/g, " ").trim();
  const out = one.split(" ").map(hideWord).join(" ");
  return out.length > cap ? `${out.slice(0, Math.max(0, cap - 1)).trimEnd()}…` : out;
}

const escapeHtml = (text: string): string =>
  text.replace(/[&<>"']/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char]!);

/** Markdown text that renders as itself: no link, mention, issue reference, HTML, emphasis or math can start in it. */
function escapeMd(text: string): string {
  return text
    .replace(/:\/\//g, ":\u200b//")
    .replace(/www\./gi, "www\u200b.")
    .replace(/#(?=\d)/g, "#\u200b")
    .replace(/@/g, "@\u200b")
    .replace(/[\\`*_{}[\]()<>#+!|~&$"'-]/g, "\\$&");
}

/** A file name from sanitized text: lower-case words joined by "-", never a path, at most 60 characters. */
export function fileNameOf(title: string): string {
  const slug = title.normalize("NFKD").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 40).replace(/-+$/g, "");
  return `${slug === "" ? "crew" : slug}-run.html`;
}

const STATES: Record<string, { word: string; tone: "ok" | "fail" | "run" | "stop" }> = {
  passed: { word: "Passed", tone: "ok" },
  failed: { word: "Failed", tone: "fail" },
  stopped: { word: "Stopped", tone: "fail" },
  cancelled: { word: "Cancelled", tone: "stop" },
};

export interface RunPageFinding {
  where: string;
  claim: string;
  who: string;
  severity: string;
  check: string;
  status: string;
  blocks: boolean;
}

export interface RunPageModel {
  title: string;
  stateWord: string;
  tone: "ok" | "fail" | "run" | "stop";
  crew: string;
  round: number;
  madeAt: string;
  generatedAt: string;
  route: string | null;
  checks: string | null;
  checksFailed: boolean;
  purpose: string;
  files: string[];
  budget: string;
  findings: RunPageFinding[];
  findingsOmitted: number;
  confirmed: number;
  gates: string[];
  merge: string | null;
  /** The sections this page cannot fill from the Flow read, named so the page never implies them. */
  planned: string[];
}

const utc = (iso: string): string => {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? cleanText(iso, 40) : `${at.toISOString().slice(0, 16).replace("T", " ")} UTC`;
};

/** What the page and the comment say, from one Flow task read. `now` is when it is made. */
export function runPageModel(detail: FlowTaskDetail, now: string): RunPageModel {
  const { row } = detail;
  const state = STATES[row.state] ?? { word: "In progress", tone: "run" as const };
  const checks = cellOf(row, "checks");
  const confirmed = detail.findings.filter((one) => one.check === "confirmed").length;
  return {
    title: cleanText(row.name || "Untitled task", CAP.title),
    stateWord: state.word,
    tone: state.tone,
    crew: cleanText(detail.crew, CAP.short),
    round: row.round,
    madeAt: utc(row.updated_at),
    generatedAt: utc(now),
    route: detail.route ? [detail.route.engine, detail.route.model, detail.route.tier].map((part) => cleanText(part, CAP.short)).join(" · ") : null,
    checks: checks.passed != null && checks.of != null ? `${checks.passed} of ${checks.of} passed` : null,
    checksFailed: checks.state === "failed",
    purpose: cleanText(detail.brief.purpose, 600),
    files: detail.brief.files.slice(0, LIST_CAP).map((file) => cleanText(file, CAP.line)),
    budget: `${detail.brief.budget_turns} turns · ${detail.brief.budget_minutes} min`,
    findings: detail.findings.slice(0, LIST_CAP).map((one) => ({
      where: cleanText(`${one.path}${one.line > 0 ? `:${one.line}` : ""}`, CAP.line),
      claim: cleanText(one.claim, CAP.line),
      who: cleanText(`${one.lane} · round ${one.round}`, CAP.short),
      severity: cleanText(one.severity, 20),
      check: checkWord(cleanText(one.check, 30)),
      status: findingStatusWord(cleanText(one.status, 30)),
      blocks: one.blocks,
    })),
    findingsOmitted: Math.max(0, detail.findings.length - LIST_CAP),
    confirmed,
    gates: detail.gates.slice(0, LIST_CAP).map((gate) => cleanText(`${gate.stage}: ${plain(gate.decision)} · decided by ${decidedBy(gate)}`, CAP.line)),
    merge: detail.merge ? cleanText(`${plain(detail.merge.state)} · risk ${detail.merge.risk} · ${detail.merge.mode}`, CAP.line) : null,
    planned: ["Prompt and plan", "Check commands, exit codes and durations", "What was not verified", "Signed proof and verify"],
  };
}

const CSS = `:root{color-scheme:light dark;--bg:#fff;--fg:#1c1c1e;--mute:#5f6368;--line:#d8d8dc;--chip:#eef0f3;--ok:#146c2e;--bad:#b3261e}
@media (prefers-color-scheme:dark){:root{--bg:#161618;--fg:#ececee;--mute:#a0a4ab;--line:#33353a;--chip:#26282c;--ok:#5fd37f;--bad:#ff8a80}}
body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,sans-serif}
main{max-width:760px;margin:0 auto;padding:24px 16px 48px}
h1{font-size:22px;margin:0 0 4px;overflow-wrap:anywhere}h2{font-size:15px;margin:28px 0 8px;border-top:1px solid var(--line);padding-top:16px}
p,li,dd{overflow-wrap:anywhere}.mute{color:var(--mute)}ul{padding-left:20px}dl{display:grid;grid-template-columns:max-content 1fr;gap:4px 16px}dt{color:var(--mute)}dd{margin:0}
.chip{display:inline-block;padding:0 8px;border-radius:10px;background:var(--chip);font-size:12px}.ok{color:var(--ok)}.bad{color:var(--bad)}
.banner{border:1px solid var(--bad);border-radius:8px;padding:8px 12px;margin:12px 0}code{font:13px ui-monospace,monospace}`;

const li = (html: string) => `<li>${html}</li>`;

/** One self-contained page: inline style, no script, no remote resource, no link. Every value is escaped. */
export function runPageHtml(model: RunPageModel): string {
  const e = escapeHtml;
  const head = model.tone === "fail" ? `<p class="banner bad" role="alert">${e(model.stateWord)}: this run did not pass. Read the checks and the findings before relying on it.</p>` : "";
  const findings = model.findings.length === 0 ? `<p class="mute">No findings.</p>` : `<ul>${model.findings.map((one) => li(`<strong>${e(one.claim)}</strong> <code>${e(one.where)}</code><br><span class="mute">${e(one.who)} · ${e(one.severity)}</span> <span class="chip">${e(one.check)}</span> <span class="chip">${e(one.status)}</span>${one.blocks ? ` <span class="chip bad">Blocks</span>` : ""}`)).join("")}</ul>${model.findingsOmitted > 0 ? `<p class="mute">${model.findingsOmitted} more findings are not listed.</p>` : ""}`;
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'"><meta name="referrer" content="no-referrer">
<title>${e(model.title)} · run page</title><style>${CSS}</style></head><body><main>
<h1>${e(model.title)}</h1>
<p><span class="chip ${model.tone === "ok" ? "ok" : model.tone === "fail" ? "bad" : ""}">${e(model.stateWord)}</span> <span class="mute">Crew ${e(model.crew)} · round ${model.round} · ${e(model.madeAt)}</span></p>
${head}
<p class="mute">This page is a snapshot made on the device that ran the crew, on ${e(model.generatedAt)}. It does not update, and nothing was uploaded.</p>
<h2>Brief</h2><p>${e(model.purpose)}</p>
<dl><dt>Files</dt><dd>${model.files.length === 0 ? "None" : e(model.files.join(", "))}</dd><dt>Budget</dt><dd>${e(model.budget)}</dd><dt>Route</dt><dd>${model.route ? e(model.route) : "Not routed yet"}</dd></dl>
<h2>Checks</h2><p>${model.checks ? `<span class="${model.checksFailed ? "bad" : "ok"}">${e(model.checks)}</span>` : "No check result yet."} <span class="mute">Counts only; the commands are Planned.</span></p>
<h2>Findings</h2>${findings}
${model.gates.length > 0 ? `<h2>Gates</h2><ul>${model.gates.map((gate) => li(e(gate))).join("")}</ul>` : ""}
${model.merge ? `<h2>Merge</h2><p>${e(model.merge)}</p>` : ""}
<h2>Not verified</h2><p class="mute">Planned, so not on this page: ${model.planned.map(e).join("; ")}. Only confirmed findings block; model reviews are advisory.</p>
</main></body></html>
`;
}

/** The pull request comment: a short summary in Markdown, at most `RUN_COMMENT_CAP` characters. It is never posted by Orbyn. */
export function runPageMarkdown(model: RunPageModel, pull: number | null): string {
  const m = escapeMd;
  const head = [
    `### Crew run${pull != null ? ` for #${pull}` : ""}: ${m(model.title)}`,
    `Result: **${model.stateWord}** · round ${model.round} · ${m(model.madeAt)}`,
    ...(model.checks ? [`Checks: ${m(model.checks)}`] : []),
    ...(model.route ? [`Route: ${m(model.route)}`] : []),
    `Findings: ${model.findings.length + model.findingsOmitted} (${model.confirmed} confirmed)`,
    "",
  ];
  const foot = ["", `_Made on the device that ran the crew. Not on this summary yet: ${m(model.planned.join("; "))}. Nothing was uploaded._`];
  let size = [...head, ...foot].join("\n").length;
  const rows: string[] = [];
  for (const [at, one] of model.findings.entries()) {
    const line = `- ${m(one.claim)} (${m(one.where)}, ${m(one.check)}, ${m(one.status)}${one.blocks ? ", blocks" : ""})`;
    const rest = `- …and ${model.findings.length - at + model.findingsOmitted} more findings`;
    if (size + line.length + 1 + rest.length + 1 > RUN_COMMENT_CAP) {
      rows.push(rest);
      break;
    }
    rows.push(line);
    size += line.length + 1;
  }
  if (rows.length === model.findings.length && model.findingsOmitted > 0) rows.push(`- …and ${model.findingsOmitted} more findings`);
  return [...head, ...rows, ...foot].join("\n").slice(0, RUN_COMMENT_CAP);
}
