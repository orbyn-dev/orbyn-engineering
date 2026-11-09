import { Check, ChevronRight, Copy } from "lucide-react";
import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { useRoute } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { IconButton } from "../../../ui/controls";
import { Collapsible } from "../../../ui/Collapsible";
import { CodeBlock } from "../code/CodeBlock";
import { MarkdownMedia, MarkdownThreadContext } from "./MarkdownMedia";
import { ExternalLink, FileLink, MarkdownProjectContext, ThreadLink } from "./MarkdownLink";
import { INLINE, parseBlocks, type Block } from "../../../lib/markdown";
import { fileLinkTarget, safeHref, threadLinkId } from "../../../lib/links";
import { Mermaid } from "./Mermaid";
import { Tex } from "./Tex";

/**
 * A small Markdown subset rendered to React elements. There is no HTML path:
 * anything that looks like a tag stays literal text, and links must be
 * http(s) or mailto. Partial input is fine, which is what streaming produces.
 */

export { safeHref };

/** Off, the source stays as the person would have typed it. */
function InlineMath({ source }: { source: string }) {
  const enabled = useUi((state) => state.mathRendering);
  return enabled ? <Tex source={source} block={false} /> : <>${source}$</>;
}

function MathBlock({ source }: { source: string }) {
  const enabled = useUi((state) => state.mathRendering);
  return enabled ? <Tex source={source} block /> : <p dir="auto">{`$$${source}$$`}</p>;
}

function linkFor(key: string, rawHref: string, label: ReactNode, fallback: string): ReactNode {
  const thread = threadLinkId(rawHref);
  if (thread) return <ThreadLink key={key} id={thread}>{label}</ThreadLink>;
  const href = safeHref(rawHref);
  if (href) return <ExternalLink key={key} href={href}>{label}</ExternalLink>;
  const file = fileLinkTarget(rawHref);
  if (file) return <FileLink key={key} path={file.path} line={file.line}>{label}</FileLink>;
  return fallback;
}

const RemoteImages = createContext<"render" | "link">("render");

function InlineMedia({ source, alt, fallback }: { source: string; alt: string; fallback: string }) {
  const policy = useContext(RemoteImages);
  const href = safeHref(source);
  if (policy === "link" && href && /^https?:/i.test(href)) {
    return <ExternalLink href={href}>{alt || "Image"}</ExternalLink>;
  }
  return <MarkdownMedia source={source} alt={alt} fallback={fallback} />;
}

function renderInline(text: string, keyPrefix = ""): ReactNode[] {
  const out: ReactNode[] = [];
  let last = 0;
  let index = 0;
  for (const match of text.matchAll(INLINE)) {
    const at = match.index;
    if (at > last) out.push(text.slice(last, at));
    const key = `${keyPrefix}${index++}`;
    const [whole, imageAlt, imageSrc, math, , code, bold1, bold2, em1, em2, strike, linkText, linkHref, bare] = match;
    if (imageSrc !== undefined) {
      out.push(<InlineMedia key={key} source={imageSrc} alt={imageAlt ?? ""} fallback={whole} />);
    } else if (math !== undefined) out.push(<InlineMath key={key} source={math} />);
    else if (code !== undefined) out.push(<code key={key}>{code}</code>);
    else if (bold1 !== undefined || bold2 !== undefined)
      out.push(<strong key={key}>{renderInline(bold1 ?? bold2 ?? "", `${key}-`)}</strong>);
    else if (em1 !== undefined || em2 !== undefined)
      out.push(<em key={key}>{renderInline(em1 ?? em2 ?? "", `${key}-`)}</em>);
    else if (strike !== undefined)
      out.push(<del key={key}>{renderInline(strike, `${key}-`)}</del>);
    else if (linkText !== undefined) {
      out.push(linkFor(key, linkHref ?? "", renderInline(linkText, `${key}-`), whole));
    } else if (bare !== undefined) {
      out.push(linkFor(key, bare, bare, bare));
    }
    last = at + whole.length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

function withBreaks(text: string): ReactNode[] {
  const parts = text.split("\n");
  return parts.flatMap((part, at) => {
    const nodes = renderInline(part, `l${at}-`);
    return at < parts.length - 1 ? [...nodes, <br key={`br${at}`} />] : nodes;
  });
}

function Details({ block, at }: { block: Extract<Block, { type: "details" }>; at: number }) {
  return (
    <Collapsible
      className="md-details"
      persistId={`details:${at}:${block.summary}`}
      summaryClassName="trace-summary"
      summary={
        <>
          <ChevronRight size={14} className="chev" aria-hidden />
          <span className="trace-label">{renderInline(block.summary)}</span>
        </>
      }
    >
      {block.blocks.map(renderBlock)}
    </Collapsible>
  );
}

const tableText = (block: Extract<Block, { type: "table" }>) =>
  [block.head, ...block.rows].map((row) => block.head.map((_, at) => row[at] ?? "").join("\t")).join("\n");

function Table({ block }: { block: Extract<Block, { type: "table" }> }) {
  const [copied, setCopied] = useState(false);
  const copy = () =>
    void navigator.clipboard?.writeText(tableText(block)).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }, () => undefined);
  const align = (column: number) => block.aligns[column] ?? undefined;
  return (
    <div className="md-table-wrap">
      <div className="md-table" tabIndex={0}>
        <table>
          <thead>
            <tr>
              {block.head.map((cell, at) => (
                <th key={at} style={{ textAlign: align(at) }}>{renderInline(cell)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {block.rows.map((row, at) => (
              <tr key={at}>
                {block.head.map((_, cell) => (
                  <td key={cell} style={{ textAlign: align(cell) }}>{renderInline(row[cell] ?? "")}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <span className="md-table-actions">
        <IconButton label={copied ? "Copied" : "Copy table"} onClick={copy}>
          {copied ? <Check size={14} aria-hidden /> : <Copy size={14} aria-hidden />}
        </IconButton>
      </span>
    </div>
  );
}

function renderBlock(block: Block, key: number): ReactNode {
  switch (block.type) {
    case "heading": {
      const Tag = `h${block.level + 1}` as "h2" | "h3" | "h4" | "h5";
      return <Tag key={key} dir="auto">{renderInline(block.text)}</Tag>;
    }
    case "paragraph":
      return <p key={key} dir="auto">{withBreaks(block.text)}</p>;
    case "code":
      return block.lang === "mermaid" ? (
        <Mermaid key={key} code={block.text} />
      ) : (
        <CodeBlock key={key} code={block.text} lang={block.lang} />
      );
    case "quote":
      return <blockquote key={key} dir="auto">{block.blocks.map(renderBlock)}</blockquote>;
    case "math":
      return <MathBlock key={key} source={block.source} />;
    case "details":
      return <Details key={key} block={block} at={key} />;
    case "list": {
      const children = block.items.map((item, at) => {
        const done = block.tasks[at];
        return done === null || done === undefined ? (
          <li key={at} dir="auto">{item.map(renderBlock)}</li>
        ) : (
          <li key={at} dir="auto" className="md-task" id={`${key}-task-${at}`}>
            <input type="checkbox" checked={done} disabled aria-labelledby={`${key}-task-${at}`} onChange={() => undefined} />
            {item.map(renderBlock)}
          </li>
        );
      });
      return block.ordered ? (
        <ol key={key} start={block.start === 1 ? undefined : block.start}>
          {children}
        </ol>
      ) : (
        <ul key={key}>{children}</ul>
      );
    }
    case "table":
      return <Table key={key} block={block} />;
    case "rule":
      return <hr key={key} />;
  }
}

export function Markdown({ text, projectId = null, remoteImages = "render" }: { text: string; projectId?: string | null; remoteImages?: "render" | "link" }) {
  const blocks = useMemo(() => parseBlocks(text.split(/\r\n|\n|\r/)), [text]);
  const route = useRoute();
  const threadId = route.name === "thread" ? route.id : null;
  const cwd = useWorkspace((state) => (threadId === null ? null : (state.threads[threadId]?.cwd ?? null)));
  const thread = useMemo(() => (threadId === null ? null : { id: threadId, cwd }), [threadId, cwd]);
  return (
    <MarkdownProjectContext.Provider value={projectId}>
      <MarkdownThreadContext.Provider value={thread}>
        <RemoteImages.Provider value={remoteImages}>
          <div className="md">{blocks.map(renderBlock)}</div>
        </RemoteImages.Provider>
      </MarkdownThreadContext.Provider>
    </MarkdownProjectContext.Provider>
  );
}
