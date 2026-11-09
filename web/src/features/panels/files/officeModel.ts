/** How deep a part may nest before the reader refuses it: every descent below
 * this is a recursive call, and a stack overflow is not a message to show. */
export const MAX_NESTING = 100;
/** The most paragraphs and tables one Word document may hold. */
export const MAX_BLOCKS = 5000;
/** The most worksheets one workbook may name. */
export const MAX_SHEETS = 200;
/** The most rows and the most columns one sheet's grid shows. */
export const MAX_ROWS_PER_SHEET = 5000;
export const MAX_COLUMNS_PER_SHEET = 64;
/**
 * The most cells every grid of one workbook may show together: each sheet's
 * rows by its widest column, added up, with a sheet that holds no value still
 * counting one column for its rows. One sheet at its own caps shows 320,000,
 * so this is about three such sheets. Bounding each sheet alone was not
 * enough: two hundred sheets of one cell at row 5000 column BL each show
 * 320,000 cells, which is 64,000,000 together, off a 61 KB file.
 */
export const MAX_CELLS = 1000 * 1000;

/** The formats this reader opens. `.docm` and `.xlsm` carry the same parts. */
export type OfficeKind = "docx" | "xlsx";

/** Why a document was refused. Every one of these is shown to the person. */
export type OfficeRefusal =
  | "office_unreadable"
  | "office_too_deep"
  | "office_too_many_blocks"
  | "office_too_many_sheets"
  | "office_too_many_cells";

/** A refusal to read a document, with a message ready to show. */
export class OfficeError extends Error {
  readonly code: OfficeRefusal | string;

  constructor(code: OfficeRefusal | string, message: string) {
    super(message);
    this.name = "OfficeError";
    this.code = code;
  }
}

/** One block of a Word body, with the plain text the reader can show as source. */
export interface DocxHeading {
  kind: "heading";
  /** 1 to 4; a deeper heading style is shown at the deepest level here. */
  level: 1 | 2 | 3 | 4;
  text: string;
}

export interface DocxParagraph {
  kind: "paragraph";
  text: string;
}

export interface DocxTable {
  kind: "table";
  /** One entry per row; a cell's paragraphs are joined with a newline. */
  rows: string[][];
  /** The rows flattened: cells by tab, rows by newline. */
  text: string;
}

export type DocxBlock = DocxHeading | DocxParagraph | DocxTable;

export interface DocxDocument {
  format: "docx";
  blocks: DocxBlock[];
  /** The source view's text: each block's own text on its own line. */
  text: string;
}

/** One worksheet: its tab's label and the used range as display text. */
export interface OfficeSheet {
  name: string;
  /** Rows from row 1, each holding the cells up to its own last value. */
  rows: string[][];
  /** Set when the sheet holds a value past the row cap, which is not shown. */
  rowsCut?: boolean;
  /** Set when the sheet holds a value past the column cap, which is not shown. */
  columnsCut?: boolean;
}

export interface XlsxDocument {
  format: "xlsx";
  sheets: OfficeSheet[];
}

export type OfficeModel = DocxDocument | XlsxDocument;

/** What the viewer hands the worker: the file's bytes and what it should be. */
export interface OfficeRequest {
  kind: OfficeKind;
  bytes: Uint8Array<ArrayBuffer>;
}

/** What the worker hands back: a model, or a refusal with its own code. */
export type OfficeResponse = { ok: true; model: OfficeModel } | { ok: false; code: string; message: string };

/** The XML parts of one archive, read on demand. */
export interface OfficeParts {
  /** Every file entry, as the archive names it with `/` separators. */
  readonly names: readonly string[];
  /** One part's decompressed bytes, with the archive reader's caps and checks. */
  read(name: string): Promise<Uint8Array<ArrayBuffer>>;
}
