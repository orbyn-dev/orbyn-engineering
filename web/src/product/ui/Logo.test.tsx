import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { Logo, LogoTile, type LogoSize } from "./Logo";

const SIZES: LogoSize[] = [16, 20, 24, 28, 48, 64];

describe("Logo", () => {
  it.each(SIZES)("renders at %i px with the ring and prompt on currentColor and the dot on the accent token", (size) => {
    const { container } = render(<Logo size={size} />);
    const svg = container.querySelector("svg.logo");
    expect(svg).toHaveAttribute("width", String(size));
    expect(svg).toHaveAttribute("height", String(size));
    expect(svg).toHaveAttribute("aria-hidden", "true");
    const strokes = Array.from(svg?.querySelectorAll("[stroke]") ?? []);
    expect(strokes).toHaveLength(3);
    for (const stroke of strokes) expect(stroke).toHaveAttribute("stroke", "currentColor");
    const dots = Array.from(svg?.querySelectorAll("[fill]") ?? []).filter((node) => node.getAttribute("fill") !== "none");
    expect(dots).toHaveLength(1);
    expect(dots[0]).toHaveAttribute("fill", "var(--logo-accent)");
  });

  it("draws 16, 20 and 24 on their own pixel grid and every other size from the 512 drawing", () => {
    const viewBox = (size: LogoSize) => render(<Logo size={size} />).container.querySelector("svg")?.getAttribute("viewBox");
    expect([16, 20, 24].map((size) => viewBox(size as LogoSize))).toEqual(["0 0 16 16", "0 0 20 20", "0 0 24 24"]);
    expect([28, 48, 64].map((size) => viewBox(size as LogoSize))).toEqual(["0 0 512 512", "0 0 512 512", "0 0 512 512"]);
  });

  it("puts the mark on a tile at 40 px with a 28 px mark, or 28 px with a 20 px mark", () => {
    const large = render(<LogoTile size={40} />).container;
    expect(large.querySelector(".logo-tile")).toHaveAttribute("data-size", "40");
    expect(large.querySelector("svg")).toHaveAttribute("data-logo", "28");
    const small = render(<LogoTile size={28} />).container;
    expect(small.querySelector(".logo-tile")).toHaveAttribute("data-size", "28");
    expect(small.querySelector("svg")).toHaveAttribute("data-logo", "20");
  });
});

describe("logo tokens", () => {
  const css = readFileSync(join(import.meta.dirname, "../../styles/tokens.css"), "utf8");
  /** The declarations of the first rule whose selector is exactly `selector`. */
  const rule = (selector: string) => {
    const start = css.indexOf(`${selector} {`);
    expect(start, selector).toBeGreaterThanOrEqual(0);
    return css.slice(start, css.indexOf("\n}", start));
  };
  const themes: [string, string][] = [
    ["Light", ":root"],
    ["Dark", ':root[data-theme="dark"]'],
    ["Dim", ':root[data-theme="dark"][data-preset="dim"]'],
  ];

  it.each(themes)("defines the dot and the tile in %s", (_name, selector) => {
    const block = rule(selector);
    for (const token of ["--logo-accent", "--logo-tile-bg", "--logo-tile-ink"]) expect(block, token).toMatch(new RegExp(`${token}:\\s*#[0-9a-f]{6};`, "i"));
  });

  it("uses the blue of the designs: #3b82f6 in light, #60a5fa in dark and dim", () => {
    expect(rule(":root")).toContain("--logo-accent: #3b82f6;");
    expect(rule(':root[data-theme="dark"]')).toContain("--logo-accent: #60a5fa;");
    expect(rule(':root[data-theme="dark"][data-preset="dim"]')).toContain("--logo-accent: #60a5fa;");
  });
});
