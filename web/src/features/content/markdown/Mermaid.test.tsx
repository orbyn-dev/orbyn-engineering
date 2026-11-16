import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const initialize = vi.fn();
const draw = vi.fn(async () => ({ svg: '<svg data-diagram="yes"><g/></svg>' }));
vi.mock("mermaid", () => ({ default: { initialize: (config: unknown) => initialize(config), render: draw } }));

import { Mermaid } from "./Mermaid";

beforeEach(() => {
  initialize.mockClear();
  draw.mockClear();
});

describe("Mermaid", () => {
  it("initialises Mermaid at its strict security level, and draws the SVG inline", async () => {
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    await waitFor(() => expect(screen.getByRole("img", { name: "Diagram" })).toBeInTheDocument());
    expect(initialize).toHaveBeenCalledWith(expect.objectContaining({ startOnLoad: false, securityLevel: "strict" }));
    // Strict keeps the SVG in the page. "sandbox" would wrap it in an iframe, which the question view's rule and the
    // end-to-end checks (".md-diagram svg", no iframe in the panel) do not allow.
    expect(container.querySelector(".md-diagram svg")).not.toBeNull();
    expect(container.querySelector("iframe")).toBeNull();
  });

  it("shows the source while a diagram is not drawn", () => {
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    expect(container.querySelector(".md-diagram")).toBeNull();
  });
});
