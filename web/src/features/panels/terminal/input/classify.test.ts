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
  it.each(["sk-ant-api03-abcdefghijklmnopqrstuvwx", "ghp_abcdefghijklmnopqrstuvwxyz0123456789", "AKIAIOSFODNN7EXAMPLE", "export OPENAI_API_KEY=abc123", "TOKEN=abcdef curl x", "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2", "curl -H 'Authorization: Bearer abcdefghijklmnop1234'", "export DB_PASSWORD=hunter2", "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVo0MTIzNDU2Nzg5", "export API_TOKEN=${TOKEN:-hunter2}", "DB_PASS=${PASS:=s3cret}", "DB_PASS=abc123", "export API_TOKEN=\"${TOKEN:-a b}\"", "API_TOKEN=$'hunter2'", "API_TOKEN=$\"hunter2\"", "API_TOKEN=\"$'hunter2'\"", "TOKEN=${1:-hunter2}", "export DB_PASSWORD=\"hunter2\"", "API_TOKEN=${TOKEN:-${PREFIX}hunter2}", "API_TOKEN=${TOKEN:-' hunter2'}", "export DB_PASSWORD=\" hunter2\"", "export DB_PASSWORD=' hunter2'", "export DB_PASSWORD=\"  hunter2  \"", "API_TOKEN=${TOKEN:-${OTHER:-hunter2}}", "export API_TOKEN=${TOKEN:-$'hunter2'}", "API_TOKEN=${TOKEN:-$\"hunter2\"}", "API_TOKEN=${TOKEN:-${OTHER:-${THIRD:-hunter2}}}", "export API_TOKEN=${TOKEN:-${PREFIX}$'hunter2'}", "export GITHUB_PASSPHRASE=hunter2", "PASSPHRASE=abc123", "tool --passphrase hunter2"])("flags %j", (text) => expect(looksSecret(text)).toBe(true));
  it.each(["ls -la", "why is the build failing?", "git log --oneline", "export PATH=$PATH:/x", "echo $API_KEY", "how do I set an api key", "TOKEN=$(gh auth token)", "TOKEN=${VAR}", "TOKEN=${VAR:-$OTHER}", "TOKEN=${VAR:-}", "API_TOKEN=${TOKEN:-$(gh auth token)}", "PATH=${PATH:-/usr/local/bin}", "BYPASS=${BYPASS:-1}", "API_TOKEN=${TOKEN:-`gh auth token`}", "API_TOKEN=\"${TOKEN:-`gh auth token`}\"", "TOKEN=\"${VAR}\"", "TOKEN=\"$VAR\"", "TOKEN=\"$(gh auth token)\"", "API_TOKEN=${TOKEN:-${OTHER}}", "API_TOKEN=${TOKEN:-$OTHER}", "API_TOKEN=${TOKEN:- }", "TOKEN=\"\"", "TOKEN=\"`gh auth token`\"", "API_TOKEN=${TOKEN:-${OTHER:-$ANOTHER}}", "API_TOKEN=${TOKEN:-${OTHER:-${THIRD}}}", "API_TOKEN=${TOKEN:-${OTHER:- }}", "PASSPHRASE=$(op read op://vault/item)", "PASSPHRASE=${PASSPHRASE:-$OTHER}"])("passes %j", (text) => expect(looksSecret(text)).toBe(false));
});
