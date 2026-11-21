import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FRAME_LOAD_TIMEOUT, openFrame } from "../shared/test-support";
import { DASHBOARD_FRAMES } from "./names";

/** What each frame must show once it has read: text taken from its design, on the sample client. */
const SHOWS: Record<string, string[]> = {
  "dash-default": ["Dashboard", "Sample data", "This device only", "71%", "27 of 38 merges · last 60 days", "41 minmedian", "2 h 05 min", "6.1M tokens", "3.4M tokens", "2.3M tokens", "5%", "2 of 38 merges reverted", "1 reverted automatically, 1 by you · 38 merges", "Over time", "How to read this", "A rate shows from 10 merges"],
  "dash-metric": ["Reverted merges by week", "Rotate the signing key", "Reverted by you", "Reverted automatically · cause certain", "38 merges · 60 days", "All metrics", "Weeks with fewer than 10 merges are not turned into percentages."],
  "dash-breakdown": ["By engine", "claude · Sonnet 5.5", "76% merged without you · 16 of 21", "codex · GPT-5.5", "opencode · DeepSeek V4", "3 of 5 merged without you · too few for a rate", "claude’s own unit", "By area", "docs/guides", "93% merged without you · 13 of 14", "web/auth", "53% merged without you · 8 of 15", "Open Trust"],
  "dash-low-sample": ["This repo has 5 merges in the last 60 days. A rate shows from 10 merges, so each tile says what it rests on and Crew lists the merges themselves.", "5 merges · too few for a rate", "55 minmedian", "5 merges · a median needs", "The 5 merges", "Update the calendar feed token", "Add a seed script"],
  "dash-empty": ["No merges yet", "When Crew merges a change in orbyn-engine, this page shows how often it needed you", "Open the Review queue"],
  "dash-many-repos": ["All projects", "orbyn-engineeringclaude, codex, opencode", "orbyn-planner", "nothing to measure yet", "orbyn-docs", "86%", "All projects, cost per engine", "claude 8.4M tokens · codex 4.3M tokens · opencode 2.3M tokens"],
  "dash-filters": ["Project", "orbyn-engineering", "orbyn-planner", "orbyn-engine", "orbyn-docs", "All projects", "30 days", "60 days", "90 days"],
  "dash-loading": ["Dashboard", "Autonomy", "Cycle time", "Cost", "Revert rate"],
  "dash-error": ["Couldn’t read the run history on this device. Nothing was changed.", "Copy the error", "Try again", "Not shown while the history can’t be read"],
  "dash-narrow-panel": ["71%", "27 of 38 merges · last 60 days", "41 min", "Open in Crew"],
  "dash-narrow-window": ["71%", "27 of 38 merges", "41 min", "5%", "6.1M tokens"],
  "dash-area-history": ["Autopilot by area", "Repo ceiling: Full", "web/auth", "Demoted", "2 of 9", "reverted · 14 days · 9 merges", "Follows your ceiling", "No change until there is data", "3 merges, below the 5 needed", "Not measured", "Changes of mode", "Full → On", "2 reverts in 14 days, in 9 merges", "Escapes are not measured"],
};

describe("Crew dashboard frames (Crew Designs, 23. Dashboard)", () => {
  it("builds every frame of the group", () => {
    expect(new Set(Object.keys(SHOWS))).toEqual(new Set(DASHBOARD_FRAMES));
    expect(DASHBOARD_FRAMES).toHaveLength(12);
  });

  it.each(DASHBOARD_FRAMES)("#/preview/crew/%s draws its state and sends no request", async (name) => {
    const { container, fetchSpy } = await openFrame(name);
    // The first line is in the DOM at once; the figures arrive after the sample client answers, and the skeleton after 400 ms.
    await waitFor(
      () => {
        for (const text of SHOWS[name] ?? []) expect(container.textContent).toContain(text);
      },
      { timeout: 4000 },
    );
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(container.textContent).not.toMatch(/\$\d/);
  }, 2 * FRAME_LOAD_TIMEOUT);

  it("dash-loading shows the shape of the tiles after 400 ms and no number", async () => {
    const { container } = await openFrame("dash-loading");
    await waitFor(() => expect(container.querySelectorAll(".cx-t10sk").length).toBeGreaterThan(0), { timeout: 3000 });
    expect(container.textContent).not.toContain("71%");
  });

  it("dash-error offers Try again and keeps the tiles empty", async () => {
    const { container } = await openFrame("dash-error");
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(container.textContent).not.toContain("71%");
  });

  it("dash-filters opens the project menu with the period beside it", async () => {
    await openFrame("dash-filters");
    expect(await screen.findByRole("menu", { name: "Project" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "60 days" })).toHaveAttribute("aria-checked", "true");
  });

  it("dash-narrow-panel is 320 px wide with no bars and no tables", async () => {
    const { container } = await openFrame("dash-narrow-panel");
    await screen.findByText("71%");
    expect(container.querySelector(".cx-panel")).not.toBeNull();
    expect(container.querySelector(".cx-t10bars")).toBeNull();
    expect(container.querySelector(".cx-aptb")).toBeNull();
  });
}, 60_000);
