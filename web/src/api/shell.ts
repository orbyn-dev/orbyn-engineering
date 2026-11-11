import { serviceUrl } from "./origin";
import { api, authHeaders } from "./client";
import { SseParser } from "./sse";
import type { ShellAction, ShellEvent } from "./types";

const RETRY_MS = 3000;
/** A stream that stayed up this long was healthy, whatever it delivered. */
const STABLE_MS = 5000;

let attached = false;

/** True in the desktop app, once the service has said so. */
export const inDesktopApp = () => attached;

export async function readShellStatus(signal?: AbortSignal) {
  const status = await api.shell(signal);
  attached = status.attached;
  return status;
}

const changeListeners = new Set<() => void>();

/** Calls back when the shell's facts change. Returns a stop function. */
export function onShellChange(listener: () => void): () => void {
  changeListeners.add(listener);
  return () => {
    changeListeners.delete(listener);
  };
}

export const shellChanged = () => changeListeners.forEach((listener) => listener());

const dataListeners = new Set<(what: string) => void>();

/**
 * Calls back when the service answered from its last known value and a fresher one has landed,
 * with what changed: `pulls`, `pull:<project>:<number>`, `pull-people:<project>`, `forges`,
 * `models:<engine>` (or `models` for every engine), `workspace`, `updates`, `limits:<engine>`,
 * `tools:<thread>`, `engine-tools:<engine>` and `health`. A listener ignores names it does not
 * show, and reads again when it opens: a slow window can miss an event. Returns a stop function.
 */
export function onDataChanged(listener: (what: string) => void): () => void {
  dataListeners.add(listener);
  return () => {
    dataListeners.delete(listener);
  };
}

export const dataChanged = (what: string) => dataListeners.forEach((listener) => listener(what));

/** Zoom keys the native menu doesn't catch: the keypad, and Cmd with the shifted `+`. */
export function zoomKeyAction(event: KeyboardEvent): ShellAction | null {
  if (!(event.metaKey || event.ctrlKey) || event.altKey) return null;
  if (event.code === "NumpadAdd" || event.key === "+") return "zoom_in";
  if (event.code === "NumpadSubtract") return "zoom_out";
  if (event.code === "Numpad0") return "zoom_reset";
  return null;
}

/**
 * Follows what the desktop shell sends the window. On a plain server nothing ever
 * arrives, and the open stream costs one idle connection. Returns a stop function.
 */
export function followShell(onEvent: (event: ShellEvent) => void): () => void {
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  // A menu command sent while the stream is down is lost, and Settings (Cmd+, ) is such a command, so a stream that
  // was up for a while is retried within 250 ms. One that connects and closes at once (a server that accepts and
  // drops) counts as a failure, so it backs off from 250 ms up to RETRY_MS instead of being retried hot forever.
  let failures = 0;
  const connect = async () => {
    let delivered = false;
    // When the stream came up, set outside the try so a stream that then errors is still measured.
    let upSince: number | null = null;
    try {
      const response = await fetch(serviceUrl("/api/shell/events"), {
        signal: controller.signal,
        credentials: "same-origin",
        headers: { ...authHeaders(), Accept: "text/event-stream" },
      });
      if (!response.ok || !response.body) throw new Error("no stream");
      upSince = Date.now();
      const parser = new SseParser();
      const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        for (const data of parser.push(value)) {
          delivered = true;
          try {
            onEvent(JSON.parse(data) as ShellEvent);
          } catch {
            // A frame this version can't read carries nothing for it.
          }
        }
      }
    } catch {
      // Retried below, unless this stream was stopped.
    }
    if (controller.signal.aborted) return;
    const upFor = upSince === null ? 0 : Date.now() - upSince;
    // Healthy: it delivered something or stayed up for a few seconds. Then the next try is the first, not a retry.
    failures = delivered || upFor >= STABLE_MS ? 1 : failures + 1;
    timer = setTimeout(() => void connect(), Math.min(RETRY_MS, 250 * 2 ** (failures - 1)));
  };

  void connect();
  return () => {
    controller.abort();
    if (timer !== undefined) clearTimeout(timer);
  };
}
