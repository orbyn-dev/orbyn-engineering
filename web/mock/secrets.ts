/**
 * The mock's Settings › Secrets: the routes of `orbynd` (`/api/settings/secrets/...`) with the generated wire shapes.
 * It keeps the service's rules where a screen can see them (formats, limits, the plain-words errors, the broad-pattern
 * check) but not its engine: patterns run as JavaScript regular expressions. Secret-looking examples are the dotted
 * ones the screen shows, never a value.
 */
import type { PatternKind } from "../src/generated/PatternKind.ts";
import type { PatternScope } from "../src/generated/PatternScope.ts";
import type { RedactionPatternInput } from "../src/generated/RedactionPatternInput.ts";
import type { RedactionPatternTest } from "../src/generated/RedactionPatternTest.ts";
import type { RedactionPatternTestInput } from "../src/generated/RedactionPatternTestInput.ts";
import type { RedactionSettings } from "../src/generated/RedactionSettings.ts";
import type { MockResponse } from "./api.ts";

const FORMATS = [
  ["private_key", "Private key block", "-----BEGIN … PRIVATE KEY-----"],
  ["api_key", "API key", "sk-••••••••••••••••••••"],
  ["github_token", "GitHub token", "ghp_•••••••••••••"],
  ["aws_access_key", "AWS access key", "AKIA••••••••••••••••"],
  ["aws_secret_key", "AWS secret key", "40 characters after an access key"],
  ["slack_token", "Slack token", "xoxb-•••••••••"],
  ["bearer_token", "Bearer token", "Bearer ••••••••••••••••••••"],
  ["jwt", "JSON Web Token", "eyJ•••••.•••••.•••••"],
  ["named_value", "Environment values named like a secret", "KEY, TOKEN, SECRET, PASSWORD"],
] as const;

const LIMITS = { max_patterns: 50, max_name_chars: 60, max_pattern_chars: 200, max_sample_chars: 4000, max_context_lines: 200 };

/** Ordinary text, as in the service: a pattern that hides most of it hides what a person reads. */
const ORDINARY = [
  "Compiling orbyn-core v0.1.0", "Finished dev profile in 12.4s", "src/redaction/mod.rs:42: fn build() -> Result<()> {",
  "The quick brown fox jumps over the lazy dog.", "$ git status --short", " M web/src/SettingsPage.tsx", "running 14 tests",
  "test result: ok. 14 passed", "npm run check", "Listening on http://127.0.0.1:5391", "const total = items.length;", "Please review the change.",
];

interface StoredPattern { id: string; name: string; pattern: string; kind: PatternKind; applies_to: PatternScope }
type Problem = { message: string; position: number | null };

function compile(kind: PatternKind, pattern: string): RegExp | Problem {
  if (!pattern.trim()) return { message: "Enter a pattern.", position: null };
  if (pattern.length > LIMITS.max_pattern_chars) return { message: `A pattern holds at most ${LIMITS.max_pattern_chars} characters.`, position: null };
  if (kind === "text") {
    if (pattern.replaceAll("*", "").length < 3) return { message: "Spell out at least 3 characters besides *, so the pattern names something.", position: null };
    return new RegExp(pattern.split("*").map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("\\S*"), "g");
  }
  const open: number[] = [];
  let escaped = false;
  let inClass = false;
  for (let at = 0; at < pattern.length; at += 1) {
    const char = pattern[at];
    if (escaped) { escaped = false; continue; }
    if (char === "\\") escaped = true;
    else if (inClass) inClass = char !== "]";
    else if (char === "[") inClass = true;
    else if (char === "(") open.push(at);
    else if (char === ")" && !open.pop()) return { message: `This isn’t a valid pattern: the closing bracket at position ${at} has no group to close.`, position: at };
  }
  const unclosed = open.pop();
  if (unclosed !== undefined) return { message: `This isn’t a valid pattern: the group opened at position ${unclosed} is not closed.`, position: unclosed };
  try { return new RegExp(pattern, "g"); } catch { return { message: "This isn’t a valid pattern.", position: null }; }
}

const isProblem = (value: RegExp | Problem): value is Problem => !(value instanceof RegExp);
const spans = (regex: RegExp, text: string) => [...text.matchAll(regex)].filter((hit) => hit[0].length > 0).map((hit) => ({ start: hit.index, end: hit.index + hit[0].length }));
const hitLines = (regex: RegExp, lines: string[]) => lines.filter((line) => spans(regex, line).length > 0).length;

export function createSecretsMock(projects: () => { id: string; name: string }[]) {
  let terminal = true;
  let agents = true;
  let formatsOff = new Set<string>();
  let patterns: StoredPattern[] = [];
  let off = new Set<string>();
  let serial = 0;

  const reset = () => { terminal = true; agents = true; formatsOff = new Set(); patterns = []; off = new Set(); serial = 0; };

  const view = (): RedactionSettings => ({
    terminal, agents, crew_locked: true,
    formats: FORMATS.map(([id, label, example]) => ({ id, label, example, enabled: !formatsOff.has(id) })),
    patterns: patterns.map((entry) => ({ ...entry })),
    projects_off: projects().filter((project) => off.has(project.id)),
    limits: LIMITS,
  });
  const refuse = (code: string, message: string, detail: unknown = {}): MockResponse => ({ status: 422, body: { code, message, detail } });
  const notFound = (what: string): MockResponse => ({ status: 404, body: { code: "not_found", message: `${what} not found` } });

  function save(input: RedactionPatternInput, editing: string | null): MockResponse {
    const regex = compile(input.kind, input.pattern);
    if (isProblem(regex)) return refuse("invalid_pattern", regex.message, { position: regex.position });
    const name = input.name.trim();
    if (!name) return refuse("invalid_pattern", "Give the pattern a name.");
    if (patterns.some((entry) => entry.id !== editing && entry.name.toLowerCase() === name.toLowerCase())) return refuse("invalid_pattern", "A pattern with this name is already in the list.");
    if (!editing && patterns.length >= LIMITS.max_patterns) return refuse("invalid_pattern", `Orbyn keeps at most ${LIMITS.max_patterns} patterns. Remove one first.`);
    const hit = hitLines(regex, ORDINARY);
    if (input.accept_broad !== true && hit * 100 >= ORDINARY.length * 90) {
      return refuse("pattern_too_broad", `This pattern is very broad. It would hide ${hit} of ${ORDINARY.length} lines of ordinary text.`, { lines_matched: hit, lines_total: ORDINARY.length });
    }
    if (editing) {
      if (!patterns.some((entry) => entry.id === editing)) return notFound("pattern");
      patterns = patterns.map((entry) => (entry.id === editing ? { ...entry, name, pattern: input.pattern, kind: input.kind, applies_to: input.applies_to } : entry));
    } else {
      serial += 1;
      patterns = [...patterns, { id: `pattern-${serial}`, name, pattern: input.pattern, kind: input.kind, applies_to: input.applies_to }];
    }
    return { status: 200, body: view() };
  }

  function test(input: RedactionPatternTestInput): RedactionPatternTest {
    const regex = compile(input.kind, input.pattern);
    if (isProblem(regex)) return { problem: regex, matches: [], masked: "", lines_matched: 0, lines_total: 0, broad: false };
    const sample = input.sample.slice(0, LIMITS.max_sample_chars);
    const found = spans(regex, sample);
    const context = (input.lines ?? []).slice(0, LIMITS.max_context_lines);
    const lines = context.length >= 10 ? context : ORDINARY;
    const hit = hitLines(regex, lines);
    const masked = [...sample].map((char, at) => (found.some((span) => at >= span.start && at < span.end) ? "•" : char)).join("");
    return { problem: null, matches: found, masked, lines_matched: hit, lines_total: lines.length, broad: hit * 100 >= lines.length * 90 };
  }

  function handle(method: string, path: string, body: unknown): MockResponse {
    const [, , , , head, id, extra] = path.split("/");
    if (!head) return method === "GET" ? { status: 200, body: view() } : { status: 405, body: { code: "invalid_input", message: "Not allowed." } };
    if (head === "surfaces" && method === "PUT") {
      const change = body as { terminal?: boolean; agents?: boolean };
      terminal = change.terminal ?? terminal;
      agents = change.agents ?? agents;
      return { status: 200, body: view() };
    }
    if (head === "formats" && method === "PUT" && id) {
      if (!FORMATS.some(([known]) => known === id)) return notFound("recognised format");
      if ((body as { enabled: boolean }).enabled) formatsOff.delete(id); else formatsOff.add(id);
      return { status: 200, body: view() };
    }
    if (head === "projects" && method === "PUT" && id) {
      if (!projects().some((project) => project.id === id)) return notFound("project");
      if ((body as { redaction: boolean }).redaction) off.delete(id); else off.add(id);
      return { status: 200, body: view() };
    }
    if (head === "patterns" && id === "test" && method === "POST") return { status: 200, body: test(body as RedactionPatternTestInput) };
    if (head === "patterns" && !id && method === "POST") return save(body as RedactionPatternInput, null);
    if (head === "patterns" && id && !extra && method === "PUT") return save(body as RedactionPatternInput, id);
    if (head === "patterns" && id && method === "DELETE") {
      if (!patterns.some((entry) => entry.id === id)) return notFound("pattern");
      patterns = patterns.filter((entry) => entry.id !== id);
      return { status: 200, body: view() };
    }
    return { status: 404, body: { code: "not_found", message: `No mock for ${method} ${path}.` } };
  }

  return { reset, handle };
}
