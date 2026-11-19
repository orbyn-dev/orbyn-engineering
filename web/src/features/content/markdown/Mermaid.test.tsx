import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const initialize = vi.fn();
const draw = vi.fn(async (id: unknown, _code?: unknown) => ({ svg: `<svg xmlns="http://www.w3.org/2000/svg" id="${String(id)}" data-diagram="yes"><g/></svg>` }));
vi.mock("mermaid", () => ({ default: { initialize: (config: unknown) => initialize(config), render: draw } }));

import { DRAW_DEBOUNCE_MS, Mermaid } from "./Mermaid";
import { MAX_DIAGRAM_CHARS, sanitizeSvg } from "./mermaidSafety";

beforeEach(() => {
  vi.useFakeTimers();
  initialize.mockClear();
  draw.mockClear();
});
afterEach(() => {
  vi.useRealTimers();
});

/** Fires the debounce and flushes the draw, so a broken debounce fails the next assertion instead of sleeping. */
const settled = () => act(async () => { await vi.advanceTimersByTimeAsync(DRAW_DEBOUNCE_MS); });

describe("Mermaid", () => {
  it("initialises Mermaid at its strict security level, and draws the SVG inline", async () => {
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    await settled();
    expect(screen.getByRole("img", { name: "Diagram" })).toBeInTheDocument();
    expect(initialize).toHaveBeenCalledWith(expect.objectContaining({ startOnLoad: false, securityLevel: "strict" }));
    // Strict keeps the SVG in the page. "sandbox" would wrap it in an iframe, which the question view's rule and the
    // end-to-end checks (".md-diagram svg", no iframe in the panel) do not allow.
    expect(container.querySelector(".md-diagram svg")).not.toBeNull();
    expect(container.querySelector("iframe")).toBeNull();
  });

  it("initialises with the hardened config: SVG text labels, limits, and a locked key list", async () => {
    render(<Mermaid code="graph LR; A-->B;" />);
    await settled();
    expect(initialize).toHaveBeenCalled();
    const config = initialize.mock.calls[0]![0] as Record<string, unknown>;
    expect(config).toMatchObject({
      startOnLoad: false,
      securityLevel: "strict",
      htmlLabels: false,
      suppressErrorRendering: true,
      maxTextSize: MAX_DIAGRAM_CHARS,
    });
    expect(config.maxEdges).toBeGreaterThan(0);
    expect(config).not.toHaveProperty("flowchart");
    expect(config).not.toHaveProperty("themeCSS");
    expect(config).not.toHaveProperty("themeVariables");
  });

  it("locks the keys an init directive could use to weaken the config", async () => {
    render(<Mermaid code={'%%{init: {"securityLevel":"loose","htmlLabels":true}}%%\ngraph LR; A-->B;'} />);
    await settled();
    expect(initialize).toHaveBeenCalled();
    const config = initialize.mock.calls[0]![0] as { secure: string[]; securityLevel: string };
    expect(config.securityLevel).toBe("strict");
    for (const key of ["securityLevel", "htmlLabels", "startOnLoad", "maxTextSize", "maxEdges", "theme", "themeCSS", "themeVariables", "suppressErrorRendering"]) {
      expect(config.secure).toContain(key);
    }
    // Every setting the app makes is itself locked, so a directive cannot change what the app chose.
    for (const key of Object.keys(config)) if (key !== "secure") expect(config.secure, key).toContain(key);
  });

  it("shows the source while a diagram is not drawn", () => {
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    expect(container.querySelector(".md-diagram")).toBeNull();
  });

  it("strips script, foreignObject and handlers from the drawn SVG", async () => {
    draw.mockImplementationOnce(async (id: unknown) => ({
      svg: `<svg xmlns="http://www.w3.org/2000/svg" id="${String(id)}" onload="alert(1)"><script>alert(1)</script><foreignObject><div>x</div></foreignObject><a href="javascript:alert(1)"><text>t</text></a><text onclick="x()">ok</text></svg>`,
    }));
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    await settled();
    expect(container.querySelector(".md-diagram svg")).not.toBeNull();
    const html = container.querySelector(".md-diagram")!.innerHTML;
    expect(html).not.toMatch(/script|foreignObject|onload|onclick|javascript:/i);
    expect(html).toContain("ok");
  });

  it("keeps the source for an oversized diagram and never calls Mermaid", async () => {
    const { container } = render(<Mermaid code={"graph LR;\n" + "A-->B;".repeat(MAX_DIAGRAM_CHARS)} />);
    await settled();
    expect(draw).not.toHaveBeenCalled();
    expect(container.querySelector(".md-diagram")).toBeNull();
  });

  it("falls back to the source on a render error, without throwing or leaving a render node", async () => {
    draw.mockImplementationOnce(async (id: unknown) => {
      const stray = document.createElement("div");
      stray.id = `d${String(id)}`;
      document.body.appendChild(stray);
      throw new Error("Parse error");
    });
    const { container } = render(<Mermaid code="graph LR; A--" />);
    await settled();
    expect(draw).toHaveBeenCalled();
    expect(document.querySelector('[id^="dmermaid"]')).toBeNull();
    expect(container.querySelector(".md-diagram")).toBeNull();
    expect(container.textContent).toContain("A--");
  });

  it("falls back to the source when the output is not one SVG document", async () => {
    draw.mockResolvedValueOnce({ svg: "<div>not svg</div>" });
    const { container } = render(<Mermaid code="graph LR; A-->B;" />);
    await settled();
    expect(draw).toHaveBeenCalled();
    expect(container.querySelector(".md-diagram")).toBeNull();
  });
});

const SVG = (inner: string) => `<svg xmlns="http://www.w3.org/2000/svg" id="s">${inner}</svg>`;
const clean = (markup: string, id = "s") => sanitizeSvg(markup, id)?.outerHTML ?? "";

describe("sanitizeSvg", () => {
  it("removes every unsafe attribute and keeps every safe one on an element that holds many", () => {
    // A live attribute list walked while removals reshape it would leave every other unsafe attribute behind.
    const attrs = Array.from({ length: 200 }, (_, i) => (i % 2 ? `data-safe-${i}="v${i}"` : `onx${i}="bad()"`)).join(" ");
    const out = sanitizeSvg(SVG(`<rect ${attrs}/>`), "s")!;
    const rect = out.querySelector("rect")!;
    expect(rect.getAttributeNames().filter((name) => name.startsWith("onx"))).toEqual([]);
    expect(rect.getAttributeNames().filter((name) => name.startsWith("data-safe-"))).toHaveLength(100);
  });

  it("keeps a label written with a non-breaking space entity", () => {
    expect(sanitizeSvg(SVG("<text>a&nbsp;b</text>"), "s")?.textContent).toBe("a\u00a0b");
  });

  it("unwraps a link, keeping the node and label but not the link", () => {
    const out = clean(SVG('<a xlink:href="https://evil.test" href="javascript:alert(1)" target="_blank"><rect id="n"/><text>Label</text></a>'));
    expect(out).toContain('<rect id="n">');
    expect(out).toContain("Label");
    expect(out).not.toMatch(/<a[ >]|href|evil|javascript/i);
  });

  it("drops processing instructions, CDATA and comments (mXSS payload)", () => {
    const hostile = [
      '<svg xmlns="http://www.w3.org/2000/svg"><?x > <img src=x onerror=window.__pwn1=1> ?></svg>',
      SVG("<![CDATA[<img src=x onerror=window.__pwn2=1>]]>"),
      SVG("<!-- <img src=x onerror=window.__pwn3=1> --><g/>"),
    ];
    for (const markup of hostile) {
      const out = sanitizeSvg(markup, "s");
      const holder = document.createElement("div");
      if (out) holder.append(out);
      expect(holder.querySelector("img")).toBeNull();
      for (const el of Array.from(holder.querySelectorAll("*"))) {
        expect(el.getAttributeNames().filter((n) => n.startsWith("on"))).toEqual([]);
      }
      expect(holder.innerHTML).not.toMatch(/<!--|<\?/);
    }
  });

  it("rejects CSS reached through escapes, comments and image-set in style elements and attributes", () => {
    for (const css of [
      '@\\69mport "https://x/a.css";',
      "a{fill:u\\72l(https://x/a)}",
      'a{background:image-set("https://x/a.png" 1x)}',
      "a{background:u/**/rl(https://x/a)}",
      "a{background:SRC(https://x/a)}",
      "a{width:expression(alert(1))}",
    ]) {
      expect(clean(SVG(`<style>${css}</style><g/>`)), css).not.toContain("<style");
    }
    for (const css of ["fill:u\\72l(https://x/a)", 'background:image-set("https://x/a.png" 1x)', "fill:url( 'https://x/a' )"]) {
      expect(clean(SVG(`<rect style="${css.replace(/"/g, "&quot;")}"/>`)), css).not.toContain("style=");
    }
  });

  it("removes script, frames, embeds, external references and handlers but keeps theme CSS and #id references", () => {
    const out = clean(
      SVG(
        '<style>#s .a{fill:red}</style><iframe/><object/><image href="http://x/i.png"/><path marker-end="url(#m)" d="M0 0" onclick="x()"/><rect style="fill:url(#g)"/><use xlink:href="data:text/xml,x"/><script>1</script>',
      ),
    );
    expect(out).not.toMatch(/iframe|object|embed|image|http:\/\/x|data:|<script|onclick/i);
    expect(out).toContain("fill:red");
    // <embed> ends foreign content in the HTML parser; it can never survive into the result either way.
    expect(clean(SVG("<embed/><g/>"))).not.toMatch(/embed/i);
    expect(out).toContain("url(#m)");
    expect(out).toContain("url(#g)");
  });

  it("keeps Mermaid's own scoped rules and keyframes but drops rules outside the root id", () => {
    const mermaidCss = '#d{font-family:sans-serif;fill:#000;}#d .node rect,#d .node circle{fill:#eee;}@keyframes dash{to{stroke-dashoffset:0;}}';
    const svg = (css: string) => `<svg xmlns="http://www.w3.org/2000/svg" id="d"><style>${css}</style><g/></svg>`;
    expect(sanitizeSvg(svg(mermaidCss), "d")?.querySelector("style")?.textContent).toBe(mermaidCss);
    const mixed = sanitizeSvg(svg(`${mermaidCss}body{display:none}*{opacity:0}#d,body{color:red}#dx{fill:red}.a{fill:red}`), "d")?.querySelector("style")?.textContent;
    expect(mixed).toBe(mermaidCss);
    expect(clean(svg("body{display:none}"))).not.toContain("<style");
  });

  it("drops a style block it cannot parse, and any rule when the root has no id", () => {
    for (const css of ["#d{fill:red", "#d{fill:red}}", '#d{content:"x}', "#d{fill:red};@charset 'x';", "#d{fill:red}body"]) {
      expect(clean(`<svg xmlns="http://www.w3.org/2000/svg" id="d"><style>${css}</style></svg>`, "d"), css).not.toContain("<style");
    }
    expect(sanitizeSvg('<svg xmlns="http://www.w3.org/2000/svg"><style>#d{fill:red}</style></svg>', "d")).toBeNull();
  });

  it("removes a <style> that holds elements, which the HTML parser makes real children", () => {
    const hostile = [
      '<style>#s{fill:red}<image href="x" onerror="alert(1)"/></style><g/>',
      '<style>#s{fill:red}<animate onbegin="alert(1)" attributeName="x" dur="1s"/></style><g/>',
    ];
    for (const markup of hostile) {
      const out = clean(SVG(markup));
      expect(out, markup).not.toMatch(/<style|image|animate|onerror|onbegin|alert/i);
    }
  });

  it("drops a style block where a comment marker, escape or newline hides a rule from a simple reader", () => {
    const blocks = [
      '#s{content:"/*"}body{display:none}#s{content:"*/"}',
      '#s{content:"/*"}body{background:url(https://evil.test/x)}#s{content:"*/"}',
      '#s{a:\\"}body{display:none}#s{b:"}',
      '#s{a:"\n}body{display:none}#s{b:"}',
      '#s{a:"\r}body{display:none}#s{b:"}',
    ];
    for (const css of blocks) {
      const markup = `<svg xmlns="http://www.w3.org/2000/svg" id="s"><style>${css}</style><g/></svg>`;
      const out = sanitizeSvg(markup, "s")?.outerHTML ?? "";
      expect(out, JSON.stringify(css)).not.toMatch(/<style|body|evil/);
    }
  });

  it("refuses a malformed url( that would swallow the next rule, in style blocks and attributes", () => {
    for (const bad of ["url(#a')", 'url(#a")', "url(#a(b)", "url(#a", "url(#a b)"]) {
      const out = clean(SVG(`<style>#s{fill:${bad}}body{display:none}#s{b:'}</style><g/>`));
      expect(out, bad).not.toMatch(/<style|body/);
      expect(clean(SVG(`<rect style="fill:${bad}"/>`)), bad).not.toContain("style=");
    }
    for (const good of ["url(#a)", 'url("#a")', "url('#a')", "url( #a-1 )"]) {
      expect(clean(SVG(`<style>#s{fill:${good}}</style><g/>`)), good).toContain("<style");
    }
  });

  it("drops rules that nest CSS inside the declarations", () => {
    for (const css of [
      "#s{fill:red;body:has(&) p{display:none}}",
      "#s{~ *{display:none}}",
      "#s{& ~ *{display:none}}",
      "#s{@media all{p{display:none}}}",
    ]) {
      expect(clean(SVG(`<style>${css}</style><g/>`)), css).not.toContain("<style");
    }
    expect(clean(SVG('<style>#s{content:"{x}"}</style><g/>'))).toContain("<style");
  });

  it("reads a quoted brace in a selector or at-rule prelude as part of the string, not as the block start", () => {
    for (const css of [
      '#s:not([x="{"]),body p{display:none;content:"x"}',
      "#s:not([x='{']),body p{display:none;content:'x'}",
      '#s:not([x="{"]) ~ *{display:none;content:"x"}',
      "#s:not([x='{']) ~ *{display:none;content:'x'}",
      '#s[x="}"],body{display:none}#s[y="{"]{fill:red}',
      '#s[x="{"],body{display:none;content:"}"}',
      '#s{fill:red}body[x="{"]{display:none;content:"x"}',
      '@media x"{"{body{display:none}}',
      '@keyframes dash"{"x{to{opacity:0}}body{display:none;content:"x"}',
      '#s:not([x="{"]),html{display:none;content:"x"}',
      '#s:not([x="{"])+*{display:none;content:"x"}',
      '#s:is([x="{"]),body p{display:none;content:"x"}',
      '#s:not([x="{ "]),body p{display:none;content:"x"}',
      "#s:not([x='{'], [y=\"{\"]),body p,#s{display:none;content:'x'}",
      '#s:has([x="{"]),:root{display:none;content:"x"}',
      "#s:is(a,body){display:none}",
      '#s:where([x="{"],body){display:none;content:"x"}',
    ]) {
      expect(clean(SVG(`<style>${css}</style><g/>`)), css).not.toMatch(/display:none/);
    }
    // Mermaid's own attribute selectors still pass.
    expect(clean(SVG('<style>#s [data-look="neo"].node rect{fill:red}#s [id$="-arrowhead"] path{fill:red}</style><g/>'))).toContain("data-look");
  });

  it("never lets a hostile style leave a rule outside the root id", () => {
    const hostile = [
      "body{display:none}",
      "*{opacity:0}",
      "#s,body{display:none}",
      "#sx{fill:red}",
      "#s~*{display:none}",
      "#s+*{display:none}",
      "#s{fill:red}\n#s{fill:red}body{display:none}",
      '#s{content:"/*"}body{display:none}#s{content:"*/"}',
      '#s{a:\\"}body{display:none}#s{b:"}',
      "#s{fill:url(#a')}body{display:none}#s{b:'}",
      "#s{fill:red;body:has(&) p{display:none}}",
      "#s{~ *{display:none}}",
      "#s{@media all{p{display:none}}}",
      '#s:not([x="{"]),body p{display:none;content:"x"}',
      "#s:not([x='{']),body p{display:none;content:'x'}",
      '#s:not([x="{"]) ~ *{display:none;content:"x"}',
      "@media all{body{display:none}}",
      "@font-face{font-family:x;src:url(https://evil.test/f)}",
      "@import url(https://evil.test/a.css);",
      "@keyframes spin{to{opacity:0}}body{display:none}",
      ":root{--x:1}",
      "html{display:none}",
      "#s{fill:red}}body{display:none}",
      "#s{fill:red}body{display:none}{",
      "#s:is(a,body){display:none}",
      "#s:has(~ body){display:none}",
      '#s{fill:red}#s [x="a,b"],body{display:none}',
      "@page{margin:0}",
      "#s{fill:red}@namespace x;body{display:none}",
    ];
    for (const css of hostile) {
      const root = sanitizeSvg(`<svg xmlns="http://www.w3.org/2000/svg" id="s"><style>${css}</style><g/></svg>`, "s");
      const holder = document.createElement("div");
      if (root) holder.append(root);
      document.body.append(holder);
      try {
        for (const sheet of Array.from(document.styleSheets)) {
          for (const rule of Array.from(sheet.cssRules) as CSSRule[]) {
            const text = rule.cssText;
            if (/^@keyframes\s+(dash|edge-animation-frame|s-[\w-]+)\b/.test(text)) continue;
            const selector = (rule as CSSStyleRule).selectorText ?? "";
            expect(selector.length, `${css} -> ${text}`).toBeGreaterThan(0);
            for (const one of selector.split(",")) expect(one.trim(), `${css} -> ${text}`).toMatch(/^#s(?![\w-])/);
          }
        }
      } finally {
        holder.remove();
      }
      expect(holder.innerHTML, css).not.toMatch(/evil\.test|body\{|html\{|@media|@import|@font-face/);
    }
  });

  it("removes comment nodes inside a style element even when every rule is kept", () => {
    expect(clean(SVG("<style><!-- x -->#s{fill:red}</style><g/>"))).toBe(clean(SVG("<style>#s{fill:red}</style><g/>")));
    expect(clean(SVG("<style><!-- x -->#s{fill:red}</style><g/>"))).not.toContain("<!--");
  });

  it("keeps only Mermaid's own keyframes names or ones prefixed with the root id", () => {
    const frames = (name: string) => clean(SVG(`<style>@keyframes ${name}{to{opacity:0}}</style><g/>`));
    expect(frames("spin")).not.toContain("<style");
    expect(frames("dash")).toContain("@keyframes dash");
    expect(frames("edge-animation-frame")).toContain("@keyframes edge-animation-frame");
    expect(frames("s-fade")).toContain("@keyframes s-fade");
    expect(sanitizeSvg('<svg xmlns="http://www.w3.org/2000/svg"><style>@keyframes spin{to{opacity:0}}</style></svg>', "")).toBeNull();
  });

  it("rejects selectors with an escape in the id part or a sibling combinator", () => {
    for (const selector of ["#s\\31 ", "#s ~ *", "#s + *", "#s .a, #s ~ b"]) {
      expect(clean(SVG(`<style>${selector}{fill:red}</style><g/>`)), selector).not.toContain("<style");
    }
    expect(clean(SVG("<style>#s .a > .b{fill:red}</style><g/>"))).toContain("<style");
  });

  it("drops CSS image() and the animateColor, animateMotion and uppercase animate elements", () => {
    expect(clean(SVG('<style>#x{background:image("https://x/a.png")}</style>'))).not.toContain("<style");
    expect(clean(SVG('<rect style="background:image(\'https://x/a.png\')"/>'))).not.toContain("style=");
    const out = clean(
      SVG(
        '<rect id="r"/><animateColor attributeName="fill" to="red"/><animateMotion href="#r" path="M0 0"/><ANIMATE attributeName="href" to="https://evil.test/x"/><g/>',
      ),
    );
    expect(out).not.toMatch(/animate/i);
    expect(out).toContain('<rect id="r">');
  });

  it("drops HTML-namespace children of desc and title (integration points)", () => {
    const out = clean(
      SVG('<desc><img src="x" onerror="window.__pwn=1"><iframe src="javascript:alert(1)"></iframe></desc><title><img src="x" onerror="window.__pwn=2"></title><g/>'),
    );
    expect(out).not.toMatch(/<img|iframe|onerror|javascript/i);
    expect(out).toContain("<desc>");
  });

  it("drops <set> and external references on use and feImage", () => {
    expect(clean(SVG('<a id="a"/><set attributeName="href" to="https://evil.test/x"/><g/>'))).not.toMatch(/<set|evil/i);
    expect(clean(SVG('<set attributeName="href" to="javascript:alert(1)"/><g/>'))).not.toMatch(/<set|javascript/i);
    expect(clean(SVG('<use href="data:image/svg+xml,x"/>'))).not.toMatch(/data:/);
    expect(clean(SVG('<filter id="f"><feImage href="https://evil.test/x.png"/></filter>'))).not.toMatch(/evil/);
  });

  it("returns null when the root id is not the one the caller gave Mermaid", () => {
    const markup = '<svg xmlns="http://www.w3.org/2000/svg" id="root"><style>#root p{display:none;color:red}</style><g/></svg>';
    expect(sanitizeSvg(markup, "root")).not.toBeNull();
    expect(sanitizeSvg(markup, "mermaidr1")).toBeNull();
    expect(sanitizeSvg('<svg xmlns="http://www.w3.org/2000/svg"><g/></svg>', "mermaidr1")).toBeNull();
  });

  it("checks selectors in linear time and still accepts Unicode names", () => {
    const slow = [
      `#s ${"a".repeat(40)}!`,
      `#s${" ".repeat(40)}!`,
      `#s .${"a".repeat(38)}é!`,
      `#s ${"a".repeat(40)}é!`,
      `#s ${"ab ".repeat(14)}!`,
    ];
    for (const selector of slow) {
      const started = performance.now();
      expect(clean(SVG(`<style>${selector}{fill:red}</style><g/>`)), selector).not.toContain("<style");
      expect(performance.now() - started, selector).toBeLessThan(200);
    }
    const name = `${"a".repeat(40)}é`;
    expect(clean(SVG(`<style>#s .${name}{fill:red}</style><g/>`))).toContain(name);
  });

  it("returns null when there is no SVG root", () => {
    expect(sanitizeSvg("<div>no</div>", "s")).toBeNull();
    expect(sanitizeSvg("", "s")).toBeNull();
  });
});
