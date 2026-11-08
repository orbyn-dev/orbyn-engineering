import { expect, it } from "vitest";
import { clockTime } from "./format";

const evening = new Date(2026, 10, 8, 17, 59, 22).toISOString();
const morning = new Date(2026, 10, 8, 9, 5, 7).toISOString();

it("writes a 12-hour clock without a leading zero and with AM or PM in capitals", () => {
  expect(clockTime(evening, "12-hour")).toMatch(/^5:59:22\s?PM$/i);
  expect(clockTime(evening, "12-hour")).not.toMatch(/pm/);
  expect(clockTime(morning, "12-hour")).toMatch(/^9:05:07\s?AM$/);
});

it("writes a 24-hour clock with two-digit hours", () => {
  expect(clockTime(morning, "24-hour")).toBe("09:05:07");
  expect(clockTime(evening, "24-hour")).toBe("17:59:22");
});

it("follows the system for the default and never lowercases the marker", () => {
  expect(clockTime(evening)).not.toMatch(/\b(am|pm)\b/);
  expect(clockTime("not a time")).toBe("");
});
