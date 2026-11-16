import { dataChanged, shellChanged } from "../../api/shell";
import type { ShellEvent } from "../../api/types";
import { goToHash, parseRoute } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { runHandlers } from "../../product/shell/shortcuts";
import { runShellCommand } from "./commands";

/** Links and menu items from the desktop shell, which behave like keys pressed here. */
export function onShellEvent(event: ShellEvent) {
  switch (event.type) {
    case "navigate":
      goToHash(event.href.slice(1));
      return;
    case "command":
      if (!runHandlers(event.command)) runShellCommand(event.command, parseRoute(window.location.hash));
      return;
    case "changed":
      shellChanged();
      return;
    case "data_changed":
      dataChanged(event.what);
      return;
    case "failed":
      useUi.getState().pushToast(event.message);
      return;
    case "show_preview":
      goToHash(`/threads/${event.thread_id}`);
      useUi.getState().showInBrowser({ tabId: event.tab_id });
  }
}
