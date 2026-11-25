import { describe, expect, it } from "vitest";
import { checkCloneSource, cloneAddress } from "./cloneSource";

describe("a clone source", () => {
  it.each(["https://github.com/orbyn-dev/orbyn-docs.git", "git@github.com:orbyn-dev/orbyn-docs.git", "ssh://git@host/x/y.git"])("accepts %s", (text) => {
    expect(checkCloneSource(` ${text} `)).toEqual({ ok: true, source: text });
  });

  it.each([["", "Enter"], ["--upload-pack=touch /tmp/x", "space"], ["-oProxyCommand=x", "dash"], ["ext::sh -c x", "space"], ["file:///etc", "https or ssh"], ["fd::1", "https or ssh"], ["orbyn-dev/orbyn-docs", "https or ssh"], ["/etc/passwd", "https or ssh"], ["https://user:secret@host/x.git", "password"], ["ssh://u:p@host/x", "password"], ["https://h/x\ny", "control"], ["git@host:-x", "https or ssh"], ["a b/c", "space"], ["../up/dir", "https or ssh"], ["https://h/x\u0000y", "control"]])("refuses %j", (text, why) => {
    const checked = checkCloneSource(text);
    expect(checked.ok).toBe(false);
    if (!checked.ok) expect(checked.reason).toContain(why);
  });
});

it("turns an owner/name into a GitHub address and leaves every other address as typed", () => {
  expect(cloneAddress(" orbyn-dev/orbyn-notes ")).toBe("https://github.com/orbyn-dev/orbyn-notes.git");
  expect(cloneAddress("https://example.com/team/repo.git")).toBe("https://example.com/team/repo.git");
  expect(cloneAddress("git@github.com:team/repo.git")).toBe("git@github.com:team/repo.git");
  expect(cloneAddress("./local/repo")).toBe("./local/repo");
});
