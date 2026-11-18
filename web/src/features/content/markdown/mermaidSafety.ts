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
  "animate", "animatecolor", "animatetransform", "animatemotion", "set", "handler", "listener",
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
  if (/@import|image-set\(|image\(|src\(|expression\(|javascript:|vbscript:|behavior:|-moz-binding|cross-fade\(|element\(/.test(css)) return true;
  for (const match of css.matchAll(/url\(\s*(['"]?)\s*(.?)/g)) if (match[2] !== "#") return true;
  return false;
}

/** Splits CSS into its top-level statements (original text), or null when it cannot be parsed with confidence. */
function topLevelRules(css: string): string[] | null {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules: string[] = [];
  let start = 0;
  let depth = 0;
  let quote = "";
  for (let i = 0; i < text.length; i++) {
    const ch = text[i]!;
    if (quote) {
      if (ch === "\\") i++;
      else if (ch === quote) quote = "";
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (ch === "{") depth++;
    else if (ch === "}") {
      if (--depth < 0) return null;
      if (depth === 0) {
        rules.push(text.slice(start, i + 1));
        start = i + 1;
      }
    } else if (ch === ";" && depth === 0) return null; // a block-less at-rule (@import, @charset, @namespace)
  }
  if (quote || depth !== 0 || text.slice(start).trim() !== "") return null;
  return rules;
}

/**
 * Keeps only the rules of a Mermaid style block that cannot reach outside the diagram: selectors that all start with
 * the root svg's own `#id` (how Mermaid scopes its rules), and its `@keyframes`. Returns the original text when
 * nothing is dropped, and null when the block cannot be parsed (drop it whole).
 */
function scopeCss(css: string, rootId: string): string | null {
  const rules = topLevelRules(css);
  if (!rules) return null;
  const own = new RegExp(`^#${rootId.replace(/[^\w-]/g, "\\$&")}(?![\\w-])`);
  const kept = rules.filter((rule) => {
    const open = rule.indexOf("{");
    const prelude = rule.slice(0, open).trim();
    if (/^@keyframes\s+[\w-]+$/i.test(prelude)) {
      // Keyframe steps are only `from`, `to` or percentages.
      const inner = topLevelRules(rule.slice(open + 1, -1));
      return inner !== null && inner.every((step) => /^\s*(from|to|[\d.]+%)(\s*,\s*(from|to|[\d.]+%))*\s*\{[^{}]*\}$/i.test(step));
    }
    return rootId !== "" && !prelude.startsWith("@") && prelude.split(",").every((selector) => own.test(selector.trim()));
  });
  return kept.length === rules.length ? css : kept.join("");
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
function cleanChildren(parent: Element, rootId: string): void {
  let child = parent.firstChild;
  while (child) {
    let next = child.nextSibling;
    if (child.nodeType === 1) {
      const el = child as Element;
      const tag = el.localName.toLowerCase();
      if (el.namespaceURI !== SVG_NS || FORBIDDEN.has(tag) || (tag === "style" && unsafeCss(el.textContent ?? ""))) {
        el.remove();
      } else if (tag === "style") {
        const scoped = scopeCss(el.textContent ?? "", rootId);
        if (!scoped?.trim()) el.remove();
        else {
          if (scoped !== el.textContent) el.textContent = scoped;
          cleanAttributes(el);
        }
      } else if (tag === "a") {
        // Mermaid wraps a clickable node's shape and label in <a>: keep them, lose the link.
        const kids = Array.from(el.childNodes);
        el.replaceWith(...kids);
        next = kids[0] ?? next;
      } else {
        cleanAttributes(el);
        cleanChildren(el, rootId);
      }
    } else if (child.nodeType !== 3) {
      child.remove(); // comments, processing instructions, CDATA
    }
    child = next;
  }
}

/**
 * Cleans Mermaid's SVG before it enters the page. It parses as HTML (what an innerHTML insert would do, so `&nbsp;`
 * and friends are accepted), keeps only the SVG-namespace root, removes denylisted elements, non-element nodes, style rules not scoped to the root's `#id`,
 * `on*` handlers, `javascript:`/`data:` values and any reference or CSS URL that is not a same-document `#id`, and
 * unwraps `<a>` links. Returns the cleaned element itself (insert it directly; do not re-serialise), or null when the
 * markup holds no SVG root.
 */
export function sanitizeSvg(markup: string): SVGElement | null {
  const doc = new DOMParser().parseFromString(markup, "text/html");
  const root = doc.body.querySelector("svg");
  if (!root || root.namespaceURI !== SVG_NS) return null;
  cleanAttributes(root);
  cleanChildren(root, root.getAttribute("id") ?? "");
  return root as SVGElement;
}
