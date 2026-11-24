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
  // as a whole word, so `DB_PASS` counts and `BYPASS` does not. A double-quoted value is read inside its quotes:
  // `"$VAR"` and `"$(command)"` copy or run, while `"hunter2"` is the literal the person typed.
  /\b[A-Z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=(?:(?!["'$])(?!\s|$)\S+|"(?![$`])\S+|'\S+)/i,
  // A secret-named assignment with an ANSI-C or locale literal value (`API_TOKEN=$'hunter2'`, `API_TOKEN=$"hunter2"`): the literal is typed.
  /\b[A-Z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=["']?\$['"](?![$"'])/i,
  // An assignment that copies a variable but falls back to a literal after `:-` or `:=` (`API_TOKEN=${TOKEN:-hunter2}`): the
  // literal is what the person typed, so it is the secret. A bare copy (`TOKEN=${VAR}`), command substitution (backticked or
  // `$(...)`, as in `TOKEN=$(gh auth token)`) and a fallback that is itself only a variable (`TOKEN=${VAR:-$OTHER}`) carry
  // no typed secret. Nested fallbacks (`${TOKEN:-${OTHER:-hunter2}}`) are the open follow-up.
  /\b[A-Z0-9_]*(?:KEY|TOKEN|SECRET|PASSWORD|PASSWD|(?<![A-Z0-9])PASS(?![A-Z0-9]))[A-Z0-9_]*=["']?\$\{[A-Za-z_0-9][A-Za-z0-9_]*(?::-|:=)["']?(?![$}`\s"'])\S/i,
  // A flag that names a secret and carries its value (`--password hunter2`, `--token=abc`); `--password-file x` names a file, not a value; a bare `$VAR` or `"${VAR}"` copies another variable, but `$'text'` and `${VAR:-text}` carry a literal.
  /(?<![\w-])--?(?:password|passwd|pass|token|secret|api-?key)(?:=|\s+)(?!"?\$(?:[A-Za-z_]\w*|\{[A-Za-z_]\w*\})"?(?:\s|$)|-)\S+/i,
  // The database clients take the password glued to `-p` (`mysql -u root -pSecret`); `-p` alone or on other tools is a port or a path flag.
  /\b(?:mysql|mysqldump|mysqladmin|mariadb)\b[^|;&\n]*\s-p\S+/,
  // A password in a URL's user information (`https://user:pass@host`); the userinfo ends at `/`, `?` or `#`.
  /:\/\/[^/?#\s:@]+:[^/?#\s@]+@/,
];

/**
 * Whether the text looks like a credential. This is a local heuristic, not the Settings › Secrets rules (those live in
 * the service); it is only used to keep such a line in the shell and out of any prompt.
 */
export function looksSecret(text: string): boolean {
  if (SECRET_PATTERNS.some((pattern) => pattern.test(text))) return true;
  return text
    .split(/\s+/)
    .some((token) => token.length >= 40 && /^[A-Za-z0-9+_=-]+$/.test(token) && /\d/.test(token) && /[A-Za-z]/.test(token));
}
