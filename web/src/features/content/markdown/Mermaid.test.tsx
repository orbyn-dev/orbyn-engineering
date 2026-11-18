import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const initialize = vi.fn();
const draw = vi.fn(async (..._args: unknown[]) => ({ svg: '<svg xmlns="http://www.w3.org/2000/svg" data-diagram="yes"><g/></svg>' }));
vi.mock("mermaid", () => ({ default: { initialize: (config: unknown) => initialize(config), render: draw } }));

import { Mermaid } from "./Mermaid";
import { MAX_DIAGRAM_CHARS, sanitizeSvg, SECURE_KEYS } from "./mermaidSafety";

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

  it("initialises with the hardened config: SVG text labels, limits, and a locked key list", async () => {
    render(<Mermaid code="graph LR; A-->B;" />);
    await waitFor(() => expect(initialize).toHaveBeenCalled());
    const config = initialize.mock.calls[0]![0] as Record<string, unknown>;
    expect(config).toMatchObject({
      startOnLoad: false,
      securityLevel: "strict",
      htmlLabels: false,
      flowchart: { htmlLabels: false },
      maxTextSize: MAX_DIAGRAM_CHARS,
    });
    expect(config.maxEdges).toBeGreaterThan(0);
    expect(config).not.toHaveProperty("themeCSS");
    expect(config).not.toHaveProperty("themeVariables");
  });

  it("locks the keys an init directive could use to weaken the config", async () => {
    render(<Mermaid code={'%%{init: {"securityLevel":"loose","htmlLabels":true}}%%\ngraph LR; A-->B;'} />);
    await waitFor(() => expect(initialize).toHaveBeenCalled());
    const config = initialize.mock.calls[0]![0] as { secure: string[]; securityLevel: string };
    expect(config.securityLevel).toBe("strict");
    for (const key of ["securityLevel", "htmlLabels", "startOnLoad", "maxTextSize", "maxEdges", "theme", "themeCSS", "themeVariables", "flowchart"]) {
      expect(config.secure).toContain(key);
    }
    expect(SECURE_KEYS).toBe(config.secure);
  });

  it("shows the source while a diagram is not drawn", () => {
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    expect(container.querySelector(".md-diagram")).toBeNull();
  });

  it("strips script, foreignObject and handlers from the drawn SVG", async () => {
    draw.mockResolvedValueOnce({
      svg: '<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"><script>alert(1)</script><foreignObject><div>x</div></foreignObject><a href="javascript:alert(1)"><text>t</text></a><text onclick="x()">ok</text></svg>',
    });
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    await waitFor(() => expect(container.querySelector(".md-diagram svg")).not.toBeNull());
    const html = container.querySelector(".md-diagram")!.innerHTML;
    expect(html).not.toMatch(/script|foreignObject|onload|onclick|javascript:/i);
    expect(html).toContain("ok");
  });

  it("keeps the source for an oversized diagram and never calls Mermaid", async () => {
    const { container } = render(<Mermaid code={"graph LR;\n" + "A-->B;".repeat(MAX_DIAGRAM_CHARS)} />);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(draw).not.toHaveBeenCalled();
    expect(container.querySelector(".md-diagram")).toBeNull();
  });

  it("falls back to the source on a render error, without throwing or leaving a render node", async () => {
    draw.mockImplementationOnce(async (id: unknown) => {
      const stray = document.createElement("div");
      stray.id = `d${String(id)}`;
      document.body.appendChild(stray);
      throw new Error("Parse error");
    });
    const { container } = render(<Mermaid code="graph LR; A--" />);
    await waitFor(() => expect(draw).toHaveBeenCalled());
    await waitFor(() => expect(document.querySelector('[id^="dmermaid"]')).toBeNull());
    expect(container.querySelector(".md-diagram")).toBeNull();
    expect(container.textContent).toContain("A--");
  });

  it("falls back to the source when the output is not one SVG document", async () => {
    draw.mockResolvedValueOnce({ svg: "<div>not svg</div>" });
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    await waitFor(() => expect(draw).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container.querySelector(".md-diagram")).toBeNull();
  });
});

describe("sanitizeSvg", () => {
  it("removes frames, embeds, external references and style URLs but keeps same-document ones", () => {
    const out = sanitizeSvg(
      '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><style>.a{fill:red}</style><style>@import url(http://x/y.css)</style><iframe/><object/><embed/><image href="http://x/i.png"/><path marker-end="url(#m)" d="M0 0"/><rect style="fill:url(http://x)"/><use xlink:href="data:text/xml,x"/></svg>',
    );
    expect(out).not.toMatch(/iframe|object|embed|image|http:\/\/x|data:|@import|<use/i);
    expect(out).toContain("fill:red");
    expect(out).toContain("url(#m)");
  });

  it("returns an empty string for malformed or non-SVG input", () => {
    expect(sanitizeSvg("<svg><g></svg>")).toBe("");
    expect(sanitizeSvg("<html/>")).toBe("");
  });
});
