import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const css = readFileSync(join(process.cwd(), "src/styles/thread.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
it("insets the side chat's empty state 16px like the other tabs", () => {
  expect(/\.side-chat-pane > \.empty \{[^}]*padding:\s*12px 16px/.test(css)).toBe(true);
});
