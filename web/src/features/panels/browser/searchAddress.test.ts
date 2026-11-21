import { describe, expect, it } from "vitest";
import { resolveAddress } from "./browserModel";
import { customSearchUrl, checkSearchAddress, SEARCH_PREVIEW_WORDS } from "./searchAddress";

describe("checkSearchAddress", () => {
  it("accepts an https or http address with one %s", () => {
    expect(checkSearchAddress("https://find.example/?q=%s")).toEqual({ ok: true });
    expect(checkSearchAddress("http://find.example/s/%s/")).toEqual({ ok: true });
  });
  it("says nothing is wrong with an empty field", () => {
    expect(checkSearchAddress("")).toEqual({ ok: false, reason: "empty", message: null });
    expect(checkSearchAddress("   ")).toEqual({ ok: false, reason: "empty", message: null });
  });
  it("refuses a bare host so it is not mistaken for words", () => {
    expect(checkSearchAddress("find.example/?q=%s")).toEqual({ ok: false, reason: "scheme", message: "Start the address with https:// or http://." });
    expect(checkSearchAddress("ftp://find.example/?q=%s")).toMatchObject({ reason: "scheme" });
    expect(checkSearchAddress("javascript:alert(%s)")).toMatchObject({ reason: "scheme" });
  });
  it("asks for %s once", () => {
    expect(checkSearchAddress("https://find.example/?q=")).toEqual({ ok: false, reason: "token", message: "Add %s where the search words go." });
    expect(checkSearchAddress("https://find.example/?q=%s&r=%s")).toEqual({ ok: false, reason: "token", message: "Use %s once, where the search words go." });
  });
  it("refuses an address that is not a URL", () => {
    expect(checkSearchAddress("https://%s")).toMatchObject({ ok: true });
    expect(checkSearchAddress("https:// %s")).toMatchObject({ reason: "scheme" });
  });
});

describe("customSearchUrl", () => {
  it("puts the encoded words where %s is, as the built-in engines encode them", () => {
    expect(customSearchUrl("https://find.example/?q=%s", SEARCH_PREVIEW_WORDS)).toBe("https://find.example/?q=orbyn%20designs");
    expect(customSearchUrl("https://find.example/?q=%s", "a&b=c#d")).toBe("https://find.example/?q=a%26b%3Dc%23d");
  });
  it("is null for an invalid address", () => {
    expect(customSearchUrl("https://find.example/", "x")).toBeNull();
  });
  it("never reads a $ pattern in the words as a replacement pattern", () => {
    expect(customSearchUrl("https://find.example/?q=%s", "$&$1")).toBe("https://find.example/?q=%24%26%241");
  });
});

describe("resolveAddress with a custom engine", () => {
  const address = "https://find.example/?q=%s";
  it("sends only words to the custom engine", () => {
    expect(resolveAddress("orbyn designs", "custom", address)).toBe("https://find.example/?q=orbyn%20designs");
    expect(resolveAddress("example.com/x", "custom", address)).toBe("https://example.com/x");
    expect(resolveAddress("localhost:5173", "custom", address)).toBe("http://localhost:5173");
    expect(resolveAddress("https://a.test/", "custom", address)).toBe("https://a.test/");
  });
  it("falls back to DuckDuckGo when the saved address is empty or invalid", () => {
    expect(resolveAddress("hello", "custom", "")).toBe("https://duckduckgo.com/?q=hello");
    expect(resolveAddress("hello", "custom", "find.example/?q=%s")).toBe("https://duckduckgo.com/?q=hello");
    expect(resolveAddress("hello", "custom", undefined)).toBe("https://duckduckgo.com/?q=hello");
  });
  it("leaves the built-in engines unchanged", () => {
    expect(resolveAddress("hello world", "google", address)).toBe("https://www.google.com/search?q=hello%20world");
  });
});
