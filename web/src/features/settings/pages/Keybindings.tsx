import { useState } from "react";
import { readKeybindings, saveKeybinding } from "../api";
import { inDesktopApp } from "../api";
import { commandLabel, whenClause, whenPhrase } from "./commandLabels";
import { formatShortcut, runHandlers, shortcutForEvent } from "../../../product/shell/shortcuts";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { AlertCircle, MoreHorizontal, Pencil, Search } from "lucide-react";
import { IconButton } from "../../../ui/controls";
import { Menu } from "../../../ui/Menu";
import { useUi } from "../../../product/state/ui";
import { ErrorState, Loading } from "../../../product/ui/states";

const CAP_GLYPHS: Record<string, string> = { Enter: "↵", Backspace: "⌫" };

/** One key cap per key: ⌘ ⇧ B on a Mac, Ctrl Shift B elsewhere. */
function keyCaps(rule: string): string[] {
  const text = formatShortcut(rule);
  const mac = text.match(/^([⌘⌃⌥⇧]+)(.+)$/);
  const parts = mac ? [...Array.from(mac[1] ?? ""), mac[2] ?? ""] : text.includes("+") ? text.split(/\+(?=.)/) : [text];
  return parts.map((part) => CAP_GLYPHS[part] ?? part);
}

const load = (signal?: AbortSignal) => readKeybindings(signal);

export function KeybindingsSection() {
  const view = useAsync("keybindings", load, { keep: true });
  const [filter, setFilter] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; command: string } | null>(null);
  const pushToast = useUi((state) => state.pushToast);

  const capture = (event: React.KeyboardEvent, command: string, when: string | null) => {
    const key = shortcutForEvent(event);
    if (key === null) return;
    event.preventDefault();
    if (event.key === "Escape") {
      setEditing(null);
      return;
    }
    setEditing(null);
    saveKeybinding({ key, command, when, remove: false }).then(
      (next) => {
        view.setData(next);
        runHandlers("keybindings.reload");
      },
      (failure: unknown) => setError(toFailure(failure).message),
    );
  };

  return (
    <>
      <h1 className="sr-only" tabIndex={-1} data-page-title>
        Keybindings
      </h1>
      <label className="set-find">
        <Search size={13} aria-hidden />
        <input aria-label="Filter keybindings" placeholder="Find a shortcut" value={filter} onChange={(event) => setFilter(event.target.value)} />
      </label>
      <p className="setting-line-desc">
        {inDesktopApp()
          ? `${formatShortcut("mod+w")} is reserved: it never closes the window.`
          : `A browser closes the tab on ${formatShortcut("mod+w")}, so its bindings here also answer to ${formatShortcut("alt+w")}.`}
      </p>
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
      {view.status === "loading" ? (
        <Loading label="Loading keybindings" rows={6} />
      ) : view.status === "error" ? (
        <ErrorState title="Couldn't load keybindings" failure={view.error} onRetry={view.reload} />
      ) : (
        <>
          {view.data.issues.length ? (
            <>
              <p className="banner banner-err" role="alert">
                <AlertCircle size={13} aria-hidden />
                {view.data.issues.length} {view.data.issues.length === 1 ? "problem" : "problems"} in keybindings.json. The other shortcuts still work.
              </p>
              <div className="set-card set-issues">
                {view.data.issues.map((issue) => <span key={issue}>{issue}</span>)}
              </div>
            </>
          ) : null}
          {view.data.conflicts.map((conflict) => (
            <p key={`${conflict.key}:${conflict.when ?? ""}`} className="banner banner-warn">
              <AlertCircle size={13} aria-hidden />
              {conflict.commands.map(commandLabel).join(" and ")} both use {keyCaps(conflict.key).join("")}{whenClause(conflict.when) ? ` ${whenClause(conflict.when)}` : ""}.
            </p>
          ))}
          <div className="set-tbl" role="table" aria-label="Keybindings">
            <div className="set-tr h" role="row" style={{ gridTemplateColumns: "minmax(0, 1fr) 230px 150px 72px" }}>
              <span role="columnheader">Action</span><span role="columnheader">When</span><span role="columnheader">Shortcut</span><span role="columnheader" />
            </div>
            {view.data.rules
              .filter((rule) => !rule.remove && `${commandLabel(rule.command)} ${rule.command} ${whenPhrase(rule.when)} ${rule.key}`.toLowerCase().includes(filter.toLowerCase()))
              .map((rule) => {
                const id = `${rule.command}:${rule.key}:${rule.when ?? ""}`;
                const clash = view.data.conflicts.find((conflict) => conflict.key === rule.key && (conflict.when ?? "") === (rule.when ?? "") && conflict.commands.includes(rule.command));
                return (
                  <div key={id} role="row" className="set-tr" style={{ gridTemplateColumns: "minmax(0, 1fr) 230px 150px 72px" }}>
                    <span role="cell" title={rule.command}>{commandLabel(rule.command)}</span>
                    <span role="cell" className="t3" title={rule.when ?? undefined}>{whenPhrase(rule.when)}</span>
                    <span role="cell">
                      {editing === id ? <button type="button" className="btn btn-small" onKeyDown={(event) => capture(event, rule.command, rule.when)} ref={(element) => element?.focus()}>Press keys…</button>
                        : <span className="keycaps" aria-label={formatShortcut(rule.key)}>{keyCaps(rule.key).map((cap, at) => <span key={at} className={clash ? "kc warn" : "kc"} aria-hidden>{cap}</span>)}</span>}
                    </span>
                    <span role="cell">
                      <IconButton label={`Change shortcut for ${commandLabel(rule.command)}`} onClick={() => setEditing(id)}><Pencil size={13} aria-hidden /></IconButton>
                      <IconButton
                        label={`More for ${commandLabel(rule.command)}`}
                        aria-haspopup="menu"
                        onClick={(event) => {
                          const box = event.currentTarget.getBoundingClientRect();
                          setMenu({ x: box.left, y: box.bottom + 4, command: rule.command });
                        }}
                      >
                        <MoreHorizontal size={13} aria-hidden />
                      </IconButton>
                    </span>
                  </div>
                );
              })}
          </div>
          {menu ? (
            <Menu
              x={menu.x}
              y={menu.y}
              label="Keybinding actions"
              onClose={() => setMenu(null)}
              items={[{
                label: "Copy id",
                onSelect: () => void navigator.clipboard?.writeText(menu.command).then(() => pushToast(`Copied ${menu.command}`)),
              }]}
            />
          ) : null}
          <p className="muted">{view.data.path}</p>
        </>
      )}
    </>
  );
}
