import { WrapText, ChevronDown } from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import type { ToolItem } from "../../../../product/timeline/types";
import { useOpenState } from "../../../../ui/openState";
import { outputSpans } from "../../../content";
import { LINES_SHOWN, lineCount } from "../../toolCalls";
import { MoreBar } from "./BlockParts";

export function OutputBlock({ item, running }: { item: ToolItem; running: boolean }) {
  const text = item.result ?? item.output;
  const [all, setAll] = useOpenState(`out-all:${item.key}`, false);
  const [wrap, setWrap] = useOpenState(`out-wrap:${item.key}`, true);
  const scroller = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  const lines = lineCount(text ?? "");
  const capped = !running && lines > LINES_SHOWN && !all;
  const mode = running ? "tail scr" : capped ? "cap" : lines > LINES_SHOWN ? "scr" : "";
  // Only a running call follows its end. A capped block is clipped, so whatever it had scrolled to would
  // stay hidden under it: it always starts at its first line, also after a run ends and after "Show less".
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    if (mode === "cap") element.scrollTop = 0;
    else if (running && following.current) element.scrollTop = element.scrollHeight;
  });
  // A line wider than the block would scroll sideways with wrapping off; measure that, and again when the block resizes.
  const [wide, setWide] = useState(false);
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const measure = () => {
      const before = element.style.whiteSpace;
      element.style.whiteSpace = "pre";
      const overflows = element.scrollWidth > element.clientWidth + 1;
      element.style.whiteSpace = before;
      setWide(overflows);
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [text, all, mode]);
  if (!text) return running ? null : <div className="co mute tool-empty">No output</div>;
  return (
    <>
      <div
        ref={scroller}
        tabIndex={mode.includes("scr") ? 0 : undefined}
        className={`co ${mode} ${wrap ? "" : "nowrap"}`}
        onScroll={(event) => {
          const element = event.currentTarget;
          following.current = element.scrollHeight - element.scrollTop - element.clientHeight <= 8;
        }}
      >
        {outputSpans(text).map((span, at) =>
          span.color || span.bold ? (
            <span key={at} style={{ color: span.color ?? undefined, fontWeight: span.bold ? 600 : undefined }}>
              {span.text}
            </span>
          ) : (
            span.text
          ),
        )}
      </div>
      {!running && lines > LINES_SHOWN ? (
        <MoreBar
          icon={<ChevronDown size={14} aria-hidden className={all ? "up" : undefined} />}
          label={all ? "Show less" : `Show all ${lines} lines`}
          note={all ? `${lines} lines · scrolls here` : `Fades after ${LINES_SHOWN} lines`}
          onClick={() => setAll(!all)}
        />
      ) : null}
      {wide || !wrap ? (
        <MoreBar
          icon={<WrapText size={14} aria-hidden />}
          label="Wrap lines"
          note={wrap ? "On" : "Scrolls sideways"}
          pressed={wrap}
          onClick={() => setWrap(!wrap)}
        />
      ) : null}
    </>
  );
}
