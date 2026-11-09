/**
 * A small reader for the Open XML parts the file viewer shows: a Word body
 * (`word/document.xml`) and a workbook (`xl/workbook.xml` and its sheets). It is
 * hand-written for the same reasons the archive reader is: opening a document
 * must not add a dependency, and a worker has no `DOMParser` to lean on, so the
 * markup is scanned here.
 *
 * It only reads. Nothing here writes a document, and every part comes from the
 * archive reader in `zipArchive.ts`, so no path on a disk is ever involved. A
 * document's own content is never inserted as HTML: the model below is text,
 * and the viewer renders it as React elements.
 */

import { ZipError, readEntryBytes, readZipArchive, type ZipEntry } from "./zipArchive";
import {
  MAX_BLOCKS,
  MAX_CELLS,
  MAX_COLUMNS_PER_SHEET,
  MAX_NESTING,
  MAX_ROWS_PER_SHEET,
  MAX_SHEETS,
  OfficeError,
  type DocxBlock,
  type DocxDocument,
  type DocxTable,
  type OfficeParts,
  type OfficeRequest,
  type OfficeResponse,
  type OfficeSheet,
  type XlsxDocument,
} from "./officeModel";
import { attr, child, children, localName, parseXml, stripControlChars, stripNoise, type XmlElement } from "./officeXml";

export * from "./officeModel";

/** The part a Word document keeps its body in. */
const DOCX_DOCUMENT_PART = "word/document.xml";

/** OPC part names compare without case, and a package may name one either way. */
function findPart(names: readonly string[], name: string): string | null {
  if (names.includes(name)) return name;
  const lower = name.toLowerCase();
  return names.find((candidate) => candidate.toLowerCase() === lower) ?? null;
}

/**
 * An archive's parts, read through the archive reader that already exists: its
 * 50 MB, 2000 entry and 2 MiB entry caps and its checksum checks all still
 * apply, and a refusal keeps its own code and message.
 */
export function openOfficeParts(bytes: Uint8Array<ArrayBuffer>): OfficeParts {
  const read = readZipArchive(bytes);
  if (!read.ok) throw new OfficeError(read.code, read.message);
  const entries = new Map<string, ZipEntry>();
  for (const entry of read.entries) if (!entry.isDirectory) entries.set(entry.path, entry);
  const names = [...entries.keys()];
  return {
    names,
    async read(name: string) {
      const path = findPart(names, name);
      const entry = path === null ? undefined : entries.get(path);
      if (!entry) throw new OfficeError("office_unreadable", `This file has no ${name}.`);
      try {
        return await readEntryBytes(bytes, entry);
      } catch (failure) {
        // The caps and the corruption checks belong to the archive reader. Its
        // message says what it refused; this one says which part that was, so
        // a reader of a document that holds dozens of parts knows where to
        // look. The 2 MiB entry cap is the one that bounds a part.
        if (failure instanceof ZipError) {
          throw new OfficeError(failure.code, `This document's ${name} can't be read. ${failure.message}`);
        }
        throw failure;
      }
    },
  };
}

/**
 * One part's own document element, which is its first element; the scanner's
 * wrapper is not one. The part's size is already bounded by the archive
 * reader's 2 MiB entry cap, which every part comes through, so there is no
 * second cap here to reach.
 */
async function readPartXml(parts: OfficeParts, name: string): Promise<XmlElement> {
  const bytes = await parts.read(name);
  const root = parseXml(new TextDecoder("utf-8").decode(bytes));
  return root.children[0] ?? root;
}

/** A heading style this viewer knows: `Heading1` to `Heading4`, deeper ones clamped. */
const HEADING_LEVELS: Readonly<Record<string, 1 | 2 | 3 | 4>> = {
  "1": 1,
  "2": 2,
  "3": 3,
  "4": 4,
  "5": 4,
  "6": 4,
  "7": 4,
  "8": 4,
  "9": 4,
};

function headingLevel(paragraph: XmlElement): 1 | 2 | 3 | 4 | null {
  const properties = child(paragraph, "pPr");
  const style = properties === null ? null : child(properties, "pStyle");
  const value = style === null ? null : attr(style, "val");
  const match = value === null ? null : /^heading\s*([1-9])$/i.exec(value.trim());
  return match ? (HEADING_LEVELS[match[1] ?? ""] ?? null) : null;
}

/**
 * A paragraph's text, runs joined in order. A `w:tab` becomes a tab and a
 * `w:br` a newline; `w:delText` is a deleted run and is deliberately not
 * matched, so a tracked change that removed text does not bring it back.
 * Insertions, content controls and field markup are flattened as written.
 */
function paragraphText(paragraph: XmlElement): string {
  let text = "";
  for (const node of paragraph.children) text += nodeText(node, 0);
  return text;
}

/** A part nested deeper than any document the viewer reads: refused with its
 * own reason rather than recursing until the stack runs out. */
function tooDeep(): OfficeError {
  return new OfficeError(
    "office_too_deep",
    `This document nests its content more than ${MAX_NESTING} levels deep; the viewer can't read it.`,
  );
}

/**
 * An `mc:Fallback` block repeats what its `mc:Choice` already said, in markup
 * for a reader that knows none of the newer names: Word writes the same
 * `w:txbxContent` in both branches of a text box, so descending into both
 * prints that box's text twice.
 */
function isFallback(node: XmlElement): boolean {
  return localName(node.name) === "Fallback";
}

function nodeText(node: XmlElement, depth: number): string {
  if (depth > MAX_NESTING) throw tooDeep();
  const name = localName(node.name);
  if (name === "t") return stripControlChars(node.text);
  if (name === "tab") return "\t";
  if (name === "br" || name === "cr") return "\n";
  if (isFallback(node)) return "";
  let text = "";
  for (const child of node.children) text += nodeText(child, depth + 1);
  return text;
}

function paragraphBlock(paragraph: XmlElement): DocxBlock {
  const text = paragraphText(paragraph);
  const level = headingLevel(paragraph);
  return level === null ? { kind: "paragraph", text } : { kind: "heading", level, text };
}

/**
 * A table's rows in the order the body gives them, found through the wrapper
 * Word puts around a repeating row (`<w:sdt><w:sdtContent><w:tr>`) and through
 * any other element that is not itself a row, a cell or a table.
 */
function tableRows(table: XmlElement): XmlElement[] {
  const rows: XmlElement[] = [];
  const visit = (node: XmlElement, depth: number) => {
    if (depth > MAX_NESTING) throw tooDeep();
    for (const child of node.children) {
      const name = localName(child.name);
      if (name === "tr") rows.push(child);
      else if (name !== "tbl" && name !== "tc" && !isFallback(child)) visit(child, depth + 1);
    }
  };
  visit(table, 0);
  return rows;
}

function tableBlock(table: XmlElement): DocxTable {
  // `w:gridSpan` and `w:vMerge` are not spread out: a merged cell stays one
  // cell here, so a row's cells are exactly the cells its markup holds.
  const rows = tableRows(table).map((row) => children(row, "tc").map(cellText));
  return { kind: "table", rows, text: rows.map((row) => row.join("\t")).join("\n") };
}

function cellText(cell: XmlElement): string {
  const lines: string[] = [];
  collectCellLines(cell, lines, 0);
  return lines.join("\n");
}

/** A table inside a cell is flattened: one line per row, its cells by tab. */
function collectCellLines(parent: XmlElement, lines: string[], depth: number): void {
  if (depth > MAX_NESTING) throw tooDeep();
  for (const node of parent.children) {
    const name = localName(node.name);
    if (name === "p") lines.push(paragraphText(node));
    else if (name === "tbl") {
      for (const row of tableRows(node)) lines.push(children(row, "tc").map(cellText).join("\t"));
    } else if (!isFallback(node)) collectCellLines(node, lines, depth + 1);
  }
}

/** Body blocks in order. A wrapper such as a content control is descended into. */
function collectBlocks(parent: XmlElement, blocks: DocxBlock[], depth = 0): void {
  if (depth > MAX_NESTING) throw tooDeep();
  for (const node of parent.children) {
    const name = localName(node.name);
    if (name === "p") blocks.push(paragraphBlock(node));
    else if (name === "tbl") blocks.push(tableBlock(node));
    else if (!isFallback(node)) collectBlocks(node, blocks, depth + 1);
    if (blocks.length > MAX_BLOCKS) {
      throw new OfficeError(
        "office_too_many_blocks",
        `This document holds more than ${MAX_BLOCKS} paragraphs and tables; the viewer shows at most ${MAX_BLOCKS}.`,
      );
    }
  }
}

export async function parseDocx(parts: OfficeParts): Promise<DocxDocument> {
  if (findPart(parts.names, DOCX_DOCUMENT_PART) === null) {
    throw new OfficeError(
      "office_unreadable",
      "This file has no word/document.xml, so the viewer can't read it as a Word document.",
    );
  }
  const document = await readPartXml(parts, DOCX_DOCUMENT_PART);
  // Relationships, images, styles and footnotes are not resolved: nothing but
  // this body's own text is read, so a document with pictures shows its words.
  const blocks: DocxBlock[] = [];
  collectBlocks(child(document, "body") ?? document, blocks);
  return { format: "docx", blocks, text: blocks.map((block) => block.text).join("\n") };
}

/** The parts a workbook names its sheets in, and where those targets live. */
const WORKBOOK_PART = "xl/workbook.xml";
const WORKBOOK_RELS_PART = "xl/_rels/workbook.xml.rels";
const SHARED_STRINGS_PART = "xl/sharedStrings.xml";
/** The folder a relationship target is relative to, and the one a missing one falls back to. */
const WORKBOOK_FOLDER = "xl";

/**
 * `AB12` is column 28 and row 12. Anything else is not a position, and no
 * index in the markup is trusted: a caller skips what this refuses.
 */
export function cellPosition(reference: string): { row: number; column: number } | null {
  const match = /^([A-Za-z]{1,3})([0-9]{1,7})$/.exec(reference.trim());
  const letters = match?.[1];
  const digits = match?.[2];
  if (!letters || !digits) return null;
  let column = 0;
  for (const letter of letters.toUpperCase()) column = column * 26 + (letter.charCodeAt(0) - 64);
  const row = Number.parseInt(digits, 10);
  return row >= 1 && column >= 1 ? { row, column } : null;
}

/** The text of one string part: its runs joined, a phonetic run left out. */
function stringText(element: XmlElement, depth = 0): string {
  if (depth > MAX_NESTING) throw tooDeep();
  let text = "";
  for (const node of element.children) {
    const name = localName(node.name);
    if (name === "rPh") continue;
    text += name === "t" ? node.text : stringText(node, depth + 1);
  }
  return text;
}

/** A relationship target as a part name, from `xl/` or from the package root. */
function resolvePartPath(target: string): string {
  const path = target.replace(/\\/g, "/");
  const segments: string[] = path.startsWith("/") ? [] : [WORKBOOK_FOLDER];
  for (const segment of path.split("/")) {
    if (segment === "" || segment === ".") continue;
    if (segment === "..") segments.pop();
    else segments.push(segment);
  }
  return segments.join("/");
}

/** The workbook's own relationships, by id; a target outside the package is not read. */
async function workbookTargets(parts: OfficeParts): Promise<Map<string, string>> {
  const targets = new Map<string, string>();
  if (findPart(parts.names, WORKBOOK_RELS_PART) === null) return targets;
  const rels = await readPartXml(parts, WORKBOOK_RELS_PART);
  for (const rel of children(rels, "Relationship")) {
    const id = attr(rel, "Id");
    const target = attr(rel, "Target");
    if (id === null || target === null) continue;
    if ((attr(rel, "TargetMode") ?? "Internal") !== "Internal") continue;
    targets.set(id, target);
  }
  return targets;
}

async function sharedStrings(parts: OfficeParts): Promise<string[]> {
  if (findPart(parts.names, SHARED_STRINGS_PART) === null) return [];
  return children(await readPartXml(parts, SHARED_STRINGS_PART), "si").map(stringText);
}

/**
 * One cell's display text. Numbers keep the text stored for them, because a
 * number format is a style and styles are not read: a date shows as its serial
 * number. A boolean is TRUE or FALSE, and a formula shows its cached value,
 * which is empty when the workbook was saved without one.
 */
function cellDisplayText(cell: XmlElement, shared: readonly string[]): string {
  const type = attr(cell, "t") ?? "n";
  if (type === "inlineStr") {
    const inline = child(cell, "is");
    return inline === null ? "" : stringText(inline);
  }
  const value = child(cell, "v");
  const raw = value === null ? "" : value.text;
  if (type === "s") {
    const index = Number.parseInt(raw.trim(), 10);
    return Number.isInteger(index) ? (shared[index] ?? "") : "";
  }
  if (type === "b") return raw.trim() === "1" ? "TRUE" : raw.trim() === "0" ? "FALSE" : "";
  if (type === "n") return raw.trim();
  return raw;
}

/**
 * The used range: a value's own reference decides where it sits, so a cell at
 * `D5` stays in column D. A reference that cannot be read is skipped rather
 * than fatal, and a cell with nothing to show does not count towards the caps.
 *
 * A value past a cap is left out and the sheet says so, because refusing the
 * whole sheet would hide the part of it that does fit.
 */
async function sheetGrid(
  parts: OfficeParts,
  part: string,
  shared: readonly string[],
): Promise<{ rows: string[][]; rowsCut: boolean; columnsCut: boolean }> {
  const sheet = await readPartXml(parts, part);
  const data = child(sheet, "sheetData");
  const values: string[][] = [];
  let rowsCut = false;
  let columnsCut = false;
  let rowNumber = 0;
  for (const row of data === null ? [] : children(data, "row")) {
    const declared = Number.parseInt(attr(row, "r") ?? "", 10);
    rowNumber = Number.isInteger(declared) && declared > 0 ? declared : rowNumber + 1;
    let column = 0;
    for (const cell of children(row, "c")) {
      const reference = attr(cell, "r");
      const position = reference === null ? null : cellPosition(reference);
      if (reference !== null && position === null) continue;
      const text = stripNoise(cellDisplayText(cell, shared));
      if (text === "") continue;
      const at = position?.row ?? rowNumber;
      column = position?.column ?? column + 1;
      if (column > MAX_COLUMNS_PER_SHEET) {
        columnsCut = true;
        continue;
      }
      if (at > MAX_ROWS_PER_SHEET) {
        rowsCut = true;
        continue;
      }
      const line = values[at - 1] ?? (values[at - 1] = []);
      line[column - 1] = text;
    }
  }
  // A row keeps only the cells up to its own last value. Padding every row out
  // to the widest column built nothing the grid needed: it gives each row the
  // sheet's width from that one width, and shows a missing cell as nothing.
  const rows: string[][] = [];
  for (let at = 0; at < values.length; at += 1) {
    const line = Array.from(values[at] ?? [], (cell) => cell ?? "");
    while (line.length > 0 && line.at(-1) === "") line.pop();
    rows.push(line);
  }
  return { rows, rowsCut, columnsCut };
}

export async function parseXlsx(parts: OfficeParts): Promise<XlsxDocument> {
  if (findPart(parts.names, WORKBOOK_PART) === null) {
    throw new OfficeError(
      "office_unreadable",
      "This file has no xl/workbook.xml, so the viewer can't read it as a spreadsheet.",
    );
  }
  const workbook = await readPartXml(parts, WORKBOOK_PART);
  const group = child(workbook, "sheets");
  const elements = group === null ? [] : children(group, "sheet");
  if (elements.length === 0) {
    throw new OfficeError("office_unreadable", "This workbook names no sheets, so the viewer can't read it as a spreadsheet.");
  }
  if (elements.length > MAX_SHEETS) {
    throw new OfficeError(
      "office_too_many_sheets",
      `This workbook names ${elements.length} sheets; the viewer shows at most ${MAX_SHEETS}.`,
    );
  }
  const targets = await workbookTargets(parts);
  const shared = await sharedStrings(parts);
  const sheets: OfficeSheet[] = [];
  let shown = 0;
  for (const [at, element] of elements.entries()) {
    const declared = attr(element, "name");
    const name = declared === null || declared.trim() === "" ? `Sheet ${at + 1}` : declared;
    const id = attr(element, "id");
    const target = id === null ? undefined : targets.get(id);
    const part = target === undefined ? `${WORKBOOK_FOLDER}/worksheets/sheet${at + 1}.xml` : resolvePartPath(target);
    // A workbook that names a part it does not carry still opens: its other
    // sheets are readable, and inventing an empty one would misstate the file.
    if (findPart(parts.names, part) === null) continue;
    const grid = await sheetGrid(parts, part, shared);
    const { rows } = grid;
    // A sheet is read one at a time, and its own caps hold each one to 320,000
    // cells, so the workbook is refused once the sheets together pass the
    // budget rather than after every sheet has been laid out.
    const widest = rows.reduce((width, row) => Math.max(width, row.length), 0);
    shown += rows.length * Math.max(1, widest);
    if (shown > MAX_CELLS) {
      throw new OfficeError(
        "office_too_many_cells",
        `This workbook's grids add up to more than ${MAX_CELLS} cells; the viewer shows at most ${MAX_CELLS} across all its sheets.`,
      );
    }
    sheets.push({
      name,
      rows,
      ...(grid.rowsCut ? { rowsCut: true } : {}),
      ...(grid.columnsCut ? { columnsCut: true } : {}),
    });
  }
  if (sheets.length === 0) {
    throw new OfficeError("office_unreadable", "This workbook's worksheets are missing from the file, so the viewer can't read it.");
  }
  return { format: "xlsx", sheets };
}

/**
 * The worker's whole answer, refusal included, kept here so the worker is a
 * three-line wrapper and a test can stand in for a Worker without one.
 */
export async function respondToOfficeRequest(request: OfficeRequest): Promise<OfficeResponse> {
  try {
    if (request.kind !== "docx" && request.kind !== "xlsx") {
      throw new OfficeError("office_unreadable", `The viewer can't read ${String(request.kind)} files.`);
    }
    const parts = openOfficeParts(request.bytes);
    if (request.kind === "docx") return { ok: true, model: await parseDocx(parts) };
    return { ok: true, model: await parseXlsx(parts) };
  } catch (failure) {
    if (failure instanceof OfficeError) return { ok: false, code: failure.code, message: failure.message };
    if (failure instanceof Error && failure.message) return { ok: false, code: "office_unreadable", message: failure.message };
    return { ok: false, code: "office_unreadable", message: "This document can't be read." };
  }
}

