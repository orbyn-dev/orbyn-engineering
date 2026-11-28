import { describe, expect, it } from "vitest";
import { classifyLine, looksSecret } from "./classify";

const shell = ["ls", "ls -la", "git status", "git commit -m \"fix the bug\"", "npm run build", "cd ../web", "cat src/main.tsx | grep foo", "make test && make lint", "echo hi > out.txt", "./scripts/run.sh", "~/bin/tool --flag", "FOO=1 node app.js", "export PATH=$PATH:/x", "rm -rf node_modules", "docker ps -a", "$(pwd)", "!!", "/usr/bin/env python3", "cargo test -p core", "grep -rn \"how to\" src"];
const prompt = ["How do I undo my last commit?", "why is the build failing?", "what does this error mean", "Can you explain what the migration does", "please fix the failing test in the terminal", "show me which files changed since yesterday", "Is there a way to run only one test?", "explain the difference between rebase and merge"];
const ambiguous = ["make sure the tests pass", "find the file that holds the routes", "test", "hello", "build it now", "git", "what", "open the file", "a b"];

describe("classifyLine", () => {
  it.each(shell)("reads %j as a command", (line) => expect(classifyLine(line).verdict).toBe("shell"));
  it.each(prompt)("reads %j as a prompt", (line) => expect(classifyLine(line).verdict).toBe("prompt"));
  it.each(ambiguous)("leaves %j ambiguous, never a prompt", (line) => expect(classifyLine(line).verdict).not.toBe("prompt"));
  it("treats a blank line as empty", () => expect(classifyLine("   ").verdict).toBe("empty"));
  it("is deterministic and gives a reason", () => {
    expect(classifyLine("why is it slow?")).toEqual(classifyLine("why is it slow?"));
    expect(classifyLine("why is it slow?").reason).not.toBe("");
  });
  it("never reads a sentence-looking line with an operator as a prompt", () => {
    expect(classifyLine("how are you | cat").verdict).toBe("shell");
    expect(classifyLine("what is this > file").verdict).toBe("shell");
  });
  it("a multi-line line is judged as a whole", () => {
    expect(classifyLine("ls\npwd").verdict).toBe("shell");
  });
});

describe("looksSecret", () => {
  it.each(["sk-ant-api03-abcdefghijklmnopqrstuvwx", "ghp_abcdefghijklmnopqrstuvwxyz0123456789", "AKIAIOSFODNN7EXAMPLE", "export OPENAI_API_KEY=abc123", "TOKEN=abcdef curl x", "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2", "curl -H 'Authorization: Bearer abcdefghijklmnop1234'", "export DB_PASSWORD=hunter2", "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo0MTIzNDU2Nzg5", "export API_TOKEN=${TOKEN:-hunter2}", "DB_PASS=${PASS:=s3cret}", "DB_PASS=abc123", "export API_TOKEN=\"${TOKEN:-a b}\"", "API_TOKEN=$'hunter2'", "API_TOKEN=$\"hunter2\"", "API_TOKEN=\"$'hunter2'\"", "TOKEN=${1:-hunter2}", "export DB_PASSWORD=\"hunter2\"", "API_TOKEN=${TOKEN:-${PREFIX}hunter2}", "API_TOKEN=${TOKEN:-' hunter2'}", "export DB_PASSWORD=\" hunter2\"", "export DB_PASSWORD=' hunter2'", "export DB_PASSWORD=\"  hunter2  \"", "API_TOKEN=${TOKEN:-${OTHER:-hunter2}}", "export API_TOKEN=${TOKEN:-$'hunter2'}", "API_TOKEN=${TOKEN:-$\"hunter2\"}", "API_TOKEN=${TOKEN:-${OTHER:-${THIRD:-hunter2}}}", "export API_TOKEN=${TOKEN:-${PREFIX}$'hunter2'}", "export GITHUB_PASSPHRASE=hunter2", "PASSPHRASE=abc123", "API_TOKEN=${TOKEN:-$'hunter{2'}", "API_TOKEN=${TOKEN:-$'$ecret42'}", "API_TOKEN=${TOKEN:-'$ecret42'}", "API_TOKEN=\"${TOKEN:-\"hunter2\"}\"", "tool --passphrase hunter2",
    // bash: `PREFIX=hunter2` then `API_TOKEN=$PREFIX-hunter2` prints `hunter2-hunter2` — the copy is followed by typed text,
    // and `${TOKEN:-$PREFIX-hunter2}` and `${TOKEN:-${OTHER:-$PREFIX-hunter2}}` print the same.
    "API_TOKEN=$PREFIX-hunter2",
    "API_TOKEN=$PREFIX-hunter2 curl x",
    "API_TOKEN=\"${TOKEN:-$PREFIX-hunter2}\"",
    "API_TOKEN=${TOKEN:-${OTHER:-$PREFIX-hunter2}}",
    // bash: `API_TOKEN="${TOKEN:-$(echo })}hunter2"` prints `}hunter2` — the `}` stays inside the command substitution.
    "API_TOKEN=${TOKEN:-$(echo })}hunter2",
    // bash: `API_TOKEN=${TOKEN:-hunter{2}` prints `hunter{2` — only a `${` opens another expansion, a bare `{` is text.
    "API_TOKEN=${TOKEN:-hunter{2}",
    // bash: each of these prints text someone typed: `hunter2"`, `hunter2`, `hunter2` and the user name plus `hunter2`.
    "API_TOKEN=${TOKEN:-\"hunter2$(printf '\"')\"}",
    "API_TOKEN=${TOKEN:-${OTHER:-''hunter2}}",
    "API_TOKEN=${TOKEN:-${OTHER:-$''hunter2}}",
    "API_TOKEN=${TOKEN:-${OTHER:-$(whoami)hunter2}}",
    "API_TOKEN=${TOKEN:-${OTHER:-`whoami`hunter2}}",
    // bash: `API_TOKEN=${TOKEN:-"hunter2$(whoami # don't log` then a new line `)"}` prints `hunter2` followed by the user
    // name — the `#` starts a comment, so the apostrophe is text and the substitution closes on the next line.
    "API_TOKEN=${TOKEN:-\"hunter2$(whoami # don't log\n)\"}",
    "API_TOKEN=${TOKEN:-\"hunter2$(printf X # it's fine\n)\"}",
    // bash: `API_TOKEN=${TOKEN:-$OTHER}}hunter2` prints `}hunter2` — the second `}` is part of the assignment word, and
    // `hunter2` after it is typed.
    "API_TOKEN=${TOKEN:-$OTHER}}hunter2",
    // bash: `unset TOKEN OTHER; API_TOKEN=${TOKEN:-${OTHER-hunter2}}` prints `hunter2`, and `${OTHER=hunter2}` assigns the
    // same — `-` and `=` with no colon are the unset-only default and assignment operators.
    "API_TOKEN=${TOKEN:-${OTHER-hunter2}}",
    "API_TOKEN=${TOKEN:-${OTHER=hunter2}}",
    // bash: `unset TOKEN OTHER COPY X; API_TOKEN=${TOKEN:-${OTHER-hunter2${COPY:-$X}}}` prints `hunter2` — the outer
    // unset-only default is the whole word, so the `:-` inside it must not hide the `-` that carries `hunter2`.
    "API_TOKEN=${TOKEN:-${OTHER-hunter2${COPY:-$X}}}",
    // bash: `unset arr; API_TOKEN=${arr[0]:-hunter2}` prints `hunter2` — the operator sits after the subscript.
    "API_TOKEN=${arr[0]:-hunter2}",
    // bash: `PASSPHRASE=`printf RAN`hunter2` prints `RANhunter2` — the command runs and the typed `hunter2` follows it.
    "PASSPHRASE=`printf RAN`hunter2",
    // bash: `PASSPHRASE=$' hunter2'` prints a blank and then `hunter2`, which is typed text.
    "PASSPHRASE=$' hunter2'"])("flags %j", (text) => expect(looksSecret(text)).toBe(true));
  it.each(["ls -la", "why is the build failing?", "git log --oneline", "export PATH=$PATH:/x", "echo $API_KEY", "how do I set an api key", "TOKEN=$(gh auth token)", "TOKEN=${VAR}", "TOKEN=${VAR:-$OTHER}", "TOKEN=${VAR:-}", "API_TOKEN=${TOKEN:-$(gh auth token)}", "PATH=${PATH:-/usr/local/bin}", "BYPASS=${BYPASS:-1}", "API_TOKEN=${TOKEN:-`gh auth token`}", "API_TOKEN=\"${TOKEN:-`gh auth token`}\"", "TOKEN=\"${VAR}\"", "TOKEN=\"$VAR\"", "TOKEN=\"$(gh auth token)\"", "API_TOKEN=${TOKEN:-${OTHER}}", "API_TOKEN=${TOKEN:-$OTHER}", "API_TOKEN=${TOKEN:- }", "TOKEN=\"\"", "TOKEN=\"`gh auth token`\"", "API_TOKEN=${TOKEN:-${OTHER:-$ANOTHER}}", "API_TOKEN=${TOKEN:-${OTHER:-${THIRD}}}", "API_TOKEN=${TOKEN:-${OTHER:- }}", "PASSPHRASE=$(op read op://vault/item)", "PASSPHRASE=${PASSPHRASE:-$OTHER}", "API_TOKEN=\"${TOKEN:-$OTHER}\"",
    // bash: `API_TOKEN="${TOKEN:-$(echo })}"` prints `}` — the `}` belongs to the command substitution, so the expansion
    // closes at the last `}` and the value is that command's output, never text someone typed.
    "API_TOKEN=\"${TOKEN:-$(echo })}\"",
    "API_TOKEN=${TOKEN:-$(echo })}",
    "API_TOKEN=\"${TOKEN:-$(echo $(echo x))}\"",
    "API_TOKEN=${TOKEN:-`echo }`}",
    // bash: `OTHER=x; TOKEN=$OTHER;true` leaves `x` in `TOKEN` — `;`, `&` and `|` end the assignment word.
    "TOKEN=$OTHER;true",
    "TOKEN=$OTHER&&true",
    "TOKEN=$OTHER|cat",
    // bash: `PASSPHRASE=$"`id`"` prints the output of `id`, and `PASSPHRASE=$" ${VAR}"` prints `VAR`'s value — a `$"..."`
    // value that starts with a command or a copy holds no text someone typed.
    "PASSPHRASE=$\"`id`\"",
    "PASSPHRASE=$\" `id`\"",
    "PASSPHRASE=$\" ${VAR}\"",
    // bash: `API_TOKEN=${TOKEN:-${OTHER-$COPY}}` and `API_TOKEN=${OTHER-$COPY}` print `COPY`'s value — an unset-only default
    // that only copies a variable is not typed text.
    "API_TOKEN=${TOKEN:-${OTHER-$COPY}}",
    "API_TOKEN=${OTHER-$COPY}",
    // bash: `unset TOKEN OTHER; API_TOKEN=${TOKEN:-$OTHER}}` prints only `}` — a `}` that closes nothing is a literal
    // character of the word, and on its own it is no typed secret.
    "API_TOKEN=${TOKEN:-$OTHER}}",
    // bash: `TOKEN=abcdefgh; API_TOKEN=${TOKEN:0:${COPY:-2}}` prints `ab`, and `${TOKEN#*:-hunter2}` prints `abcdefgh` —
    // the operator the shell acts on is the one after the parameter (or its subscript), and these words only cut a copy.
    "API_TOKEN=${TOKEN:0:${COPY:-2}}",
    "API_TOKEN=${TOKEN#*:-hunter2}",
    // bash: `PASSPHRASE=`printf RAN`` and `PASSPHRASE=$(printf RAN)` print `RAN`, and `PASSPHRASE=$'   '` prints only
    // blanks — a value the shell runs or that holds nothing is no text someone typed.
    "PASSPHRASE=`printf RAN`",
    "PASSPHRASE=$(printf RAN)",
    "PASSPHRASE=$'   '"])("passes %j", (text) => expect(looksSecret(text)).toBe(false));
  it("reads a very long line without a stack overflow, and a value past the bound is a typed secret", () => {
    // 20,000 characters of nesting would recurse far past the stack if read in full.
    for (const nested of ['"'.repeat(20000), '"${A:-'.repeat(4000), '"$('.repeat(7000), "${A:-".repeat(4000) + "}".repeat(4000)]) {
      expect(() => looksSecret(`API_TOKEN=${nested}`)).not.toThrow();
      expect(looksSecret(`API_TOKEN=${nested}`)).toBe(true);
    }
    // A copy or a command is still not a secret, however long the rest of the line is.
    expect(looksSecret(`API_TOKEN=$(gh auth token) ${"echo ok ".repeat(5000)}`)).toBe(false);
  });
});
