import { expect, it } from "vitest";
import { profileFixtures } from "../../../mock/profiles";
import { availabilityPill, availabilityText, checkDraft, duplicateName, emptyDraft, grantsFromFailure, grantsOf, modeLabel, plain, profileLine, tierLabel, usable } from "./model";

it("carries grants only for full access and Autopilot Full, never for the three built-ins", () => {
  const [careful, fast, cheap, , , hands] = profileFixtures();
  for (const builtin of [careful!, fast!, cheap!]) expect(grantsOf(builtin)).toEqual([]);
  expect(grantsOf(hands!)).toEqual(["full_access", "autopilot_full"]);
  expect(grantsOf({ permission_mode: "auto", autopilot_mode: "full" })).toEqual(["autopilot_full"]);
  expect(grantsOf({ permission_mode: "full_access", autopilot_mode: "on" })).toEqual(["full_access"]);
});

it("reads the grants a confirm_required names, keeping only known codes once each", () => {
  expect(grantsFromFailure({ grants: ["full_access", "full_access", "root", 4] })).toEqual(["full_access"]);
  expect(grantsFromFailure(undefined)).toEqual([]);
  expect(grantsFromFailure({ grants: "full_access" })).toEqual([]);
});

it("shows a word the app does not know as it came, cut, and never reads an inherited key", () => {
  expect(modeLabel("auto_edits")).toBe("Auto edits");
  expect(modeLabel("read_only")).toBe("read_only");
  expect(tierLabel("__proto__")).toBe("__proto__");
  expect(tierLabel("constructor")).toBe("constructor");
  expect(plain("x".repeat(100), 10)).toBe(`${"x".repeat(10)}…`);
  expect(plain({ a: 1 })).toBe("");
});

it("checks the name: required, at most 60, no control characters, unique ignoring case", () => {
  const others = [{ id: "a", name: "Fast" }];
  const draft = { ...emptyDraft(), name: "fast " };
  expect(checkDraft(draft, others, null).name).toMatch(/already exists/);
  expect(checkDraft(draft, others, "a").name).toBeUndefined();
  expect(checkDraft({ ...draft, name: "  " }, others, null).name).toBe("Give the profile a name.");
  expect(checkDraft({ ...draft, name: "n".repeat(61) }, others, null).name).toMatch(/at most 60/);
  expect(checkDraft({ ...draft, name: "bad\u0007name" }, others, null).name).toMatch(/letters/);
  expect(checkDraft({ ...draft, name: "ok", description: "d".repeat(201) }, others, null).description).toMatch(/at most 200/);
});

it("picks the first free numbered duplicate name", () => {
  expect(duplicateName("Fast", ["Fast"])).toBe("Fast (2)");
  expect(duplicateName("Fast", ["Fast", "fast (2)"])).toBe("Fast (3)");
});

it("says why an engine cannot run a profile, and treats that profile as unusable", () => {
  expect(availabilityText({ engine: "not_found" }, "Codex")).toEqual({ text: "Codex is not installed.", fix: true });
  expect(availabilityText({ engine: "cannot_start", reason: "No sign-in." }, "Codex")?.text).toBe("No sign-in.");
  expect(availabilityText({ engine: "ok" }, "Codex")).toBeNull();
  expect(availabilityText({ engine: "none" }, "x")).toBeNull();
  expect(availabilityText({ engine: "brand_new" as never }, "x")?.text).toMatch(/can’t tell/);
  expect(availabilityPill({ engine: "not_found" })).toBe("Engine not found");
  expect(availabilityPill({ engine: "cannot_start" })).toBe("Can’t start");
  expect(usable({ availability: { engine: "not_found" } })).toBe(false);
  expect(usable({ availability: { engine: "disabled" } })).toBe(false);
  expect(usable({ availability: { engine: "ok" } })).toBe(true);
  expect(usable({})).toBe(true);
});

it("writes the second line of a row", () => {
  const [careful] = profileFixtures();
  expect(profileLine(careful!, "Keep the thread’s engine")).toBe("Keep the thread’s engine · Builder · Supervised · Autopilot Off");
});
