import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { useUi } from "../../../product/state/ui";
import { CodeBlock } from "../code/CodeBlock";
import { MAX_DIAGRAM_CHARS, mermaidConfig, sanitizeSvg } from "./mermaidSafety";

const dark = (theme: string) =>
  theme === "dark" || (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);

/** Draws a diagram; while it's incomplete or invalid, the source stands in. Mermaid SVG renders inline; no other SVG or HTML is rendered. */
export function Mermaid({ code }: { code: string }) {
  const theme = useUi((state) => state.theme);
  const id = useId().replace(/[^a-zA-Z0-9]/g, "");
  const [drawn, setDrawn] = useState<{ code: string; svg: SVGElement } | null>(null);
  useEffect(() => {
    let current = true;
    // Settles after streaming; a broken diagram simply keeps its source showing.
    const timer = setTimeout(() => {
      void import("mermaid")
        .then(async ({ default: mermaid }) => {
          if (code.length > MAX_DIAGRAM_CHARS) return;
          mermaid.initialize(mermaidConfig(dark(theme)));
          const { svg } = await mermaid.render(`mermaid${id}`, code);
          const clean = sanitizeSvg(svg, `mermaid${id}`);
          if (current && clean) setDrawn({ code, svg: clean });
        })
        .catch(() => {
          // Bad syntax or over a limit: Mermaid may leave a stray render node behind; drop it.
          document.getElementById(`dmermaid${id}`)?.remove();
        });
    }, 120);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [code, id, theme]);
  if (drawn?.code !== code) return <CodeBlock code={code} lang="mermaid" />;
  return <Drawing svg={drawn.svg} />;
}

/** Inserts the already-cleaned SVG nodes directly, so what was checked is what the page gets. */
function Drawing({ svg }: { svg: SVGElement }) {
  const host = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    host.current?.replaceChildren(document.importNode(svg, true));
  }, [svg]);
  return <div ref={host} className="md-diagram" role="img" aria-label="Diagram" />;
}
