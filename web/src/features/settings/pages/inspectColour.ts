// The element inspector of the theme editor (parity K15): which theme colour a part of the app uses. It reads the computed
// colours of the app's own page and compares them with the tokens' values. Nothing here changes a colour; the editor's list does.

export type TokenList = readonly (readonly [token: string, label: string])[];
export type Part = "background" | "text" | "border";

export interface Paint {
  background: string;
  text: string;
  border: string;
  /** 0 when the element draws no border. */
  borderWidth: number;
}

/** The page the inspector reads, so a test can give it one that resolves custom properties. */
export interface InspectEnv {
  tokens: TokenList;
  /** A token's current value as the page resolves it. */
  tokenValue(token: string): string;
  paint(element: Element): Paint;
  /** One spelling for the same colour: `#fff` and `rgb(255, 255, 255)` are equal. */
  normalise(value: string): string;
  all(): Element[];
}

export type Inspected =
  | { kind: "token"; token: string; label: string; part: Part; name: string; sharedWith: number }
  | { kind: "content"; note: string }
  | { kind: "none" };

export const CONTENT_NOTE = "This colour belongs to the content, not the theme. A project’s colour is set on the project.";
/** Colours that belong to what is shown, not to the theme. */
const CONTENT = "[data-content-colour], .xterm";
const MAX_ELEMENTS = 5000;

const isClear = (value: string) => value === "" || value === "transparent" || /^rgba\(0,0,0,0\)$/.test(value) || /^rgba\(\d+,\d+,\d+,0\)$/.test(value);

const ownText = (element: Element) => [...element.childNodes].some((node) => node.nodeType === 3 && (node.textContent ?? "").trim() !== "");

/** What a person would call the element, taken as text only: a name from the page is never read as markup. */
export function nameOf(element: Element): string {
  const label = element.getAttribute("aria-label") ?? element.getAttribute("alt") ?? element.getAttribute("title");
  if (label && label.trim()) return label.trim().slice(0, 60);
  const role = element.getAttribute("role");
  return (role ?? element.tagName.toLowerCase()).slice(0, 60);
}

export function inspectElement(start: Element, env: InspectEnv): Inspected {
  if (start.closest(CONTENT)) return { kind: "content", note: CONTENT_NOTE };
  const byValue = new Map<string, [string, string]>();
  for (const [token, label] of env.tokens) {
    const value = env.normalise(env.tokenValue(token));
    if (value !== "" && !byValue.has(value)) byValue.set(value, [token, label]);
  }
  const match = (value: string) => (isClear(value) ? undefined : byValue.get(value));
  const own = env.paint(start);
  const text = match(env.normalise(own.text));
  const border = own.borderWidth > 0 ? match(env.normalise(own.border)) : undefined;
  let found: { token: string; label: string; part: Part } | null = null;
  if (ownText(start) && text) found = { token: text[0], label: text[1], part: "text" };
  // The background that is seen is the nearest painted one, up through transparent parents.
  for (let element: Element | null = start; !found && element; element = element.parentElement) {
    const paint = env.paint(element);
    const value = env.normalise(paint.background);
    if (isClear(value)) continue;
    const hit = byValue.get(value);
    if (hit) found = { token: hit[0], label: hit[1], part: "background" };
    break;
  }
  if (!found && border) found = { token: border[0], label: border[1], part: "border" };
  if (!found && text) found = { token: text[0], label: text[1], part: "text" };
  if (!found) return { kind: "none" };
  const colour = byValue.has(env.normalise(env.tokenValue(found.token))) ? env.normalise(env.tokenValue(found.token)) : "";
  const key = (element: Element) => `${element.tagName}.${element.classList[0] ?? ""}`;
  const same = new Set<string>();
  for (const element of env.all().slice(0, MAX_ELEMENTS)) {
    if (element === start || element.closest("[data-inspector-ui]")) continue;
    const paint = env.paint(element);
    const value = env.normalise(found.part === "text" ? paint.text : found.part === "border" ? paint.border : paint.background);
    if (value === colour) same.add(key(element));
  }
  same.delete(key(start));
  return { kind: "token", ...found, name: nameOf(start), sharedWith: same.size };
}

/** The page's own resolver: custom properties from the root and computed colours. */
export function pageEnv(tokens: TokenList): InspectEnv {
  const probe = document.createElement("span");
  const normalise = (value: string) => {
    if (value.trim() === "") return "";
    probe.style.color = "";
    probe.style.color = value;
    if (probe.style.color === "") return value.replace(/\s+/g, "");
    document.body.appendChild(probe);
    const out = getComputedStyle(probe).color;
    probe.remove();
    return out.replace(/\s+/g, "");
  };
  return {
    tokens,
    tokenValue: (token) => getComputedStyle(document.documentElement).getPropertyValue(token).trim(),
    paint: (element) => {
      const style = getComputedStyle(element);
      return {
        background: style.backgroundColor.replace(/\s+/g, ""),
        text: style.color.replace(/\s+/g, ""),
        border: style.borderTopColor.replace(/\s+/g, ""),
        borderWidth: parseFloat(style.borderTopWidth) || 0,
      };
    },
    normalise,
    all: () => [...document.querySelectorAll("body *")],
  };
}
