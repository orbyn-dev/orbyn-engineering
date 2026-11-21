import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { createCursorThreadMock } from "../../../mock/cursorThread";
import { CrewRoleNote, crewRoleLabel } from "./CrewRole";
import { cursorThreadApi, setCursorThreadApi } from "./api";
import { EngineNote } from "./EngineNote";

afterEach(() => setCursorThreadApi(null));

describe("the Engine row of Details", () => {
  it("shows the engine's name and the notice once it has been answered", async () => {
    setCursorThreadApi(createCursorThreadMock({ acknowledged: true }));
    render(<EngineNote engine="cursor" engineName="Cursor" />);
    expect(await screen.findByText("Cursor does not ask before acting. Its own sandbox applies.")).toBeVisible();
    expect(screen.getByText("Engine")).toBeVisible();
    // A note, not a button.
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("is not drawn before the notice is answered, for an engine with no notice, or on a service without the route", async () => {
    setCursorThreadApi(createCursorThreadMock({ acknowledged: false }));
    const before = render(<EngineNote engine="cursor" engineName="Cursor" />);
    await new Promise((done) => setTimeout(done, 10));
    expect(screen.queryByText("Engine")).toBeNull();
    before.unmount();
    setCursorThreadApi(createCursorThreadMock({ acknowledged: true }));
    const other = render(<EngineNote engine="claude" engineName="Claude Code" />);
    await new Promise((done) => setTimeout(done, 10));
    expect(screen.queryByText("Engine")).toBeNull();
    other.unmount();
    setCursorThreadApi(null);
    render(<EngineNote engine="cursor" engineName="Cursor" />);
    await new Promise((done) => setTimeout(done, 10));
    expect(screen.queryByText("Engine")).toBeNull();
  });
});

describe("the default client", () => {
  it("sends nothing for the unmerged routes: the notice reads none and each write says it is not offered", async () => {
    const api = cursorThreadApi();
    expect(await api.notice("cursor")).toBeNull();
    await expect(api.acknowledge("cursor")).rejects.toThrow("does not offer");
    await expect(api.sendWithoutApprovals("t1", { prompt: "x" })).rejects.toThrow("does not offer");
  });
});

describe("Reviewer only", () => {
  it("labels an engine that may only review, with the service's reason, and no other", () => {
    expect(crewRoleLabel({ crew_role: "reviewer_only" })).toBe("Reviewer only");
    expect(crewRoleLabel({ crew_role: "any" })).toBeNull();
    expect(crewRoleLabel({})).toBeNull();
    expect(crewRoleLabel(null)).toBeNull();
    render(<CrewRoleNote engine={{ crew_role: "reviewer_only", crew_role_reason: "Not yet confined for writing work" }} />);
    expect(screen.getByText("Reviewer only · Not yet confined for writing work")).toHaveAttribute("title", "Not yet confined for writing work");
  });

  it("draws nothing for any, and the reason is plain text", () => {
    const none = render(<CrewRoleNote engine={{ crew_role: "any", crew_role_reason: "x" }} />);
    expect(none.container).toBeEmptyDOMElement();
    none.unmount();
    render(<CrewRoleNote engine={{ crew_role: "reviewer_only", crew_role_reason: "<b>hi</b>" }} />);
    expect(document.querySelector("b")).toBeNull();
  });
});
