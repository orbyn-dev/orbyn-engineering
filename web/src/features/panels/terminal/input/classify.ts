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
  // A secret-named assignment with an ANSI-C literal value (`API_TOKEN=$'hunter2'`): the literal is typed, and what it
  // holds is text even when it is a backtick or a `$` (`API_TOKEN=$'`id`'` holds the two backticks and runs nothing).
  /\b[A-Z0-9_]*(?:PASSPHRASE|KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=["']?\$'(?!['"$])/i,
  // A secret-named assignment with a locale-string value (`API_TOKEN=$"hunter2"`). A `$"..."` still expands what it
  // holds, so one that starts with a copy or a command is only that value (`PASSPHRASE=$"`id`"`, `PASSPHRASE=$" ${VAR}"`),
  // and one that starts with a typed character is the locale literal it says it is (`PASSPHRASE=$"hunter2"`).
  /\b[A-Z0-9_]*(?:PASSPHRASE|KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=["']?\$"(?![ \t]*[$`"])/i,
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
 * `$"..."` also nest a `${...}` or a command substitution, so the quote inside `"${TOKEN:-"hunter2"}"` and inside
 * `"hunter2$(printf '"')"` does not end the span.
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
    if (quote === '"' && ((char === "$" && text[cursor + 1] === "(") || char === "`")) {
      const command = commandEnd(text, cursor);
      if (command === null) return null;
      cursor = command;
      continue;
    }
    cursor += 1;
  }
  return null;
}

/**
 * Whether a `#` begins a shell comment. The shell starts one only at the beginning of a word, so `a#b` is text while
 * `printf X # it's fine` comments out the apostrophe; `from` is where the command that holds the `#` starts.
 */
function startsComment(text: string, at: number, from: number): boolean {
  return at === from || /[ \t\r\n;&|(]/.test(text[at - 1] ?? "");
}

/**
 * The index just past the `$(...)` or backtick span that starts at `at`, or `null` when it does not close. The shell reads
 * the span as one word, so a `)` or `}` written inside a quote or a `${...}` in it closes nothing: in
 * `API_TOKEN="${TOKEN:-$(echo })}"` the `echo` argument is the `}`, and the expansion ends at the last one. A `#` that
 * starts a word comments out the rest of its line, so the quote in `$(whoami # don't log)` opens nothing and the span
 * closes on the next line; a backtick is found before the command inside it is read, so there the comment ends at the
 * backtick or the line, whichever comes first.
 */
function commandEnd(text: string, at: number): number | null {
  const backtick = text[at] === "`";
  const start = at + (backtick ? 1 : 2);
  let depth = 1;
  for (let cursor = start; cursor < text.length; cursor += 1) {
    const char = text[cursor];
    if (char === "\\") {
      cursor += 1;
      continue;
    }
    if (char === "#" && startsComment(text, cursor, start)) {
      const line = text.indexOf("\n", cursor);
      const close = backtick ? text.indexOf("`", cursor) : -1;
      if (close >= 0 && (line < 0 || close < line)) return close + 1;
      if (line < 0) return null;
      cursor = line;
      continue;
    }
    if (char === "'" || char === '"' || (char === "$" && (text[cursor + 1] === "'" || text[cursor + 1] === '"'))) {
      const close = quotedEnd(text, cursor);
      if (close === null) return null;
      cursor = close - 1;
      continue;
    }
    if (backtick) {
      if (char === "`") return cursor + 1;
      continue;
    }
    if (char === "$" && text[cursor + 1] === "{") {
      const braced = bracesIn(text, cursor + 1);
      if (braced === null) return null;
      cursor = braced.end - 1;
      continue;
    }
    if (char === "`") {
      const close = commandEnd(text, cursor);
      if (close === null) return null;
      cursor = close - 1;
      continue;
    }
    if (char === "$" && text[cursor + 1] === "(") {
      depth += 1;
      cursor += 1;
      continue;
    }
    if (char === "(") depth += 1;
    else if (char === ")") {
      depth -= 1;
      if (depth === 0) return cursor + 1;
    }
  }
  return null;
}

/**
 * The inside of the `${...}` whose `{` is at `open`, and the index just past its `}`. Only another `${` nests, since a
 * default can hold one (`API_TOKEN=${TOKEN:-${OTHER:-hunter2}}`), while a bare `{` is text: `API_TOKEN=${TOKEN:-hunter{2}`
 * runs `$TOKEN` to `hunter{2` and ends at the one `}`. A brace inside a quoted span, a command substitution or behind a
 * backslash is text too, so `API_TOKEN=${TOKEN:-$'hunter{2'}` ends at the first `}` and keeps its `{`, and
 * `API_TOKEN=${TOKEN:-$(echo })}` ends at the last one; nothing is returned when the expansion does not close.
 */
function bracesIn(text: string, open: number): { body: string; end: number } | null {
  let depth = 1;
  for (let at = open + 1; at < text.length; at += 1) {
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
    if ((char === "$" && text[at + 1] === "(") || char === "`") {
      const close = commandEnd(text, at);
      if (close === null) return null;
      at = close - 1;
      continue;
    }
    if (char === "$" && text[at + 1] === "{") {
      depth += 1;
      at += 1;
      continue;
    }
    if (char === "}") {
      depth -= 1;
      if (depth === 0) return { body: text.slice(open + 1, at), end: at + 1 };
    }
  }
  return null;
}

/**
 * The value written after an assignment's `=`, as one shell word. The shell glues a word together from quoted strings,
 * `${...}` expansions, command substitutions and plain text until a space, a line break or an unquoted `;`, `&`, `|`, `<`
 * or `>` ends it, so `${TOKEN:-$(echo })}hunter2` is one value while `TOKEN=$OTHER;true` is only `$OTHER`; a line break
 * ends it too, so the next line is never read as this value. A `}` that closes no expansion is a literal character of the
 * word, the way the shell reads it, so `API_TOKEN=${TOKEN:-$OTHER}}hunter2` carries the `}hunter2` it substitutes.
 */
function valueAt(line: string, from: number): string {
  let end = from;
  while (end < line.length) {
    const rest = line.slice(end);
    if (/^[ \t\r\n;&|<>]/.test(rest)) break;
    if (rest[0] === "\\") {
      end += 2;
      continue;
    }
    if (rest[0] === "'" || rest[0] === '"' || rest.startsWith("$'") || rest.startsWith('$"')) {
      const close = quotedEnd(rest, 0);
      if (close === null) break;
      end += close;
      continue;
    }
    if (rest.startsWith("${")) {
      const braced = bracesIn(rest, 1);
      if (braced === null) break;
      end += braced.end;
      continue;
    }
    if (rest.startsWith("$(") || rest[0] === "`") {
      const close = commandEnd(rest, 0);
      if (close === null) break;
      end += close;
      continue;
    }
    end += 1;
  }
  return line.slice(from, Math.min(end, line.length));
}

/**
 * The index just past the unbraced reference that starts at `at`: `$NAME`, a positional `$1`, or a special parameter such
 * as `$?`. The name runs to the first character that cannot be part of one, so `$PREFIX-hunter2` refers to `PREFIX`.
 */
function referenceEnd(text: string, at: number): number {
  const name = /^[A-Za-z_][A-Za-z0-9_]*|^[0-9]/.exec(text.slice(at + 1));
  return at + 1 + (name?.[0].length ?? 1);
}

/**
 * The word a `${...}` substitutes instead of the parameter's own value: the default or assignment in `${NAME:-word}` and
 * `${NAME:=word}`, and the unset-only `${NAME-word}` and `${NAME=word}`. `null` when the expansion only reads its
 * parameter. The unset-only operator sits right after the name, which is why `${MY-VAR}` is the parameter `MY` with the
 * default `VAR` and not a subtraction, while `${NAME%%word}`, `${NAME:1}` and `${NAME/word/x}` substitute no word.
 */
function defaultWord(body: string): string | null {
  const colon = body.search(/:-|:=/);
  if (colon >= 0) return body.slice(colon + 2);
  const unsetOnly = /^(?:#|!)?(?:[A-Za-z_][A-Za-z0-9_]*|[0-9]+|[-@*#?$!])[-=]/.exec(body);
  return unsetOnly === null ? null : body.slice(unsetOnly[0].length);
}

/**
 * Whether a value as written is text the person typed, rather than a copy or a command. Read the way the shell would: a
 * quote wraps what it holds (`" hunter2"` is text, `"$VAR"` and `"$(command)"` are not), `$(...)` and a backtick run a
 * command, `$'...'` and `$"..."` are ANSI-C and locale literals, `$NAME` and `${NAME}` copy a variable, and the word in
 * `${NAME:-word}` or `${NAME:=word}` — or in the unset-only `${NAME-word}` and `${NAME=word}` — is what the shell
 * substitutes when that variable is unset, so it is read the same way again. A single-quoted or ANSI-C literal is taken
 * exactly as written, so a `$` inside it (`$'$ecret42'`) is text and not a reference, while `"..."` and `$"..."` expand
 * their contents and are read again. More of the value can follow a span and is read too, since the shell keeps it:
 * `$PREFIX-hunter2` copies `PREFIX` and glues typed `-hunter2` to it, and `''hunter2` adds an empty literal to typed
 * `hunter2`. Anything left is text someone typed, which is the secret.
 */
function typedValue(raw: string): boolean {
  const text = raw.replace(/^[ \t]+/, "");
  if (text === "") return false;
  // A `}` that closes no expansion is a literal character of the word, so on its own it is not text anyone typed
  // (`API_TOKEN=${TOKEN:-$OTHER}}` prints `}`); whatever follows it can still be typed.
  if (text[0] === "}") return typedValue(text.slice(1));
  if (text[0] === "'" || text.startsWith("$'")) {
    const literal = text.slice(text[0] === "'" ? 1 : 2);
    const close = literal.indexOf("'");
    if (close < 0) return literal.trim() !== "";
    if (literal.slice(0, close).trim() !== "") return true;
    // An empty literal holds nothing of its own, and the rest of the word can still be typed.
    return typedValue(literal.slice(close + 1));
  }
  if (text[0] === '"' || text.startsWith('$"')) return typedValue(text.slice(text[0] === "$" ? 2 : 1));
  if (text[0] === "`" || text.startsWith("$(")) {
    const end = commandEnd(text, 0);
    return end !== null && typedValue(text.slice(end));
  }
  if (text.startsWith("${")) {
    const braced = bracesIn(text, 1);
    if (braced === null) return false;
    const word = defaultWord(braced.body);
    if (word !== null && typedValue(word)) return true;
    // More of the value can follow the reference (`${PREFIX}hunter2`), and that part is text someone typed as well.
    return typedValue(text.slice(braced.end));
  }
  // Any other `$` starts `$NAME`, `$1` or `$?`: a copy or a special parameter, never text of its own. What follows it is
  // still part of the value (`$PREFIX-hunter2`), so read that part; a reference that ends the value copies only.
  if (text[0] === "$") return typedValue(text.slice(referenceEnd(text, 0)));
  return true;
}

/** A secret-named assignment on a line, with its `=`, so the value that follows can be read. */
const SECRET_ASSIGNMENT = /\b[A-Z0-9_]*(?:PASSPHRASE|KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=/gi;

/**
 * Whether a secret-named assignment's value falls back to text someone typed: `API_TOKEN=${TOKEN:-hunter2}`, a fallback
 * inside a fallback (`API_TOKEN=${TOKEN:-${OTHER:-hunter2}}` and `API_TOKEN=${TOKEN:-${OTHER-hunter2}}`), and an ANSI-C or
 * locale literal as the fallback (`API_TOKEN=${TOKEN:-$'hunter2'}`, `API_TOKEN=${TOKEN:-$"hunter2"}`). A default that only
 * copies a variable, runs a command or is blank (`${TOKEN:-${OTHER}}`, `${TOKEN:-$OTHER}`, `${TOKEN:-$(gh auth token)}`,
 * `${TOKEN:- }`) is not.
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
