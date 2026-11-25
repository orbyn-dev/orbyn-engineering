import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { installMockFetch } from "../../test-support";
import { SkillsPage } from "../skills/SkillsPage";
import { SettingsPage } from "./SettingsPage";

/**
 * One page-title scale (#1115): a Settings page title is the size of every other page's title. Settings draws its visible
 * title in a span, with the h1 left to the screen reader, so a settings stylesheet can size it apart from the h1 the other
 * pages draw. This compares the rules that size each page's title, following their token through tokens.css.
 */
beforeEach(() => {
  installMockFetch();
  window.location.hash = "";
});

const SRC = join(process.cwd(), "src");

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)],
  );
}

/** Every stylesheet under src, by its path from src. */
const STYLESHEETS = Object.fromEntries(
  walk(SRC)
    .filter((file) => file.endsWith(".css"))
    .map((file) => [relative(SRC, file).replaceAll("\\", "/"), readFileSync(file, "utf8")]),
);

const tokens = STYLESHEETS["styles/tokens.css"] ?? "";

/**
 * The rule each page sizes its title with: the class Settings draws the visible title in, and the one the other pages draw
 * (a skill's h1, and the crumb title a thread and the onboarding draft share). A thread topbar's own h1 is left out: the
 * Reading layout compacts that topbar, so its crumbs are sized to fit it (layouts.css).
 */
const SETTINGS_TITLE = ".settings-title";
const PAGE_TITLE_HOOKS = [SETTINGS_TITLE, ".skills-heading h1", ".crumb-title"];

/** A px token from tokens.css. */
function tokenPx(name: string): number {
  const found = new RegExp(`--${name}:\\s*(\\d+(?:\\.\\d+)?)px`).exec(tokens)?.[1];
  if (found === undefined) throw new Error(`--${name} is not a pixel token`);
  return Number(found);
}

/** The px a declaration body asks for, following a var() through tokens.css. */
function declaredPx(body: string): number | undefined {
  const value = /(?:^|;)\s*font-size:\s*([^;]+)/.exec(body)?.[1]?.trim();
  if (value === undefined) return undefined;
  const token = /^var\(--([\w-]+)\)$/.exec(value)?.[1];
  return token === undefined ? Number.parseFloat(value) : tokenPx(token);
}

/** Every rule that sizes one of `hooks`, as `["file: selector", px]`, optionally only under `folder`. */
function titleSizes(hooks: readonly string[], folder?: string): [string, number][] {
  return Object.entries(STYLESHEETS)
    .filter(([path]) => folder === undefined || path.startsWith(folder))
    .flatMap(([path, css]) =>
      Array.from(css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g)).flatMap(([, list = "", body = ""]) => {
        const px = declaredPx(body);
        if (px === undefined) return [];
        return list
          .split(",")
          .map((one) => one.trim())
          .filter((one) => hooks.some((hook) => one.includes(hook)))
          .map((one): [string, number] => [`${path}: ${one}`, px]);
      }),
    );
}

/** The sizes the title rules ask for, so two different tokens show up as two entries. */
const sizesOf = (hooks: readonly string[], folder?: string) => [...new Set(titleSizes(hooks, folder).map(([, px]) => px))];

describe("the page title's size", () => {
  it("is one size on every page", () => {
    expect(sizesOf(PAGE_TITLE_HOOKS)).toEqual([tokenPx("text-page")]);
  });

  it("is the shared page-title rule for Settings, which sets none of its own", () => {
    // settingsDesign.css adds to the shared rules in pages.css without touching them. A settings rule that re-declared
    // the title's size is what drew Settings' headings smaller than every other page's (#1115).
    expect(titleSizes([SETTINGS_TITLE], "features/settings/")).toEqual([]);
    expect(titleSizes([SETTINGS_TITLE]).map(([where]) => where)).toEqual(["styles/pages.css: .settings-title"]);
  });

  it("is the same on a rendered Settings page and a page outside it", async () => {
    const settings = render(<SettingsPage section="general" onClose={() => undefined} />);
    await screen.findAllByRole("heading", { level: 1 });
    const settingsTitle = settings.container.querySelector(SETTINGS_TITLE);
    expect(settingsTitle).not.toBeNull();
    expect(settingsTitle).toHaveAttribute("aria-hidden");
    expect(settingsTitle?.textContent).not.toBe("");

    const other = render(<SkillsPage />);
    const pageTitle = await waitFor(() => {
      const title = other.container.querySelector(".skills-heading h1");
      expect(title).not.toBeNull();
      return title;
    });
    expect(pageTitle).toHaveAttribute("data-page-title");

    // Both titles are drawn by a hook the rules above size, and those rules agree.
    expect(sizesOf([SETTINGS_TITLE, ".skills-heading h1"])).toEqual([tokenPx("text-page")]);
  });
});
