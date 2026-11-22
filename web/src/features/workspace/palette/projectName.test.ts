import { expect, it } from "vitest";
import { projectNameProblem } from "./projectName";

it("refuses a name the folder cannot hold, in the words the service uses", () => {
  const invalidCharacters = 'A project name cannot contain / \\ : * ? " < > |.';
  expect(projectNameProblem("bad:name*?<x")).toBe(invalidCharacters);
  expect(projectNameProblem("../x|y")).toBe(invalidCharacters);
  expect(projectNameProblem("a/b")).toBe(invalidCharacters);
  expect(projectNameProblem("a\\b")).toBe(invalidCharacters);
  expect(projectNameProblem(".hidden")).toBe("A project name cannot start with a dot.");
  expect(projectNameProblem("trailing.")).toBe("A project name cannot end with a dot.");
  expect(projectNameProblem("nul")).toBe("Windows reserves that project name.");
  expect(projectNameProblem("CON")).toBe("Windows reserves that project name.");
});

it("accepts a name a folder can hold, and stays quiet while the field is empty", () => {
  expect(projectNameProblem("")).toBeNull();
  expect(projectNameProblem("   ")).toBeNull();
  expect(projectNameProblem("fresh-idea")).toBeNull();
  expect(projectNameProblem("My Project")).toBeNull();
  expect(projectNameProblem("notes_2")).toBeNull();
  expect(projectNameProblem("v1.2")).toBeNull();
  expect(projectNameProblem("Fresh idea")).toBeNull();
});
