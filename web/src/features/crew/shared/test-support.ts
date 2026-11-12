import { act, render, screen } from "@testing-library/react";
import { createElement } from "react";
import { describe, expect, it, vi } from "vitest";
import { App } from "../../../app/App";
import { UNAVAILABLE_REASON } from "../autopilot/review/Parts";

/** Opens a Preview frame the way a person does: by its hash, with `fetch` watched. */
export async function openFrame(name: string) {
  const fetchSpy = vi.fn();
  vi.stubGlobal("fetch", fetchSpy);
  window.location.hash = `#/preview/crew/${name}`;
  const view = render(createElement(App));
  await act(async () => {});
  await screen.findByText(/Sample data\./);
  return { ...view, fetchSpy };
}

const USAGE_LINE = "Claude Code 412k of 1M tokens · Codex 62% of its weekly limit · 2h 14m";

/** The checks every drawn Crew frame shares; `shows` is what each frame must draw, from the design source. */
export function describeFrames(title: string, ids: readonly string[], shows: Record<string, string[]>) {
  describe(title, () => {
    it("builds every frame of the slice", () => {
      expect(new Set(Object.keys(shows))).toEqual(new Set(ids));
    });

    it.each(ids)("#/preview/crew/%s draws its state and makes no request", async (name) => {
      const { container, fetchSpy } = await openFrame(name);
      for (const text of shows[name] ?? []) expect(container.textContent).toContain(text);
      expect(fetchSpy).not.toHaveBeenCalled();
    });

    it.each(ids)("#/preview/crew/%s: every control is disabled and says why", async (name) => {
      const { container } = await openFrame(name);
      for (const control of container.querySelectorAll<HTMLButtonElement>("button")) {
        expect(control).toBeDisabled();
        expect(control).toHaveAttribute("aria-description", UNAVAILABLE_REASON);
      }
      expect(container.querySelectorAll("a[href], input, select, textarea")).toHaveLength(0);
    });

    it.each(ids)("#/preview/crew/%s: Crew v22 units, no dollar figure and the per-engine usage line", async (name) => {
      const { container } = await openFrame(name);
      expect(container.textContent).not.toMatch(/\$\d/);
      const usage = container.querySelector(".cx-end");
      if (usage?.textContent?.includes("Codex")) expect(usage.textContent).toBe(USAGE_LINE);
    });
  });
}
