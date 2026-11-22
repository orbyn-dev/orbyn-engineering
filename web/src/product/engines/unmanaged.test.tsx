import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { EngineStatus } from "../../api/types";
import { UnmanagedLabel } from "./UnmanagedLabel";
import { askBeforeText, isUnmanaged, unmanagedReason } from "./unmanaged";

const engine = (capabilities: unknown) => ({ id: "kiro", name: "Kiro", capabilities }) as EngineStatus;
const cursor = { id: "cursor", name: "Cursor", capabilities: { managed: false } } as EngineStatus;

describe("the unmanaged reason", () => {
  it("keeps Cursor's own words, because Cursor has no approvals", () => {
    expect(askBeforeText("cursor", "Cursor")).toBe("Cursor does not ask before acting.");
    expect(unmanagedReason(cursor)).toBe("Cursor does not ask before acting.");
  });

  it("says Orbyn cannot ask for every other engine, and never claims a sandbox", () => {
    expect(askBeforeText("kiro", "Kiro")).toBe("Orbyn cannot ask before Kiro acts.");
    expect(unmanagedReason(engine({ managed: false }))).toBe("Orbyn cannot ask before Kiro acts.");
    expect(unmanagedReason(engine({ managed: false }))).not.toMatch(/sandbox/i);
  });
});

describe("isUnmanaged", () => {
  it("is true only when the service says managed is false", () => {
    expect(isUnmanaged(engine({ managed: false }))).toBe(true);
    expect(isUnmanaged(engine({ managed: true }))).toBe(false);
  });

  it("is false when the field is missing, empty or there is no engine", () => {
    expect(isUnmanaged(engine({}))).toBe(false);
    expect(isUnmanaged({ id: "unknown", name: "Unknown" } as EngineStatus)).toBe(false);
    expect(isUnmanaged(engine(undefined))).toBe(false);
    expect(isUnmanaged(null)).toBe(false);
    expect(isUnmanaged(undefined)).toBe(false);
  });
});

describe("UnmanagedLabel", () => {
  it("draws the visible word for an engine Orbyn cannot ask, with the icon it is given", () => {
    render(<UnmanagedLabel engine={engine({ managed: false })} icon={<svg data-testid="shield" />} />);
    expect(screen.getByText("Unmanaged")).toBeVisible();
    expect(screen.getByTestId("shield")).toBeInTheDocument();
  });

  it("carries the engine's own reason on hover", () => {
    const { container, unmount } = render(<UnmanagedLabel engine={engine({ managed: false })} />);
    expect(container.querySelector(".unmanaged-label")).toHaveAttribute("title", "Orbyn cannot ask before Kiro acts.");
    unmount();
    const old = render(<UnmanagedLabel engine={cursor} />);
    expect(old.container.querySelector(".unmanaged-label")).toHaveAttribute("title", "Cursor does not ask before acting.");
  });

  it("draws nothing for a managed engine", () => {
    const { container } = render(<UnmanagedLabel engine={engine({ managed: true })} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("draws nothing when the service omits the flag", () => {
    const { container } = render(<UnmanagedLabel engine={engine({})} />);
    expect(container).toBeEmptyDOMElement();
  });
});
