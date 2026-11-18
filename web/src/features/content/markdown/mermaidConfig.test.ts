import mermaid from "mermaid";
import { describe, expect, it } from "vitest";
import { mermaidConfig } from "./mermaidSafety";

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
});
