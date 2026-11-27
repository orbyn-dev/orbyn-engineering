/**
 * A local, deterministic read of one input line: is it a shell command or a sentence for the thread's engine?
 * Nothing leaves this computer to decide. When the line could be either it is `ambiguous`, and callers treat
 * that as the shell. A line that looks like a secret is never a candidate for the engine (see `looksSecret`).
 */

export type Verdict = "empty" | "shell" | "prompt" | "ambiguous";

export interface Classification {
  verdict: Verdict;
  /** One short phrase, shown to the person: why the line was read this way. */
  reason: string;
}

/** Programs and builtins people type. A line that starts with one is a command unless it plainly reads as a sentence. */
export const KNOWN_COMMANDS: ReadonlySet<string> = new Set(
  `ls cd cat git gh npm npx pnpm yarn node deno bun python python3 pip pip3 cargo rustc rustup go make cmake docker kubectl
  grep rg find sed awk curl wget ssh scp rm cp mv mkdir rmdir touch echo printf pwd chmod chown tar zip unzip open code vim nano
  less more head tail sort uniq wc diff man which whereis brew apt sudo kill ps top export source clear exit history env test
  true false time watch tmux xargs tee cut tr date df du ln jq sleep alias unset set read exec eval`.split(/\s+/),
);

/** Words that start a question or a request to an assistant. */
const ASK_WORDS: ReadonlySet<string> = new Set(
  `how what why where when who which can could should would is are do does did will please explain tell show help describe summarize
  summarise write fix`.split(/\s+/),
);

/** Commands that are also everyday English words, so a longer line starting with one may be a sentence. */
const ENGLISH_LIKE: ReadonlySet<string> = new Set(
  "make find open test time watch read set sort head tail less more which date history".split(" "),
);

const withoutQuotes = (text: string) => text.replace(/"(?:[^"\\]|\\.)*"|'[^']*'/g, '""');

const OPERATOR = /\||&&|;|[<>]|`|\$\(/;
const PATH_START = /^(\/|\.{1,2}\/|~|\$|!)/;
const ASSIGNMENT = /^[A-Za-z_][A-Za-z0-9_]*=/;
const FLAG = /^-{1,2}[A-Za-z0-9]/;
const PLAIN_WORD = /^[A-Za-z][A-Za-z'’-]*[.,!?]?$/;

function classifyOne(line: string): Classification {
  const text = line.trim();
  if (text === "") return { verdict: "empty", reason: "Nothing typed" };
  const bare = withoutQuotes(text);
  if (OPERATOR.test(bare)) return { verdict: "shell", reason: "Has a pipe, redirect or chained command" };
  const words = bare.split(/\s+/);
  const first = words[0] ?? "";
  if (PATH_START.test(first)) return { verdict: "shell", reason: "Starts like a path or variable" };
  if (ASSIGNMENT.test(first)) return { verdict: "shell", reason: "Starts with a variable assignment" };
  if (words.some((word, at) => at > 0 && FLAG.test(word))) return { verdict: "shell", reason: "Has a flag" };
  if (words.some((word, at) => at > 0 && (word.includes("/") || word === '""'))) {
    return { verdict: "shell", reason: "Has a path or quoted argument" };
  }
  const known = KNOWN_COMMANDS.has(first);
  if (known && (words.length <= 2 || !ENGLISH_LIKE.has(first))) return { verdict: "shell", reason: `Starts with the command ${first}` };
  if (known) return { verdict: "ambiguous", reason: `Starts with ${first} but reads like words` };
  const plain = words.every((word) => PLAIN_WORD.test(word));
  if (words.length >= 3 && plain && text.endsWith("?")) return { verdict: "prompt", reason: "Reads like a question" };
  if (words.length >= 3 && plain && ASK_WORDS.has(first.toLowerCase())) {
    return { verdict: "prompt", reason: "Reads like a question or request" };
  }
  return { verdict: "ambiguous", reason: words.length === 1 ? "A single word" : "Could be a command or a sentence" };
}

export function classifyLine(line: string): Classification {
  const lines = line.split("\n").filter((part) => part.trim() !== "");
  if (lines.length <= 1) return classifyOne(line);
  const each = lines.map(classifyOne);
  const shell = each.find((one) => one.verdict === "shell");
  if (shell) return { verdict: "shell", reason: `${shell.reason} (several lines)` };
  return classifyOne(lines.join(" "));
}

const SECRET_PATTERNS: readonly RegExp[] = [
  /\bsk-[A-Za-z0-9_-]{16,}/,
  /\bgh[pousr]_[A-Za-z0-9]{20,}/,
  /\bAKIA[0-9A-Z]{16}\b/,
  /\bxox[bpas]-[A-Za-z0-9-]{10,}/,
  /\bBearer\s+[A-Za-z0-9._~+/=-]{16,}/i,
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/,
  /\b[0-9a-f]{32,}\b/i,
  // An assignment to a name that says it holds a secret, unless it only copies another variable. `PASS` names a secret only
  // as a whole word, so `DB_PASS` counts and `BYPASS` does not, and `PASSPHRASE` is a name of its own. A quoted value is read
  // inside its quotes: `"$VAR"` and `"$(command)"` copy or run and an empty or blank `""` holds nothing, while `"hunter2"` and
  // `" hunter2"` are typed literals. An assignment whose value falls back to a default is read by `typedAssignment` below.
  /\b[A-Z0-9_]*(?:PASSPHRASE|KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=(?:(?!["'$])(?!\s|$)\S+|"[ \t]*(?![$`"\s])\S|'[ \t]*(?!['\s])\S)/i,
  // A secret-named assignment with an ANSI-C or locale literal value (`API_TOKEN=$'hunter2'`, `API_TOKEN=$"hunter2"`): the literal is typed.
  /\b[A-Z0-9_]*(?:PASSPHRASE|KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=["']?\$['"](?![$"'])/i,
  // A flag that names a secret and carries its value (`--password hunter2`, `--passphrase hunter2`, `--token=abc`); `--password-file x` names a file, not a value; a bare `$VAR` or `"${VAR}"` copies another variable, but `$'text'` and `${VAR:-text}` carry a literal.
  /(?<![\w-])--?(?:password|passphrase|passwd|pass|token|secret|api-?key)(?:=|\s+)(?!"?\$(?:[A-Za-z_]\w*|\{[A-Za-z_]\w*\})"?(?:\s|$)|-)\S+/i,
  // The database clients take the password glued to `-p` (`mysql -u root -pSecret`); `-p` alone or on other tools is a port or a path flag.
  /\b(?:mysql|mysqldump|mysqladmin|mariadb)\b[^|;&\n]*\s-p\S+/,
  // A password in a URL's user information (`https://user:pass@host`); the userinfo ends at `/`, `?` or `#`.
  /:\/\/[^/?#\s:@]+:[^/?#\s@]+@/,
];

/**
 * The index just past the quoted span that starts at `at`, or `null` when it does not close. `'...'` holds its text as
 * written, so a backslash there escapes nothing; `"..."`, `$'...'` and `$"..."` take a backslash escape. A `"..."` and a
 * `$"..."` also nest a `${...}` expansion, so the quote inside `"${TOKEN:-"hunter2"}"` does not end the span.
 */
function quotedEnd(text: string, at: number): number | null {
  const ansi = text[at] === "$";
  const quote = ansi ? text[at + 1] : text[at];
  if (quote !== '"' && quote !== "'") return null;
  const escapes = ansi || quote === '"';
  for (let cursor = at + (ansi ? 2 : 1); cursor < text.length; ) {
    const char = text[cursor];
    if (escapes && char === "\\") {
      cursor += 2;
      continue;
    }
    if (char === quote) return cursor + 1;
    if (quote === '"' && char === "$" && text[cursor + 1] === "{") {
      const braced = bracesIn(text, cursor + 1);
      if (braced === null) return null;
      cursor = braced.end;
      continue;
    }
    cursor += 1;
  }
  return null;
}

/**
 * The inside of the `${...}` whose `{` is at `open`, and the index just past its `}`. The braces nest, since a default can
 * hold another reference (`API_TOKEN=${TOKEN:-${OTHER:-hunter2}}`), but one inside a quoted span or behind a backslash is
 * text, so `API_TOKEN=${TOKEN:-$'hunter{2'}` ends at the first `}` and keeps its `{`; nothing is returned when the
 * expansion does not close.
 */
function bracesIn(text: string, open: number): { body: string; end: number } | null {
  let depth = 0;
  for (let at = open; at < text.length; at += 1) {
    const char = text[at];
    if (char === "\\") {
      at += 1;
      continue;
    }
    if (char === "'" || char === '"' || (char === "$" && (text[at + 1] === "'" || text[at + 1] === '"'))) {
      const close = quotedEnd(text, at);
      if (close === null) return null;
      at = close - 1;
      continue;
    }
    if (char === "{") depth += 1;
    else if (char === "}") {
      depth -= 1;
      if (depth === 0) return { body: text.slice(open + 1, at), end: at + 1 };
    }
  }
  return null;
}

/**
 * The value written after an assignment's `=`, as one shell word: a whole quoted string or `${...}`, otherwise up to the
 * next space. A `}` that closes nothing and a line break end it too, so the next line is never read as this value.
 */
function valueAt(line: string, from: number): string {
  const rest = line.slice(from);
  const quoted = quotedEnd(rest, rest[0] === "$" && (rest[1] === "'" || rest[1] === '"') ? 1 : 0);
  if (quoted !== null) return rest.slice(0, quoted);
  if (rest.startsWith("${")) {
    const braced = bracesIn(rest, 1);
    if (braced) return rest.slice(0, braced.end);
  }
  return /^[^\s}]*/.exec(rest)?.[0] ?? "";
}

/**
 * Whether a value as written is text the person typed, rather than a copy or a command. Read the way the shell would: a
 * quote wraps what it holds (`" hunter2"` is text, `"$VAR"` and `"$(command)"` are not), `$(...)` and a backtick run a
 * command, `$'...'` and `$"..."` are ANSI-C and locale literals, `$NAME` and `${NAME}` copy a variable, and the default in
 * `${NAME:-default}` or `${NAME:=default}` is what the shell substitutes when that variable is unset, so it is read the same
 * way again. A single-quoted or ANSI-C literal is taken exactly as written, so a `$` inside it (`$'$ecret42'`) is text and
 * not a reference, while `"..."` and `$"..."` expand their contents and are read again. Anything left is text someone
 * typed, which is the secret.
 */
function typedValue(raw: string): boolean {
  const text = raw.replace(/^[ \t]+/, "");
  if (text === "") return false;
  if (text[0] === "'" || text.startsWith("$'")) {
    const literal = text.slice(text[0] === "'" ? 1 : 2);
    return (literal.split("'")[0] ?? "").trim() !== "";
  }
  if (text[0] === '"' || text.startsWith('$"')) return typedValue(text.slice(text[0] === "$" ? 2 : 1));
  if (text[0] === "`" || text.startsWith("$(")) return false;
  if (text.startsWith("${")) {
    const braced = bracesIn(text, 1);
    if (braced === null) return false;
    const mark = braced.body.search(/:-|:=/);
    if (mark >= 0 && typedValue(braced.body.slice(mark + 2))) return true;
    // More of the value can follow the reference (`${PREFIX}hunter2`), and that part is text someone typed as well.
    return typedValue(text.slice(braced.end));
  }
  // Any other `$` starts `$NAME`, `$1` or `$?`: a copy or a special parameter, never text of its own.
  return text[0] !== "$";
}

/** A secret-named assignment on a line, with its `=`, so the value that follows can be read. */
const SECRET_ASSIGNMENT = /\b[A-Z0-9_]*(?:PASSPHRASE|KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=/gi;

/**
 * Whether a secret-named assignment's value falls back to text someone typed: `API_TOKEN=${TOKEN:-hunter2}`, a fallback
 * inside a fallback (`API_TOKEN=${TOKEN:-${OTHER:-hunter2}}`), and an ANSI-C or locale literal as the fallback
 * (`API_TOKEN=${TOKEN:-$'hunter2'}`, `API_TOKEN=${TOKEN:-$"hunter2"}`). A default that only copies a variable, runs a
 * command or is blank (`${TOKEN:-${OTHER}}`, `${TOKEN:-$OTHER}`, `${TOKEN:-$(gh auth token)}`, `${TOKEN:- }`) is not.
 */
function typedAssignment(line: string): boolean {
  for (const found of line.matchAll(SECRET_ASSIGNMENT)) {
    if (typedValue(valueAt(line, (found.index ?? 0) + found[0].length))) return true;
  }
  return false;
}

/**
 * Whether the text looks like a credential. This is a local heuristic, not the Settings › Secrets rules (those live in
 * the service); it is only used to keep such a line in the shell and out of any prompt.
 */
export function looksSecret(text: string): boolean {
  if (SECRET_PATTERNS.some((pattern) => pattern.test(text))) return true;
  if (typedAssignment(text)) return true;
  return text
    .split(/\s+/)
    .some((token) => token.length >= 40 && /^[A-Za-z0-9+_=-]+$/.test(token) && /\d/.test(token) && /[A-Za-z]/.test(token));
}
