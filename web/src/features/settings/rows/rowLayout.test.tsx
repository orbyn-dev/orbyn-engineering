import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Segmented } from "../../../ui/Segmented";
import { SettingRow } from "./SettingRow";

/** jsdom has no layout, so the stylesheet is read: a row must wrap or stack, never clip or cover its control. */
const read = (path: string) => readFileSync(join(process.cwd(), path), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const settings = read("src/features/settings/settingsDesign.css");
const pages = read("src/styles/pages.css");
const rule = (css: string, selector: string) => {
  for (const [, found = "", body = ""] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) if (found.trim() === selector) return body;
  return "";
};
const row = ".settings-page .setting-line:not(.setting-line-stacked)";

describe("a setting row in a narrow settings pane", () => {
  it("wraps the control onto its own line when the label block cannot keep its width", () => {
    expect(rule(settings, row)).toMatch(/flex-wrap: wrap;/);
    // The basis is what wraps the line: the label keeps 180px, and a control that no longer fits beside it moves down.
    expect(rule(settings, `${row} > .setting-line-text`)).toMatch(/flex-basis: 180px;/);
    // The control is never squeezed to fit, so the row wraps rather than drawing it over the description.
    expect(rule(pages, ".setting-line-control")).toMatch(/flex: none;/);
    // On a line of its own it may use the whole row rather than the half the shared rule allows.
    expect(rule(pages, ".setting-line-control")).toMatch(/max-width: 50%;/);
    expect(rule(settings, `${row} > .setting-line-control`)).toMatch(/max-width: 100%;/);
  });

  it("takes the label basis the shared rules already give a segmented row, adding no breakpoint of its own", () => {
    expect(pages).toMatch(/\.setting-line:has\(\.segmented\) > \.setting-line-text \{\s*flex: 1 1 180px;/);
    // A viewport rule would miss the reported case: at a 1024 px window the pane is about 484 px wide.
    for (const [, body = ""] of settings.matchAll(/@media[^{]+\{([\s\S]*?)\n\}/g)) expect(body).not.toContain(".setting-line");
  });

  it("keeps the fixed inheritance popover on the window, which a container query would re-anchor", () => {
    // The popover is inline in the row and places itself from getBoundingClientRect(); containment would make
    // that box the containing block for the fixed element and shift it.
    expect(pages).toMatch(/\.setting-inheritance-popup \{\s*position: fixed;/);
    expect(settings).not.toMatch(/@container|container(-type)?:/);
  });

  it("wraps a segmented control's choices inside the width it has, instead of under the description", () => {
    const segmented = rule(settings, ".settings-page .setting-line-control .segmented");
    expect(segmented).toMatch(/max-width: 100%;/);
    expect(segmented).toMatch(/flex-wrap: wrap;/);
  });

  it("keeps the control a sibling of the label block, so it has a line of its own to drop to", () => {
    const { container } = render(
      <div className="settings-page"><div className="settings-body">
        <SettingRow title="Quit confirmation" description="How ⌘Q quits the desktop app on macOS">
          <Segmented label="Quit confirmation" value="direct" options={[{ value: "direct", label: "Quit at once" }, { value: "hold", label: "Hold ⌘Q" }, { value: "double_press", label: "Press ⌘Q twice" }]} onChange={() => {}} />
        </SettingRow>
      </div></div>,
    );
    const line = container.querySelector(".setting-line")!;
    const text = line.children[0]!;
    const control = line.children[1]!;
    expect(text.classList.contains("setting-line-text")).toBe(true);
    expect(control.classList.contains("setting-line-control")).toBe(true);
    expect(text.contains(control)).toBe(false);
    expect(control.querySelector(".segmented")).not.toBeNull();
  });

  it("leaves a stacked row stacked, with its control in the row rather than beside the label", () => {
    const { container } = render(<SettingRow title="Layout" stacked><span>cards</span></SettingRow>);
    expect(container.querySelector(".setting-line")!.className).toBe("setting-line setting-line-stacked");
  });
});
