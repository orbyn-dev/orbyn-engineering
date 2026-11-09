import { describe, expect, it } from "vitest";
import { hexRows } from "./FileMedia";

describe("hexRows", () => {
  it("lays out offset, bytes and printable text", () => {
    const rows = hexRows(new Uint8Array([0x41, 0x00, 0xff]), 16);
    expect(rows).toEqual([{ offset: "00000010", hex: "41 00 ff", text: "A.." }]);
  });
  it("breaks every sixteen bytes", () => {
    expect(hexRows(new Uint8Array(17)).map((row) => row.offset)).toEqual(["00000000", "00000010"]);
  });
});
