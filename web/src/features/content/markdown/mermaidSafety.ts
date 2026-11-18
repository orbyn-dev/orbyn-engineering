/** Hardened Mermaid configuration and output sanitiser. Diagram source is untrusted (agent or repository content). */

/** Largest diagram source, in characters, that is drawn; larger sources keep their text showing. */
export const MAX_DIAGRAM_CHARS = 20_000;
export const MAX_DIAGRAM_EDGES = 300;

/** Keys a `%%{init: ...}%%` directive in the diagram source can never override (this list replaces Mermaid's default). */
export const SECURE_KEYS = [
  "secure",
  "securityLevel",
  "startOnLoad",
  "maxTextSize",
  "maxEdges",
  "htmlLabels",
  "flowchart",
  "theme",
  "themeCSS",
  "themeVariables",
  "fontFamily",
  "dompurifyConfig",
  "arrowMarkerAbsolute",
  "deterministicIds",
  "deterministicIDSeed",
  "legacyMathML",
  "logLevel",
];

export function mermaidConfig(dark: boolean) {
  return {
    startOnLoad: false,
    securityLevel: "strict" as const,
    theme: dark ? ("dark" as const) : ("neutral" as const),
    htmlLabels: false,
    flowchart: { htmlLabels: false },
    maxTextSize: MAX_DIAGRAM_CHARS,
    maxEdges: MAX_DIAGRAM_EDGES,
    secure: SECURE_KEYS,
  };
}

const FORBIDDEN = new Set(["script", "foreignobject", "iframe", "object", "embed", "image", "use", "a", "animate", "set", "handler", "listener"]);
const UNSAFE_CSS = /@import|url\(\s*['"]?\s*(?!#)|javascript:|expression\(|behavior:|-moz-binding/i;
const SAFE_URL = /^\s*#/;

/**
 * Allowlist-style clean-up of Mermaid's SVG before it enters the page. Returns "" when the markup is not one SVG
 * document, so the caller falls back to the source. Removes script, foreignObject, frames, embeds, `on*` handlers,
 * `style` elements and any external or `javascript:` or `data:` reference (only same-document `#id` references stay).
 */
export function sanitizeSvg(markup: string): string {
  const doc = new DOMParser().parseFromString(markup, "image/svg+xml");
  const root = doc.documentElement;
  if (!root || root.localName !== "svg" || doc.getElementsByTagName("parsererror").length > 0) return "";
  const walk = (node: Element) => {
    for (const child of Array.from(node.children)) {
      const tag = child.localName.toLowerCase();
      // Mermaid's own theme lives in a <style> element; keep it unless it reaches outside the document.
      if (FORBIDDEN.has(tag) || (tag === "style" && UNSAFE_CSS.test(child.textContent ?? ""))) child.remove();
      else walk(child);
    }
    for (const attr of Array.from(node.attributes)) {
      const name = attr.name.toLowerCase();
      const value = attr.value;
      const reference = name === "href" || name.endsWith(":href") || name === "src";
      const externalUrl = /url\(\s*['"]?\s*(?!#)/i.test(value) || /javascript:|data:|expression\(/i.test(value);
      if (name.startsWith("on") || (reference && !SAFE_URL.test(value)) || (name === "style" && externalUrl) || (externalUrl && name !== "d")) {
        node.removeAttribute(attr.name);
      }
    }
  };
  walk(root);
  return new XMLSerializer().serializeToString(root);
}
