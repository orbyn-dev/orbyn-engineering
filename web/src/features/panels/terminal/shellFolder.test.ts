import { afterEach, describe, expect, it, vi } from "vitest";
import { clearShellFolder, folderInRoot, hearShellFolder, parseOsc7, setShellFolder, shellFolderOf, subscribeShellFolder } from "./shellFolder";

describe("parseOsc7", () => {
  it("reads the path of a file URL, whatever the host", () => {
    expect(parseOsc7("file:///Users/me/work")).toBe("/Users/me/work");
    expect(parseOsc7("file://my-mac.local/Users/me/work")).toBe("/Users/me/work");
    expect(parseOsc7("file://localhost/tmp/x")).toBe("/tmp/x");
    expect(parseOsc7("FILE://host/a")).toBe("/a");
  });
  it("decodes percent escapes", () => {
    expect(parseOsc7("file:///Users/me/my%20docs/caf%C3%A9")).toBe("/Users/me/my docs/café");
    expect(parseOsc7("file:///a%23b%3Fc")).toBe("/a#b?c");
  });
  it("refuses any other scheme or shape", () => {
    for (const bad of ["http://host/a", "https://host/a", "javascript:alert(1)", "ftp://h/a", "/Users/me", "file:/a", "file://host", "file://", "", "file:relative", "x file:///a"]) {
      expect(parseOsc7(bad), bad).toBeNull();
    }
  });
  it("refuses control characters, raw or percent-encoded", () => {
    for (const bad of ["file:///a\u001bb", "file:///a\nb", "file:///a\rb", "file:///a%00b", "file:///a%0Ab", "file:///a%1Bb", "file:///a%7Fb", "file:///a\u0007b", "file:///a\tb"]) {
      expect(parseOsc7(bad), JSON.stringify(bad)).toBeNull();
    }
  });
  it("refuses malformed escapes", () => {
    expect(parseOsc7("file:///a%ZZ")).toBeNull();
    expect(parseOsc7("file:///a%E0%A4%A")).toBeNull();
  });
  it("caps the path at 4096 characters", () => {
    expect(parseOsc7(`file:///${"a".repeat(4095)}`)).toBe(`/${"a".repeat(4095)}`);
    expect(parseOsc7(`file:///${"a".repeat(4096)}`)).toBeNull();
    expect(parseOsc7(`file:///${"%61".repeat(4097)}`)).toBeNull();
  });
});

describe("folderInRoot", () => {
  it("is the folder relative to the root, with a slash, or empty for the root itself", () => {
    expect(folderInRoot("/w/tree", "/w/tree")).toBe("");
    expect(folderInRoot("/w/tree/", "/w/tree/")).toBe("");
    expect(folderInRoot("/w/tree", "/w/tree/src/a")).toBe("src/a/");
    expect(folderInRoot("/w/tree", "/w/tree/src/./a//")).toBe("src/a/");
  });
  it("refuses a folder outside the root, a sibling with the same prefix, or one that climbs out", () => {
    expect(folderInRoot("/w/tree", "/w")).toBeNull();
    expect(folderInRoot("/w/tree", "/w/tree2")).toBeNull();
    expect(folderInRoot("/w/tree", "/etc")).toBeNull();
    expect(folderInRoot("/w/tree", "/w/tree/../other")).toBeNull();
    expect(folderInRoot("/w/tree", "/w/tree/src/../..")).toBeNull();
  });
  it("climbs inside the root", () => {
    expect(folderInRoot("/w/tree", "/w/tree/a/b/..")).toBe("a/");
  });
  it("reads a Windows drive path", () => {
    expect(folderInRoot("C:\\Users\\me\\tree", "/C:/Users/me/tree/src")).toBe("src/");
    expect(folderInRoot("C:\\Users\\me\\tree", "/c:/Users/me/tree")).toBe("");
    expect(folderInRoot("C:\\Users\\me\\tree", "/D:/Users/me/tree")).toBeNull();
  });
  it("refuses when there is no root", () => {
    expect(folderInRoot("", "/a")).toBeNull();
  });
});

describe("the shell's folder, per terminal", () => {
  afterEach(() => {
    clearShellFolder("t1:main");
    clearShellFolder("t1:b");
  });
  it("keeps one folder for each terminal", () => {
    setShellFolder("t1:main", "/w/a");
    setShellFolder("t1:b", "/w/b");
    expect(shellFolderOf("t1:main")).toBe("/w/a");
    expect(shellFolderOf("t1:b")).toBe("/w/b");
    expect(shellFolderOf("t9:main")).toBeNull();
    clearShellFolder("t1:main");
    expect(shellFolderOf("t1:main")).toBeNull();
    expect(shellFolderOf("t1:b")).toBe("/w/b");
  });
  it("tells only that terminal's listeners, and only on a change", () => {
    const main = vi.fn();
    const other = vi.fn();
    const stopMain = subscribeShellFolder("t1:main", main);
    const stopOther = subscribeShellFolder("t1:b", other);
    setShellFolder("t1:main", "/w/a");
    setShellFolder("t1:main", "/w/a");
    expect(main).toHaveBeenCalledTimes(1);
    expect(other).not.toHaveBeenCalled();
    stopMain();
    setShellFolder("t1:main", "/w/c");
    expect(main).toHaveBeenCalledTimes(1);
    stopOther();
  });
});

describe("hearShellFolder", () => {
  it("registers OSC 7 and records what the shell reports, per terminal", () => {
    let handler: ((data: string) => boolean) | null = null;
    const dispose = vi.fn();
    const parser = { registerOscHandler: (ident: number, fn: (data: string) => boolean) => { expect(ident).toBe(7); handler = fn; return { dispose }; } };
    const listener = hearShellFolder(parser, "t1:main");
    expect(handler!("file://h/w/tree/src")).toBe(true);
    expect(shellFolderOf("t1:main")).toBe("/w/tree/src");
    expect(shellFolderOf("t1:b")).toBeNull();
    // Untrusted text that does not parse changes nothing and is still consumed.
    expect(handler!("http://evil/x")).toBe(true);
    expect(handler!("file:///a\u001b[31m")).toBe(true);
    expect(shellFolderOf("t1:main")).toBe("/w/tree/src");
    listener.dispose();
    expect(dispose).toHaveBeenCalled();
    expect(shellFolderOf("t1:main")).toBeNull();
  });
  it("logs nothing", () => {
    const spies = (["log", "warn", "error", "info", "debug"] as const).map((name) => vi.spyOn(console, name).mockImplementation(() => undefined));
    let handler: ((data: string) => boolean) | null = null;
    hearShellFolder({ registerOscHandler: (_i, fn) => { handler = fn; return { dispose() {} }; } }, "t1:main").dispose();
    handler!("file:///secret/path\u0000");
    for (const spy of spies) expect(spy).not.toHaveBeenCalled();
    vi.restoreAllMocks();
  });
});
