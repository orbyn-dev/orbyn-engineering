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
  "suppressErrorRendering",
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
    // The root setting wins over the deprecated per-diagram ones in this Mermaid version.
    htmlLabels: false,
    suppressErrorRendering: true,
    maxTextSize: MAX_DIAGRAM_CHARS,
    maxEdges: MAX_DIAGRAM_EDGES,
    secure: SECURE_KEYS,
  };
}

const SVG_NS = "http://www.w3.org/2000/svg";
/** Elements removed with their content. This is a denylist (plus the namespace check), not an allowlist. */
const FORBIDDEN = new Set([
  "script", "foreignobject", "iframe", "object", "embed", "image", "audio", "video", "canvas",
  "animate", "animatetransform", "animatemotion", "set", "handler", "listener",
]);

/** Lower-cases, strips comments and decodes CSS escapes so `\69mport` and `u\72l(` read as written. */
function normaliseCss(value: string): string {
  return value
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\\([0-9a-fA-F]{1,6})[ \t\r\n\f]?/g, (_, hex: string) => {
      const code = parseInt(hex, 16);
      return code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : "";
    })
    .replace(/\\([\s\S])/g, "$1")
    .toLowerCase();
}

/** True when CSS (a style element or any attribute value) could load or run something outside this document. */
function unsafeCss(raw: string): boolean {
  const css = normaliseCss(raw);
  if (/@import|image-set\(|src\(|expression\(|javascript:|vbscript:|behavior:|-moz-binding|cross-fade\(|element\(/.test(css)) return true;
  for (const match of css.matchAll(/url\(\s*(['"]?)\s*(.?)/g)) if (match[2] !== "#") return true;
  return false;
}

function cleanAttributes(el: Element): void {
  for (const attr of Array.from(el.attributes)) {
    const name = attr.name.toLowerCase();
    const reference = name === "href" || name.endsWith(":href") || name === "src";
    const bad =
      name.startsWith("on") ||
      (reference && !/^\s*#/.test(attr.value)) ||
      (name !== "d" && unsafeCss(attr.value)) ||
      /^\s*(javascript|data|vbscript):/i.test(attr.value);
    if (bad) el.removeAttribute(attr.name);
  }
}

/** Removes everything unsafe from `parent`'s subtree, in place. */
function cleanChildren(parent: Element): void {
  let child = parent.firstChild;
  while (child) {
    let next = child.nextSibling;
    if (child.nodeType === 1) {
      const el = child as Element;
      const tag = el.localName.toLowerCase();
      if (el.namespaceURI !== SVG_NS || FORBIDDEN.has(tag) || (tag === "style" && unsafeCss(el.textContent ?? ""))) {
        el.remove();
      } else if (tag === "a") {
        // Mermaid wraps a clickable node's shape and label in <a>: keep them, lose the link.
        const kids = Array.from(el.childNodes);
        el.replaceWith(...kids);
        next = kids[0] ?? next;
      } else {
        cleanAttributes(el);
        cleanChildren(el);
      }
    } else if (child.nodeType !== 3) {
      child.remove(); // comments, processing instructions, CDATA
    }
    child = next;
  }
}

/**
 * Cleans Mermaid's SVG before it enters the page. It parses as HTML (what an innerHTML insert would do, so `&nbsp;`
 * and friends are accepted), keeps only the SVG-namespace root, removes denylisted elements, non-element nodes,
 * `on*` handlers, `javascript:`/`data:` values and any reference or CSS URL that is not a same-document `#id`, and
 * unwraps `<a>` links. Returns the cleaned element itself (insert it directly; do not re-serialise), or null when the
 * markup holds no SVG root.
 */
export function sanitizeSvg(markup: string): SVGElement | null {
  const doc = new DOMParser().parseFromString(markup, "text/html");
  const root = doc.body.querySelector("svg");
  if (!root || root.namespaceURI !== SVG_NS) return null;
  cleanAttributes(root);
  cleanChildren(root);
  return root as SVGElement;
}
