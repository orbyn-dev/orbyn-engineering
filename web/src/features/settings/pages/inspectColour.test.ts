import { describe, expect, it } from "vitest";
import { inspectElement, type InspectEnv } from "./inspectColour";

const TOKENS = [
  ["--color-background", "Surface"],
  ["--color-sunken", "Raised"],
  ["--color-muted", "Text 3"],
] as const;
const VALUES: Record<string, string> = { "--color-background": "rgb(255, 255, 255)", "--color-sunken": "rgba(0, 0, 0, 0.035)", "--color-muted": "rgba(0, 0, 0, 0.6)" };

/** A test page with styles given inline, since jsdom resolves no custom properties. */
function env(): InspectEnv {
  return {
    tokens: TOKENS,
    tokenValue: (token) => VALUES[token] ?? "",
    paint: (element) => {
      const style = (element as HTMLElement).style;
      return { background: style.backgroundColor, text: style.color, border: style.borderColor, borderWidth: style.borderWidth === "" ? 0 : 1 };
    },
    normalise: (value) => value.replace(/\s+/g, "").replace(/^transparent$/, "rgba(0,0,0,0)"),
    all: () => [...document.querySelectorAll("*")],
  };
}
const page = (html: string) => {
  document.body.innerHTML = html;
  return document.body;
};
const at = (selector: string) => document.querySelector(selector)!;

describe("inspectElement", () => {
  it("names the token a painted background uses", () => {
    page('<div id="a" style="background-color: rgb(255,255,255)"></div>');
    expect(inspectElement(at("#a"), env())).toMatchObject({ kind: "token", token: "--color-background", label: "Surface", part: "background" });
  });

  it("reads the text colour first for an element with its own text", () => {
    page('<p id="a" style="background-color: rgb(255,255,255); color: rgba(0,0,0,.6)">time</p>');
    expect(inspectElement(at("#a"), env())).toMatchObject({ token: "--color-muted", label: "Text 3", part: "text" });
  });

  it("looks up through transparent ancestors for the background that is seen", () => {
    page('<section style="background-color: rgba(0,0,0,.035)"><div id="a"><span id="b"></span></div></section>');
    expect(inspectElement(at("#b"), env())).toMatchObject({ token: "--color-sunken", part: "background" });
  });

  it("uses the border when it is the only colour that matches", () => {
    page('<div id="a" style="border-color: rgba(0,0,0,.6); border-width: 1px"></div>');
    expect(inspectElement(at("#a"), env())).toMatchObject({ token: "--color-muted", part: "border" });
  });

  it("counts the other parts that use the same colour, one per kind of element", () => {
    page('<i class="row" style="background-color: rgb(255,255,255)"></i><i class="row" style="background-color: rgb(255,255,255)"></i><b class="card" style="background-color: rgb(255,255,255)"></b><u id="a" style="background-color: rgb(255,255,255)"></u>');
    const found = inspectElement(at("#a"), env());
    expect(found).toMatchObject({ kind: "token", sharedWith: 2 });
  });

  it("picks nothing for content colours and says where to change them", () => {
    page('<div data-content-colour="project"><span id="a" style="background-color: rgb(255,255,255)"></span></div>');
    expect(inspectElement(at("#a"), env())).toEqual({ kind: "content", note: "This colour belongs to the content, not the theme. A project’s colour is set on the project." });
    page('<div class="xterm"><span id="a" style="color: rgba(0,0,0,.6)">ls</span></div>');
    expect(inspectElement(at("#a"), env())).toMatchObject({ kind: "content" });
  });

  it("finds nothing when no theme colour matches", () => {
    page('<div id="a" style="background-color: rgb(1,2,3)"></div>');
    expect(inspectElement(at("#a"), env())).toEqual({ kind: "none" });
  });

  it("never reads the page it inspects as markup or runs it", () => {
    page('<img id="a" alt="&lt;script&gt;x&lt;/script&gt;" style="background-color: rgb(255,255,255)">');
    const found = inspectElement(at("#a"), env());
    expect(found).toMatchObject({ kind: "token", name: "<script>x</script>" });
  });
});
