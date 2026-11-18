import mermaid from "mermaid";
import { describe, expect, it } from "vitest";
import { mermaidConfig, sanitizeSvg } from "./mermaidSafety";

// Real Mermaid, not mocked: its own secure-key handling decides whether a directive may change the config.
describe("mermaid secure keys", () => {
  it("ignores an init directive that tries to weaken the hardened config", async () => {
    mermaid.initialize(mermaidConfig(false));
    const before = mermaid.mermaidAPI.getConfig();
    expect(before.securityLevel).toBe("strict");
    await mermaid
      .parse('%%{init: {"securityLevel":"loose","htmlLabels":true,"maxTextSize":999999999,"suppressErrorRendering":false,"themeCSS":"x{}"}}%%\ngraph LR; A-->B;')
      .catch(() => undefined);
    const after = mermaid.mermaidAPI.getConfig();
    expect(after.securityLevel).toBe("strict");
    expect(after.htmlLabels).toBe(false);
    expect(after.maxTextSize).toBe(mermaidConfig(false).maxTextSize);
    expect(after.suppressErrorRendering).toBe(true);
    expect(after.themeCSS).toBeUndefined();
  });

  it("ignores front matter config that tries to weaken the hardened config", async () => {
    mermaid.initialize(mermaidConfig(false));
    const source = '---\nconfig:\n  securityLevel: loose\n  htmlLabels: true\n  maxTextSize: 999999\n  themeCSS: "body{display:none}"\n---\nflowchart TD\n A-->B';
    await mermaid.parse(source).catch(() => undefined);
    const after = mermaid.mermaidAPI.getConfig();
    expect(after.securityLevel).toBe("strict");
    expect(after.htmlLabels).toBe(false);
    expect(after.maxTextSize).toBe(mermaidConfig(false).maxTextSize);
    expect(after.themeCSS).toBeUndefined();
  });
});

// jsdom has no layout; Mermaid only needs sizes to be numbers to produce its normal output.
const proto = SVGElement.prototype as unknown as Record<string, unknown>;
proto.getBBox = () => ({ x: 0, y: 0, width: 20, height: 10 });
proto.getComputedTextLength = () => 20;

describe("real Mermaid output through the sanitiser", () => {
  const diagrams: Record<string, string> = {
    flowchart: "flowchart TD\n A[Start]-->B{Ok}\n B-->|yes|C\n classDef hot fill:#f96\n class C hot",
    sequence: "sequenceDiagram\n A->>B: hi\n B-->>A: yo",
    class: "classDiagram\n class A{+x}\n A <|-- B",
    state: "stateDiagram-v2\n [*] --> S\n S --> [*]",
    gantt: "gantt\n dateFormat YYYY-MM-DD\n section S\n T :a1, 2024-01-01, 3d",
    pie: 'pie\n "a":1\n "b":2',
    er: "erDiagram\n A ||--o{ B : has",
    gitGraph: "gitGraph\n commit\n branch x\n commit",
    journey: "journey\n title t\n section s\n  task: 5: Me",
    quadrant: "quadrantChart\n title t\n x-axis l --> r\n y-axis l --> h\n A: [0.3, 0.6]",
    timeline: "timeline\n title t\n 2020 : a",
  };
  for (const [name, source] of Object.entries(diagrams)) {
    it(`leaves a normal ${name} diagram byte-identical, style block included`, async () => {
      mermaid.initialize(mermaidConfig(false));
      const { svg } = await mermaid.render(`m${name}`, source);
      const original = new DOMParser().parseFromString(svg, "text/html").body.querySelector("svg")!;
      expect(original.querySelector("style")?.textContent).toContain(`#m${name}`);
      const cleaned = sanitizeSvg(svg, `m${name}`)!;
      expect(cleaned.querySelector("style")?.textContent).toBe(original.querySelector("style")?.textContent);
      // Journey draws its section labels in <foreignObject>, which is removed on purpose (its <switch> text stays).
      if (name !== "journey") expect(cleaned.outerHTML).toBe(original.outerHTML);
    });
  }

  it("sanitises a long Unicode classDef name quickly", async () => {
    mermaid.initialize(mermaidConfig(false));
    const name = `${"a".repeat(40)}é`;
    const { svg } = await mermaid.render("mclassdef", `flowchart LR\n A-->B\n classDef ${name} fill:#f00\n class A ${name}`);
    const started = performance.now();
    const cleaned = sanitizeSvg(svg, "mclassdef")!;
    expect(performance.now() - started).toBeLessThan(500);
    expect(cleaned.querySelector("style")?.textContent).toContain(name);
  });
});
