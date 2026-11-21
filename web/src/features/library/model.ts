import { LIMITS, type EnvSet, type Param, type ParamKind } from "./contract";

/** Text the service or a file chose: cut so no line can grow a row. React escapes it; it is never markup. */
export const plain = (text: unknown, max = 80): string => {
  const whole = typeof text === "string" ? text : "";
  return whole.length > max ? `${whole.slice(0, max)}…` : whole;
};

/** A command or a prompt body for display: cut at the contract's own cap plus a little, so a hostile answer cannot grow the page. */
export const capped = (text: unknown, max: number): { text: string; cut: boolean } => {
  const whole = typeof text === "string" ? text : "";
  return whole.length > max ? { text: whole.slice(0, max), cut: true } : { text: whole, cut: false };
};

const KINDS = new Map<string, string>([["text", "Text"], ["choice", "Choice"], ["path", "Path"]]);
export const kindLabel = (kind: ParamKind | string): string => KINDS.get(kind) ?? "Unknown";

const NAME = /^[a-z][a-z0-9_]{0,31}$/;
const PLACEHOLDER = /\{\{\s*([^{}\s]*)\s*\}\}/g;
const KEY = /^[A-Z_][A-Z0-9_]{0,127}$/;

/** The names written as `{{name}}` in a body, in order, once each. */
export function placeholdersOf(body: string): string[] {
  const names: string[] = [];
  for (const match of body.matchAll(PLACEHOLDER)) {
    const name = match[1] ?? "";
    if (!names.includes(name)) names.push(name);
  }
  return names;
}

const hasControl = (text: string, allowBreaks: boolean): boolean => Array.from(text).some((char) => {
  const code = char.charCodeAt(0);
  if (allowBreaks && (code === 10 || code === 13 || code === 9)) return false;
  return code < 32 || (code >= 127 && code <= 159);
});

export interface ItemProblems { name?: string; description?: string; body?: string; params?: string; envSets?: string }

/** The doc's field rules, checked before a request so the person's text is kept and Save stays off. The service checks again. */
export function checkItem(item: { name: string; description: string; text: string; params: Param[] }, others: readonly { id: string; name: string }[], selfId: string | null, textMax: number): ItemProblems {
  const problems: ItemProblems = {};
  const name = item.name.trim();
  if (!name) problems.name = "Give it a name.";
  else if (name.length > LIMITS.name) problems.name = `Use at most ${LIMITS.name} characters.`;
  else if (hasControl(name, false)) problems.name = "Use letters, numbers and punctuation only.";
  else if (others.some((one) => one.id !== selfId && one.name.trim().toLowerCase() === name.toLowerCase())) problems.name = `“${plain(name, 40)}” is already used in Personal. Choose another name.`;
  if (item.description.length > LIMITS.description) problems.description = `Use at most ${LIMITS.description} characters.`;
  if (!item.text.trim()) problems.body = "Write the text first.";
  else if (new TextEncoder().encode(item.text).length > textMax) problems.body = `Use at most ${textMax.toLocaleString("en-US")} bytes.`;
  else {
    const declared = new Set(item.params.map((param) => param.name));
    const missing = placeholdersOf(item.text).find((used) => !declared.has(used));
    if (missing !== undefined) problems.body = `The text uses {{${plain(missing, 32)}}}, which is not a parameter. Add it or remove it.`;
  }
  const seen = new Set<string>();
  for (const param of item.params) {
    if (!NAME.test(param.name)) { problems.params = "A parameter name starts with a lowercase letter and uses lowercase letters, digits and underscores (32 at most)."; break; }
    if (seen.has(param.name)) { problems.params = `The parameter ${param.name} is listed twice.`; break; }
    seen.add(param.name);
    if (param.kind === "choice") {
      const choices = param.choices ?? [];
      if (choices.length < LIMITS.choiceMin || choices.length > LIMITS.choiceMax || choices.some((choice) => choice.length === 0 || choice.length > LIMITS.choiceText)) { problems.params = `A choice needs ${LIMITS.choiceMin} to ${LIMITS.choiceMax} options of at most ${LIMITS.choiceText} characters.`; break; }
    }
    if ((param.default ?? "").length > LIMITS.defaultValue) { problems.params = `A default is at most ${LIMITS.defaultValue} characters.`; break; }
    if (param.kind === "path" && param.default && (param.default.startsWith("/") || param.default.split(/[\\/]/).includes(".."))) { problems.params = "A path is relative to the project: no leading slash and no “..”."; break; }
  }
  if (!problems.params && item.params.length > LIMITS.params) problems.params = `Use at most ${LIMITS.params} parameters.`;
  return problems;
}

/** The values a form starts with: each parameter's default, or its first choice for a required choice. Keys are the item's own names. */
export function startValues(params: readonly Param[]): Record<string, string> {
  const values: Record<string, string> = Object.create(null) as Record<string, string>;
  for (const param of params) values[param.name] = param.default ?? (param.kind === "choice" && param.required ? (param.choices?.[0] ?? "") : "");
  return values;
}

/** Which required parameters have no value, and which choice values are outside their list: the form keeps Run off until none are. */
export function valueProblems(params: readonly Param[], values: Readonly<Record<string, string>>): Map<string, string> {
  const problems = new Map<string, string>();
  for (const param of params) {
    const value = Object.hasOwn(values, param.name) ? (values[param.name] ?? "") : "";
    if (param.required && value.trim() === "") problems.set(param.name, param.kind === "choice" ? `Choose ${plain(param.label || param.name, 40)}.` : `Enter ${plain(param.label || param.name, 40)}.`);
    else if (param.kind === "choice" && value !== "" && !(param.choices ?? []).includes(value)) problems.set(param.name, "Choose one of the listed options.");
    else if (param.kind === "path" && value !== "" && (value.startsWith("/") || value.split(/[\\/]/).includes(".."))) problems.set(param.name, "Use a path inside the project: no leading slash and no “..”.");
    else if (value.length > LIMITS.defaultValue * 8) problems.set(param.name, "That value is too long.");
  }
  return problems;
}

/** The body sent to a render or run: only the item's own parameter names, each as a string. */
export function valuesBody(params: readonly Param[], values: Readonly<Record<string, string>>): Record<string, string> {
  const body: Record<string, string> = {};
  for (const param of params) {
    const value = Object.hasOwn(values, param.name) ? values[param.name] : undefined;
    if (typeof value === "string" && value !== "") body[param.name] = value;
  }
  return body;
}

/**
 * Whether a command can go into a terminal line without running: one line, no escape or other control character. A newline in a
 * pty is Enter, so a multi-line command would run as it is typed. This is the only gate; the person's own click presses Enter.
 */
export function isOneLine(command: string): boolean {
  return command.length > 0 && !hasControl(command, false);
}

/** Key names the doc refuses, and the shape it allows. Checked before a request; the service checks again. */
export function checkKeyName(name: string, existing: readonly string[]): string | null {
  if (!KEY.test(name)) return "Use capital letters, digits and underscores, starting with a letter or underscore.";
  if (/^(PATH|HOME|SHELL|IFS|NODE_OPTIONS|PYTHONPATH|BASH_ENV|ENV|CURSOR_API_KEY)$/.test(name) || /^(LD_|DYLD_|ORBYN_|CLAUDE|CODEX)/.test(name)) return "Orbyn keeps this name for itself. Choose another.";
  if (existing.includes(name)) return "This set already has that name.";
  if (existing.length >= LIMITS.keysPerSet) return `A set holds at most ${LIMITS.keysPerSet} variables.`;
  return null;
}

/** The keychain state in words, for the note on a set. `null` when nothing needs saying. */
export function keychainNote(set: Pick<EnvSet, "keychain" | "status">): string | null {
  if (set.status === "planned") return "Values are not stored on this computer yet. The names are kept, and a workflow that lists this set runs without them.";
  if (set.keychain === "blocked") return "The keychain is locked or the prompt was cancelled. Unlock it, then try again.";
  if (set.keychain === "unavailable") return "The keychain is not available. Values can’t be saved.";
  return null;
}

const toBase64 = (text: string): string => {
  try {
    const bytes = new TextEncoder().encode(text);
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary);
  } catch { return ""; }
};

/**
 * Takes a secret value out of text that is about to be shown, in the forms an error is likely to carry it: as typed, URL-encoded,
 * JSON-escaped, base64 (with and without padding) and line by line. A value shorter than 4 characters is matched whole only, so a
 * one-letter value does not blank a sentence. The text is shown only after this; it never goes to a log.
 */
export function scrubValue(text: string, value: string): string {
  if (value === "") return text;
  const forms = new Set<string>([value, encodeURIComponent(value), JSON.stringify(value).slice(1, -1), toBase64(value), toBase64(value).replace(/=+$/, "")]);
  if (value.length >= 4) for (const line of value.split(/\r?\n/)) if (line.length >= 4) forms.add(line);
  let clean = text;
  for (const form of [...forms].filter((one) => one !== "").sort((a, b) => b.length - a.length)) clean = clean.split(form).join("[hidden]");
  return clean;
}
