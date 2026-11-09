/** The controls that are genuinely noise, in one test: below a tab, between a
 * newline and a space, and the two control blocks. A tab and a newline are not
 * among them, because a cell's own text can hold either. */
function isNoise(code: number): boolean {
  return (code < 0x20 && code !== 0x09 && code !== 0x0a) || (code >= 0x7f && code <= 0x9f);
}

/** Every control character and line break: a name, an attribute, or a run of
 * text, where a break can only have come from markup this reader already
 * turns into one (`w:tab`, `w:br`). */
export function stripControlChars(text: string): string {
  let clean = "";
  for (const character of text) {
    const code = character.codePointAt(0) ?? 0;
    if (code < 0x20 || (code >= 0x7f && code <= 0x9f)) continue;
    clean += character;
  }
  return clean;
}

/**
 * The noise taken out of a string whose own tabs and line breaks are content:
 * a workbook cell, where Alt+Enter is a real `&#10;` in the file. The grid
 * shows one so the reader can see it, and the markup the scanner drops is
 * never a line break, so nothing here invents one.
 */
export function stripNoise(text: string): string {
  let clean = "";
  for (const character of text) {
    if (!isNoise(character.codePointAt(0) ?? 0)) clean += character;
  }
  return clean;
}

/** One element of a scanned part. Only what the two formats need is kept. */
export interface XmlElement {
  name: string;
  attrs: Map<string, string>;
  children: XmlElement[];
  /** The character data directly inside this element, entities decoded. */
  text: string;
}

const NAMED_ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
const TAG_NAME = /^[^\s/>]+/;
const ATTRIBUTES = /([^\s=/>]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;

function decodeEntities(text: string): string {
  if (!text.includes("&")) return text;
  return text.replace(/&(#[xX]?[0-9a-fA-F]+|[a-zA-Z]+);/g, (whole, body: string) => {
    if (!body.startsWith("#")) return NAMED_ENTITIES[body] ?? whole;
    const hex = body[1] === "x" || body[1] === "X";
    const code = Number.parseInt(hex ? body.slice(2) : body.slice(1), hex ? 16 : 10);
    // A lone surrogate cannot be built and has no meaning here.
    if (!Number.isInteger(code) || code <= 0 || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return whole;
    return String.fromCodePoint(code);
  });
}

/** The end of a tag: a `>` inside a quoted value does not end it. */
function findTagEnd(source: string, from: number): number {
  let quote = "";
  for (let at = from; at < source.length; at += 1) {
    const character = source[at] ?? "";
    if (quote) {
      if (character === quote) quote = "";
      continue;
    }
    if (character === '"' || character === "'") {
      quote = character;
      continue;
    }
    if (character === ">") return at;
  }
  return -1;
}

function readAttributes(text: string): Map<string, string> {
  const attrs = new Map<string, string>();
  for (const match of text.matchAll(ATTRIBUTES)) {
    const name = match[1];
    if (!name) continue;
    const value = match[2] ?? match[3] ?? "";
    attrs.set(name, stripControlChars(decodeEntities(value)));
  }
  return attrs;
}

/**
 * A part as a tree. The scanner is deliberately forgiving: a stray close tag
 * and a tag cut short by the end of the bytes are ignored, because a document
 * that is damaged in one place should still show what it does hold. Comments,
 * the declaration and a doctype are skipped, and CDATA is taken as text.
 */
export function parseXml(source: string): XmlElement {
  const root: XmlElement = { name: "#root", attrs: new Map(), children: [], text: "" };
  const open: XmlElement[] = [root];
  let at = 0;
  while (at < source.length) {
    const start = source.indexOf("<", at);
    const top = open[open.length - 1];
    if (!top) break;
    if (start < 0) {
      top.text += stripNoise(decodeEntities(source.slice(at)));
      break;
    }
    top.text += stripNoise(decodeEntities(source.slice(at, start)));
    if (source.startsWith("<!--", start)) {
      const end = source.indexOf("-->", start + 4);
      at = end < 0 ? source.length : end + 3;
      continue;
    }
    if (source.startsWith("<![CDATA[", start)) {
      const end = source.indexOf("]]>", start + 9);
      const stop = end < 0 ? source.length : end;
      top.text += stripNoise(source.slice(start + 9, stop));
      at = end < 0 ? source.length : end + 3;
      continue;
    }
    if (source.startsWith("<?", start)) {
      const end = source.indexOf("?>", start + 2);
      at = end < 0 ? source.length : end + 2;
      continue;
    }
    if (source.startsWith("<!", start)) {
      // A doctype; this reader never needs its declarations.
      const end = findTagEnd(source, start + 2);
      at = end < 0 ? source.length : end + 1;
      continue;
    }
    const end = findTagEnd(source, start + 1);
    if (end < 0) break;
    const body = source.slice(start + 1, end);
    at = end + 1;
    if (body.startsWith("/")) {
      const name = body.slice(1).trim();
      while (open.length > 1 && open[open.length - 1]?.name !== name) open.pop();
      if (open.length > 1) open.pop();
      continue;
    }
    const selfClosing = body.endsWith("/");
    const tag = selfClosing ? body.slice(0, -1) : body;
    const name = TAG_NAME.exec(tag)?.[0] ?? "";
    if (name === "") continue;
    const element: XmlElement = { name, attrs: readAttributes(tag.slice(name.length)), children: [], text: "" };
    top.children.push(element);
    if (!selfClosing) open.push(element);
  }
  return root;
}

/** The part after a namespace prefix: `w:p` is `p`, and `r:id` is `id`. */
export function localName(name: string): string {
  const at = name.indexOf(":");
  return at < 0 ? name : name.slice(at + 1);
}

/**
 * A match by local name, so a prefixed `w:p` and an unprefixed `p` both count:
 * a Word body prefixes its elements and a sheet's XML usually does not.
 */
export function child(element: XmlElement, name: string): XmlElement | null {
  return element.children.find((node) => localName(node.name) === name) ?? null;
}

export function children(element: XmlElement, name: string): XmlElement[] {
  return element.children.filter((node) => localName(node.name) === name);
}

/** An attribute by local name, so `w:val` and `val` both answer. */
export function attr(element: XmlElement, name: string): string | null {
  const exact = element.attrs.get(name);
  if (exact !== undefined) return exact;
  for (const [key, value] of element.attrs) if (localName(key) === name) return value;
  return null;
}

