import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import type { Thread } from "../../api/types";
import { ImportedThreadMarker, ImportedThreadPill } from "./ThreadBanners";

const imported: Thread = {
  id: "t-imported",
  project_id: "p-app",
  title: "Redirect loop on logout",
  engine: "claude",
  model: null,
  cwd: "/Users/dev/code/orbyn-engineering-app",
  worktree: null,
  skill_profile: null,
  active_run: null,
  last_run_state: null,
  created_at: "2026-11-22T10:00:00Z",
  updated_at: "2026-11-22T10:00:00Z",
  scratch: false,
  source: "orbyn-transcript",
  imported_at: "2026-11-22T10:00:00Z",
};

it("marks an imported thread with the date and the fresh-session promise", () => {
  render(<ImportedThreadMarker thread={imported} />);
  expect(screen.getByText("Imported 22 Nov · new messages start a fresh session")).toBeVisible();
});

it("draws no marker on a thread without a transcript source", () => {
  const { container } = render(<ImportedThreadMarker thread={{ ...imported, source: undefined, imported_at: undefined }} />);
  expect(container).toBeEmptyDOMElement();
  const { container: noDate } = render(<ImportedThreadMarker thread={{ ...imported, imported_at: undefined }} />);
  expect(noDate).toBeEmptyDOMElement();
});

it("carries the Imported pill in the header", () => {
  render(<ImportedThreadPill thread={imported} />);
  expect(screen.getByText("Imported")).toBeVisible();
});
