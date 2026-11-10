import { Globe, GitFork, Link2, Plug } from "lucide-react";
import { argEntries, callLabel, plain, subagentTask, isWebAddress, hostOf } from "../../toolCalls";
import { BlockHead, Kvs } from "./BlockParts";
import { InspectItem } from "./InspectItem";
import type { BlockContext } from "./types";

/** A fetched page. */
export function fetchBlock({ item, body, out, showInBrowser, timing }: BlockContext) {
  const url = typeof body.url === "string" ? body.url : null;
  return (
    <div className="cb">
      <BlockHead lead={<Globe size={14} />}>{url ? hostOf(url) : "Page"}</BlockHead>
      {url ? <Kvs rows={[["URL", url]]} /> : null}
      {out ? <div className="co sans">{plain(out)}</div> : null}
      <div className="cf">
        {timing()}
        {url ? (
          <>
            {isWebAddress(url) ? (
              <button type="button" className="bt" onClick={() => showInBrowser(url)}>
                <Globe size={14} aria-hidden />
                <span>Open in Browser</span>
              </button>
            ) : null}
            <button type="button" className="bt" onClick={() => void navigator.clipboard?.writeText(url)}>
              <Link2 size={14} aria-hidden />
              <span>Copy link</span>
            </button>
          </>
        ) : null}
        <InspectItem item={item} />
      </div>
    </div>
  );
}

/** A connector call: its arguments and result. */
export function mcpBlock({ item, cwd, out, timing }: BlockContext) {
  const label = callLabel(item, cwd);
  const rows = argEntries(item);
  return (
    <div className="cb">
      <BlockHead lead={<Plug size={14} />}>
        {label.verb} · {label.object}
      </BlockHead>
      {rows.length ? <Kvs rows={rows} /> : null}
      {out ? <div className="co sans">{plain(out)}</div> : null}
      <div className="cf">
        {timing()}
        <InspectItem item={item} />
      </div>
    </div>
  );
}

/** A subagent's task and result. */
export function subagentBlock({ item, cwd, body, out, timing }: BlockContext) {
  const rows: [string, string][] = [];
  const task = subagentTask(body) ?? "";
  if (task) rows.push(["Task", task]);
  if (out) rows.push(["Result", plain(out)]);
  return (
    <div className="cb">
      <BlockHead lead={<GitFork size={14} />}>{callLabel(item, cwd).object ?? "Subagent"}</BlockHead>
      {rows.length ? <Kvs rows={rows} /> : null}
      <div className="cf">
        {timing()}
        <InspectItem item={item} />
      </div>
    </div>
  );
}
