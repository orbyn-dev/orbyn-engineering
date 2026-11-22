import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { createCursorThreadMock } from "../../../mock/cursorThread";
import { CREW_ROLE_TOOLTIP, CrewRoleNote, crewRoleLabel } from "./CrewRole";
import { setCursorThreadApi } from "./api";
import { EngineNote } from "./EngineNote";

afterEach(() => setCursorThreadApi(null));

describe("the Engine row of Details", () => {
  it("shows the engine's name and the notice once it has been answered", async () => {
    setCursorThreadApi(createCursorThreadMock({ acknowledged: true }));
    render(<EngineNote projectId="p" engine="cursor" engineName="Cursor" />);
    expect(await screen.findByText("Cursor does not ask before acting. Its own sandbox applies.")).toBeVisible();
    expect(screen.getByText("Engine")).toBeVisible();
    // A note, not a button.
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("is drawn for every engine: another engine shows its name and the model under it, never the notice", async () => {
    setCursorThreadApi(createCursorThreadMock({ acknowledged: true }));
    render(<EngineNote projectId="p" engine="claude" engineName="Claude Code" modelName="Opus 5.5" />);
    expect(screen.getByText("Engine")).toBeVisible();
    expect(screen.getByText("Claude Code")).toBeVisible();
    expect(screen.getByText("Opus 5.5")).toBeVisible();
    await new Promise((done) => setTimeout(done, 10));
    expect(screen.queryByText(/does not ask/)).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("adds the notice only for an engine that has one and only once it has been answered", async () => {
    setCursorThreadApi(createCursorThreadMock({ acknowledged: false }));
    const before = render(<EngineNote projectId="p" engine="cursor" engineName="Cursor" />);
    await new Promise((done) => setTimeout(done, 10));
    expect(screen.getByText("Cursor")).toBeVisible();
    expect(screen.queryByText(/does not ask/)).toBeNull();
    before.unmount();
    setCursorThreadApi(null);
    render(<EngineNote projectId="p" engine="cursor" engineName="Cursor" />);
    await new Promise((done) => setTimeout(done, 10));
    expect(screen.getByText("Engine")).toBeVisible();
    expect(screen.queryByText(/does not ask/)).toBeNull();
  });

  it("uses Orbyn's own reason for another engine the service has a notice for", async () => {
    setCursorThreadApi(createCursorThreadMock({ acknowledged: true, engines: ["kiro"] }));
    render(<EngineNote projectId="p" engine="kiro" engineName="Kiro" />);
    expect(await screen.findByText("Orbyn cannot ask before Kiro acts.")).toBeVisible();
    expect(screen.queryByText(/sandbox/i)).toBeNull();
  });

  it("draws an engine's name as plain text", () => {
    render(<EngineNote projectId="p" engine="x" engineName="<b>Bold</b>" modelName="<i>m</i>" />);
    expect(document.querySelector("b, i")).toBeNull();
  });
});

describe("Reviewer only", () => {
  it("labels an engine that may only review, with and no other", () => {
    expect(crewRoleLabel({ crew_role: "reviewer_only" })).toBe("Reviewer only");
    expect(crewRoleLabel({ crew_role: "any" })).toBeNull();
    expect(crewRoleLabel({})).toBeNull();
    expect(crewRoleLabel(null)).toBeNull();
    render(<CrewRoleNote engine={{ crew_role: "reviewer_only", crew_role_reason: "Not yet confined for writing work" }} />);
    // The tooltip is the service's own reason, shown as an attribute (plain text).
    const chip = screen.getByText("Reviewer only");
    expect(chip.closest(".ct-role")).toHaveAttribute("title", "Not yet confined for writing work");
    expect(chip.closest(".ct-role")).toHaveAttribute("tabindex", "0");
  });

  it("falls back to Crew's own words when the service sends no reason", () => {
    expect(CREW_ROLE_TOOLTIP).toBe("Orbyn cannot ask before it acts");
    render(<CrewRoleNote engine={{ crew_role: "reviewer_only", crew_role_reason: null }} />);
    expect(screen.getByText("Reviewer only").closest(".ct-role")).toHaveAttribute("title", "Orbyn cannot ask before it acts");
  });

  it("draws nothing for any, and the reason is plain text", () => {
    const none = render(<CrewRoleNote engine={{ crew_role: "any", crew_role_reason: "x" }} />);
    expect(none.container).toBeEmptyDOMElement();
    none.unmount();
    render(<CrewRoleNote engine={{ crew_role: "reviewer_only", crew_role_reason: "<b>hi</b>" }} />);
    expect(document.querySelector("b")).toBeNull();
  });
});
