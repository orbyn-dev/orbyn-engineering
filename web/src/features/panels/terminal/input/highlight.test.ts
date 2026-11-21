import { describe, expect, it } from "vitest";
import { tokenize } from "./highlight";

const kinds = (line: string) => tokenize(line).filter((token) => token.kind !== "space").map((token) => `${token.kind}:${token.text}`);

describe("tokenize", () => {
  it("colours the command, its flags, strings and variables", () => {
    expect(kinds('git commit -m "fix it" $HOME')).toEqual(["command:git", "plain:commit", "flag:-m", 'string:"fix it"', "variable:$HOME"]);
  });
  it("starts a new command after an operator", () => {
    expect(kinds("ls -la | grep foo && echo done")).toEqual(["command:ls", "flag:-la", "operator:|", "command:grep", "plain:foo", "operator:&&", "command:echo", "plain:done"]);
  });
  it("skips a leading assignment", () => {
    expect(kinds("FOO=1 node app.js")).toEqual(["plain:FOO=1", "command:node", "plain:app.js"]);
  });
  it("marks a comment and an unterminated string", () => {
    expect(kinds("ls # all\necho 'open")).toContain("comment:# all");
    expect(kinds("echo 'open")).toEqual(["command:echo", "string:'open"]);
  });
  it.each(["", "  ls  ", 'a "b" $(c) | d; e # f', "multi\nline\n", "tab\there", "$", "\"", "ünï ✓"])("joins back to %j", (line) => {
    expect(tokenize(line).map((token) => token.text).join("")).toBe(line);
  });
});
