import { expect, it } from "vitest";
import type { Param } from "./contract";
import { capped, checkItem, checkKeyName, isOneLine, keychainNote, placeholdersOf, plain, scrubValue, startValues, valueProblems, valuesBody } from "./model";

const choice: Param = { name: "package", label: "Package", kind: "choice", default: "auth", required: true, choices: ["auth", "web"] };

it("finds each {{name}} once, in order", () => {
  expect(placeholdersOf("a {{x}} b {{ y }} c {{x}} {{}}")).toEqual(["x", "y", ""]);
});

it("checks an item: name, text, undeclared placeholders, parameter rules", () => {
  const ok = { name: "Run", description: "", text: "npm test -- {{package}}", params: [choice] };
  expect(checkItem(ok, [], null, 4000)).toEqual({});
  expect(checkItem({ ...ok, name: " " }, [], null, 4000).name).toBe("Give it a name.");
  expect(checkItem({ ...ok, name: "run" }, [{ id: "a", name: "Run" }], null, 4000).name).toMatch(/already used in Personal/);
  expect(checkItem({ ...ok, name: "run" }, [{ id: "a", name: "Run" }], "a", 4000).name).toBeUndefined();
  expect(checkItem({ ...ok, text: "npm test -- {{pkg}}" }, [], null, 4000).body).toMatch(/\{\{pkg\}\}, which is not a parameter/);
  expect(checkItem({ ...ok, text: "é".repeat(2001) }, [], null, 4000).body).toMatch(/at most 4,000 bytes/);
  expect(checkItem({ ...ok, params: [{ ...choice, choices: ["only"] }] }, [], null, 4000).params).toMatch(/2 to 20/);
  expect(checkItem({ ...ok, params: [choice, choice] }, [], null, 4000).params).toMatch(/twice/);
  expect(checkItem({ ...ok, params: [{ ...choice, name: "Bad-Name" }] }, [], null, 4000).params).toMatch(/lowercase/);
  expect(checkItem({ ...ok, text: "x", params: [{ name: "p", label: "p", kind: "path", default: "../x", required: false }] }, [], null, 4000).params).toMatch(/no “..”/);
  expect(checkItem({ ...ok, text: "x", params: [{ name: "p", label: "p", kind: "path", default: "/etc", required: false }] }, [], null, 4000).params).toMatch(/leading slash/);
});

it("starts each form at the defaults, and reads service-chosen keys only as own properties", () => {
  const values = startValues([choice, { name: "since", label: "Since", kind: "text", required: false }]);
  expect(values).toEqual({ package: "auth", since: "" });
  // A parameter named like an inherited key is never read off the prototype.
  const hostile: Param = { name: "constructor", label: "c", kind: "text", required: true };
  expect(valueProblems([hostile], {}).get("constructor")).toBe("Enter c.");
  expect(valuesBody([hostile], {})).toEqual({});
  expect(valuesBody([choice], { package: "web", extra: "dropped" })).toEqual({ package: "web" });
});

it("keeps Run off for a missing required value, a choice outside its list and a path outside the project", () => {
  expect(valueProblems([choice], { package: "" }).get("package")).toBe("Choose Package.");
  expect(valueProblems([choice], { package: "evil" }).get("package")).toBe("Choose one of the listed options.");
  const path: Param = { name: "dir", label: "Dir", kind: "path", required: false };
  expect(valueProblems([path], { dir: "/etc" }).get("dir")).toMatch(/inside the project/);
  expect(valueProblems([path], { dir: "a/../b" }).get("dir")).toMatch(/inside the project/);
  expect(valueProblems([path], { dir: "src/lib" }).size).toBe(0);
});

it("treats only a single clean line as safe to type into a terminal", () => {
  expect(isOneLine("npm test -- 'auth'")).toBe(true);
  expect(isOneLine("a\nb")).toBe(false);
  expect(isOneLine("a\rb")).toBe(false);
  expect(isOneLine("a\u001b[31mb")).toBe(false);
  expect(isOneLine("a\u0003")).toBe(false);
  expect(isOneLine("")).toBe(false);
});

it("refuses the key names the doc refuses, and a repeat", () => {
  expect(checkKeyName("API_BASE", [])).toBeNull();
  for (const refused of ["PATH", "HOME", "SHELL", "IFS", "LD_PRELOAD", "DYLD_X", "NODE_OPTIONS", "PYTHONPATH", "BASH_ENV", "ENV", "ORBYN_TOKEN", "CLAUDE_API_KEY", "CODEX_HOME", "CURSOR_API_KEY"]) expect(checkKeyName(refused, []), refused).toMatch(/keeps this name/);
  expect(checkKeyName("api", [])).toMatch(/capital letters/);
  expect(checkKeyName("A", ["A"])).toMatch(/already/);
  expect(checkKeyName("B", Array.from({ length: 64 }, (_, n) => `K${n}`))).toMatch(/at most 64/);
});

it("takes a secret out of text in the forms an error carries it", () => {
  const value = "p@ss word/+=\"x\"";
  const forms = [value, encodeURIComponent(value), JSON.stringify(value).slice(1, -1), btoa(value), btoa(value).replace(/=+$/, "")];
  for (const form of forms) expect(scrubValue(`failed near ${form} here`, value), form).toBe("failed near [hidden] here");
  expect(scrubValue("line one secret1\nline two", "secret1\nsecret2")).toBe("line one [hidden]\nline two");
  expect(scrubValue("nothing to hide", "")).toBe("nothing to hide");
  expect(scrubValue("a b c", "b")).toBe("a [hidden] c");
});

it("explains a Planned set and a blocked keychain", () => {
  expect(keychainNote({ keychain: "unavailable", status: "planned" })).toMatch(/not stored on this computer yet/);
  expect(keychainNote({ keychain: "blocked", status: "stored" })).toMatch(/locked/);
  expect(keychainNote({ keychain: "ok", status: "stored" })).toBeNull();
});

it("cuts text for display", () => {
  expect(capped("x".repeat(10), 4)).toEqual({ text: "xxxx", cut: true });
  expect(capped(5, 4)).toEqual({ text: "", cut: false });
  expect(plain("y".repeat(10), 3)).toBe("yyy…");
});
