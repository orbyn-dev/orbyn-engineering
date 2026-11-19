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

/**
 * True when CSS (a style element or any attribute value) could load or run something outside this document.
 * A backslash or comment marker anywhere is refused outright: browsers and a regex read escapes, comments and
 * strings differently, and Mermaid's own output has none of them.
 */
function unsafeCss(raw: string): boolean {
  if (/\\|\/\*/.test(raw)) return true;
  const css = raw.toLowerCase();
  if (/@import|image-set\(|image\(|src\(|expression\(|javascript:|vbscript:|behavior:|-moz-binding|cross-fade\(|element\(/.test(css)) return true;
  // Only a complete `url(#id)` (quoted or not) passes. A quote or `(` inside an unquoted url( makes a bad-url token
  // that swallows text up to the next `)`, which would hide the rules after it from any reader that does not do the same.
  for (const match of css.matchAll(/url\(/g)) {
    const token = /url\(\s*(?:"#[\w-]*"|'#[\w-]*'|#[\w-]*)\s*\)/y;
    token.lastIndex = match.index;
    if (!token.test(css)) return true;
  }
  return false;
}

interface CssRule {
  prelude: string;
  body: string;
  /** True when the declaration block holds another `{` outside a string. */
  nested: boolean;
}

/**
 * The one CSS tokenizer. Splits `css` into top-level `prelude{body}` rules, tracking strings in preludes and bodies
 * alike, so every later check works on this split and never re-derives quote state. Returns null when it cannot be
 * parsed with confidence. Callers run `unsafeCss` first, which refuses backslashes, comment markers and malformed
 * `url(`; a newline is refused here because a CSS string ends at one.
 */
function parseRules(css: string): CssRule[] | null {
  if (/[\n\r\f]/.test(css)) return null;
  const rules: CssRule[] = [];
  let prelude = "";
  let body = "";
  let nested = false;
  let depth = 0;
  let quote = "";
  for (const ch of css) {
    const inBody = depth > 0;
    if (quote) {
      if (ch === quote) quote = "";
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (ch === "{") {
      depth++;
      if (depth === 1) continue; // the block opener itself
      nested = true;
    } else if (ch === "}") {
      if (--depth < 0) return null;
      if (depth === 0) {
        rules.push({ prelude, body, nested });
        prelude = body = "";
        nested = false;
        continue;
      }
    } else if (ch === ";" && depth === 0) return null; // a block-less at-rule (@import, @charset, @namespace)
    if (inBody || depth > 0) body += ch;
    else prelude += ch;
  }
  if (quote || depth !== 0 || prelude.trim() !== "") return null;
  return rules;
}

/** A name character: ASCII word characters, `-` and anything non-ASCII (CSS reads those, no-break space included, as part of a name). */
const NAME = "[\\w\\u0080-\\uffff-]";
/**
 * What may follow the root `#id` in a selector, as one token at a time: names, classes, ids, `*`, `>` and descendant
 * spaces (space and tab only), plain pseudo-classes and `[attr]`/`[attr=value]`-style tests. Each token is a separate
 * sticky pattern with no star around an alternation, so a failed match cannot backtrack super-linearly.
 */
const SELECTOR_TOKENS = [
  /[ \t]*>[ \t]*/y,
  /[ \t]+/y,
  new RegExp(`\\.${NAME}+`, "y"),
  new RegExp(`#${NAME}+`, "y"),
  new RegExp(`:[a-z-]+(?!${NAME}|\\()`, "iy"),
  /\*/y,
  new RegExp(`${NAME}+`, "y"),
  new RegExp(`\\[${NAME}+(?:[$^*]?=(?:"${NAME}*"|'${NAME}*'|${NAME}*))?\\]`, "y"),
];
const NAME_CHAR = new RegExp(NAME);

function selectorTailOk(tail: string): boolean {
  let pos = 0;
  while (pos < tail.length) {
    let next = -1;
    for (const token of SELECTOR_TOKENS) {
      token.lastIndex = pos;
      if (token.test(tail)) {
        next = token.lastIndex;
        break;
      }
    }
    if (next <= pos) return false;
    pos = next;
  }
  return true;
}
const KEYFRAME_STEP = /^\s*(?:from|to|[\d.]+%)(?:\s*,\s*(?:from|to|[\d.]+%))*\s*$/i;

/**
 * Keeps only the rules of a Mermaid style block that cannot reach outside the diagram, and rebuilds the text from
 * them: rules whose selectors all are `#<rootId>` followed by the strict grammar above (how Mermaid scopes its
 * rules), with no nested block, and its `@keyframes`. Returns null when the block cannot be parsed (drop it whole).
 */
function scopeCss(css: string, rootId: string): string | null {
  const rules = parseRules(css);
  if (!rules) return null;
  const kept = rules.filter(({ prelude, body, nested }) => {
    const head = prelude.trim();
    const frames = /^@keyframes\s+([\w-]+)$/i.exec(head);
    if (frames) {
      // Keyframe names are page-global: only Mermaid's own, or ones prefixed with this diagram's id.
      const name = frames[1]!;
      if (name !== "dash" && name !== "edge-animation-frame" && !(rootId !== "" && name.startsWith(`${rootId}-`))) return false;
      const steps = parseRules(body);
      return steps !== null && steps.every((step) => !step.nested && KEYFRAME_STEP.test(step.prelude));
    }
    if (rootId === "" || nested || head.startsWith("@")) return false;
    return head.split(",").every((raw) => {
      const selector = raw.trim();
      return (
        selector.startsWith(`#${rootId}`) &&
        !NAME_CHAR.test(selector.charAt(rootId.length + 1)) &&
        selectorTailOk(selector.slice(rootId.length + 1))
      );
    });
  });
  return kept.map(({ prelude, body }) => `${prelude}{${body}}`).join("");
}

function cleanAttributes(el: Element): void {
  // Walk a snapshot of the names (getAttributeNames), so removing as we go cannot skip an attribute. This reads more plainly
  // than Array.from(el.attributes), which was already a snapshot; the checks and what is removed are unchanged.
  for (const raw of el.getAttributeNames()) {
    const value = el.getAttribute(raw) ?? "";
    const name = raw.toLowerCase();
    const reference = name === "href" || name.endsWith(":href") || name === "src";
    const bad =
      name.startsWith("on") ||
      (reference && !/^\s*#/.test(value)) ||
      (name !== "d" && unsafeCss(value)) ||
      /^\s*(javascript|data|vbscript):/i.test(value);
    if (bad) el.removeAttribute(raw);
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
        // CSS text only: an element inside <style> is foreign-content markup the rule check never sees.
        const scoped = el.children.length === 0 ? scopeCss(el.textContent ?? "", rootId) : null;
        if (!scoped?.trim()) el.remove();
        else {
          el.textContent = scoped; // also drops comment nodes
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
export function sanitizeSvg(markup: string, expectedId: string): SVGElement | null {
  const doc = new DOMParser().parseFromString(markup, "text/html");
  const root = doc.body.querySelector("svg");
  if (!root || root.namespaceURI !== SVG_NS) return null;
  // The scope of every kept style rule is the root's id, so it must be the one the caller gave Mermaid, not whatever
  // the markup claims (an id such as the app's own would let a rule style the page).
  if (root.getAttribute("id") !== expectedId) return null;
  cleanAttributes(root);
  cleanChildren(root, expectedId);
  return root as SVGElement;
}
