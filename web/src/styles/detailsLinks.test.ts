import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const css = readFileSync(join(process.cwd(), "src/styles/detailsCard.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const rule = (selector: string) => css.split("}").find((block) => block.trim().startsWith(`${selector} {`)) ?? "";

it("sets the Pull requests row's action links in the card's own type, wrapping, not in mono or underlined", () => {
  const link = rule(".drow .dv small .dlinks .dlink");
  expect(link).toMatch(/font-family:\s*var\(--font-sans\)/);
  expect(link).toMatch(/text-decoration:\s*none/);
  expect(rule(".drow .dlinks")).toMatch(/white-space:\s*normal/);
});
