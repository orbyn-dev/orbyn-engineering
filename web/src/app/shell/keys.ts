import { useEffect, useRef } from "react";
import { api, setSlowRequestHandler } from "../../api/client";
import { inDesktopApp, zoomKeyAction } from "../../api/shell";
import { releaseUndoShortcut, undoHasShortcut, undoLatest, useUndoNotice } from "../../features/workspace";
import { useAsync } from "../../lib/useAsync";
import { actionIdFromCommand } from "../../product/composer/actionCommand";
import { navigate, parseRoute, type Route } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { FALLBACK_RULES, actionShortcutAllowed, isReservedCloseEvent, matchesKey, ruleMatches, runHandlers, setTerminalRules, useCommand, whenHolds, type WhenContext } from "../../product/shell/shortcuts";
import { PANEL_KEYS, editing, openThread, runShellCommand } from "./commands";

function keyContext(route: Route): WhenContext {
  const active = document.activeElement;
  const ui = useUi.getState();
  return {
    terminalFocus: active?.closest(".terminal-drawer-body") != null,
    previewFocus: active?.closest("[data-preview]") != null,
    composerFocus: active?.closest(".composer") != null,
    editableFocus: editing(active),
    dialogOpen: document.querySelector("dialog[open]") !== null,
    rightPanelOpen: ui.panelOpen === true,
    usagePageOpen: route.name === "usage",
    turnRunning: openThread(route)?.active_run != null,
    draftRoute: route.name === "new" || route.name === "home",
  };
}

/** The undo shortcut outranks everything while its notice shows. */
function undoKey(event: KeyboardEvent): boolean {
  const mod = /mac/i.test(navigator.platform) ? event.metaKey : event.ctrlKey;
  const key = event.key.toLowerCase();
  if (!mod) {
    if (!["shift", "meta", "control", "alt"].includes(key)) releaseUndoShortcut();
    return false;
  }
  if (key !== "z" || event.shiftKey || event.altKey) return false;
  if (useUndoNotice.getState().notice && (undoHasShortcut() || !editing(event.target))) {
    event.preventDefault();
    void undoLatest();
    return true;
  }
  return false;
}

const SLOW_TOAST_GAP_MS = 60_000;

/** Says so when the service is slow, once a minute at most. */
function useSlowRequestToast() {
  useEffect(() => {
    let last = 0;
    setSlowRequestHandler(() => {
      if (Date.now() - last < SLOW_TOAST_GAP_MS) return;
      last = Date.now();
      useUi.getState().pushToast("The Orbyn service is taking a while to answer.");
    });
    return () => setSlowRequestHandler(() => undefined);
  }, []);
}

export function useGlobalKeys(route: Route) {
  useSlowRequestToast();
  const keymap = useAsync("keybindings", (signal) => api.keybindings(signal));
  const reload = keymap.reload;
  useCommand("keybindings.reload", () => void reload());
  const rules = keymap.data?.rules ?? FALLBACK_RULES;
  useEffect(() => {
    setTerminalRules(rules, () => keyContext(parseRoute(window.location.hash)));
    return () => setTerminalRules([]);
  }, [rules]);
  const pushToast = useUi((state) => state.pushToast);
  const seenFile = useRef<string | null>(null);
  useEffect(() => {
    const data = keymap.data;
    if (!data) return;
    const signature = JSON.stringify([data.rules, data.issues]);
    if (data.issues.length > 0 && signature !== seenFile.current) pushToast(`The keybindings file has a problem. ${data.issues[0] ?? ""}`);
    else if (seenFile.current !== null && signature !== seenFile.current) pushToast("Keybindings reloaded");
    seenFile.current = signature;
  }, [keymap.data, pushToast]);
  useEffect(() => {
    window.addEventListener("focus", reload);
    return () => window.removeEventListener("focus", reload);
  }, [reload]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const actionAllowed = () => actionShortcutAllowed(event, keyContext(route));
      if (event.key === "Escape" && !event.defaultPrevented) {
        if (actionAllowed()) {
          for (const rule of [...rules].reverse()) {
            if (!rule.remove && actionIdFromCommand(rule.command) !== null && matchesKey(rule.key, event)
              && whenHolds(rule.when, keyContext(route)) && (runHandlers(rule.command) || runHandlers("projectActions.pending", rule.command))) {
              event.preventDefault(); return;
            }
          }
        }
        if (route.name === "thread" && !document.querySelector("dialog[open]")) {
          const { panelOpen, closePanel } = useUi.getState();
          if (panelOpen === true) closePanel();
        }
        return;
      }
      if (undoKey(event)) return;
      const zoom = inDesktopApp() ? zoomKeyAction(event) : null;
      if (zoom) {
        event.preventDefault();
        api.shellAction(zoom).catch(() => undefined);
        return;
      }
      // A focused surface that took the key, like the model picker's number jumps, keeps it.
      if (event.defaultPrevented) return;
      const modified = event.metaKey || event.ctrlKey || event.altKey || /^F\d+$/.test(event.key);
      if (modified || actionAllowed()) {
        const context = keyContext(route);
        // The last rule wins, so the file overrides a default.
        for (const rule of [...rules].reverse()) {
          const action = actionIdFromCommand(rule.command) !== null;
          if ((!modified && !action) || (action && !actionAllowed())) continue;
          if (rule.remove || !ruleMatches(rule.key, event, inDesktopApp()) || !whenHolds(rule.when, context)) continue;
          if (runHandlers(rule.command) || (action && runHandlers("projectActions.pending", rule.command)) || runShellCommand(rule.command, route)) {
            event.preventDefault();
            return;
          }
        }
      }
      // Nothing took it, but the desktop window still must not close on it.
      if (inDesktopApp() && isReservedCloseEvent(event)) {
        event.preventDefault();
        return;
      }
      const digit = /^Digit([1-9])$/.exec(event.code);
      if (digit && (event.metaKey || event.ctrlKey) && !event.shiftKey && !event.altKey) {
        if (runShellCommand(`thread.jump.${digit[1]}`, route)) event.preventDefault();
        return;
      }
      if (!event.altKey || event.metaKey || event.ctrlKey) return;
      if (event.code === "KeyN") {
        event.preventDefault();
        navigate({ name: "new", projectId: null });
      } else if (route.name === "thread" && PANEL_KEYS[event.code]) {
        event.preventDefault();
        useUi.getState().togglePanel(PANEL_KEYS[event.code] ?? "diff");
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [route, rules]);
}
