import {
  useEffect,
  useRef,
  useState,
  type ClipboardEvent,
  type KeyboardEvent,
  type MouseEvent,
  type WheelEvent,
} from "react";
import { serviceSocketUrl } from "../api";
import { pickPreviewElement, resizePreview, sendPreviewInput } from "../api";
import { readToken } from "../api";
import type { PickedElement, PreviewTab } from "../../../api/types";
import { toFailure } from "../../../lib/useAsync";
import { useUi } from "../../../product/state/ui";
import { AgentCursor } from "./AgentCursor";
import { copyFromPage } from "./PageBridge";
import { RESIZE_DELAY_MS, RESIZE_SLACK, q } from "./browserModel";
import { ResizeHandles, type PageResize } from "./ResizeHandles";
import { pageBox, pageSize, type PageSize } from "./viewportResize";

function frameSocket(threadId: string, tabId: string) {
  const token = readToken();
  const query = token ? `?token=${q(token)}` : "";
  return new WebSocket(serviceSocketUrl(`/ws/preview/${q(threadId)}/${q(tabId)}${query}`));
}

/** The page's frames, with input forwarded while you hold control. */
export function PageView({
  threadId,
  tab,
  fitted: follow = true,
  picking = false,
  resize = null,
  onPicked,
}: {
  threadId: string;
  tab: PreviewTab;
  /** False leaves the page at the size it was given. */
  fitted?: boolean;
  picking?: boolean;
  /** Set by the device toolbar while it shows a page at a size of its own. */
  resize?: PageResize | null;
  onPicked?(picked: PickedElement): void;
}) {
  const [frame, setFrame] = useState<string | null>(null);
  const [lost, setLost] = useState(false);
  const [stageSize, setStageSize] = useState<PageSize>({ width: 0, height: 0 });
  const stage = useRef<HTMLImageElement>(null);
  const area = useRef<HTMLDivElement>(null);
  const pushToast = useUi((state) => state.pushToast);
  const human = tab.control === "human";
  const boxed = resize !== null;
  const page = resize?.live ?? { width: tab.width, height: tab.height };
  const box = pageBox(page, stageSize);
  useEffect(() => {
    const socket = frameSocket(threadId, tab.id);
    socket.onmessage = (event) => {
      setLost(false);
      setFrame(String(event.data));
    };
    socket.onclose = () => setLost(true);
    return () => {
      socket.onclose = null;
      socket.close();
    };
  }, [threadId, tab.id]);
  const send = (body: { kind: string; x?: number; y?: number; delta_y?: number; text?: string }) => {
    if (!human) return;
    sendPreviewInput(threadId, tab.id, body).catch((failure: unknown) =>
      pushToast(`Couldn't reach the page. ${toFailure(failure).message}`),
    );
  };
  // The size the page has, or was last asked to take. The tab list only catches up
  // on its next poll, and a click mapped from the old size lands somewhere else.
  const sized = useRef({ width: tab.width, height: tab.height });
  useEffect(() => {
    sized.current = { width: tab.width, height: tab.height };
  }, [tab.width, tab.height]);
  const point = (event: MouseEvent) => {
    const box = stage.current?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    return {
      x: ((event.clientX - box.left) / box.width) * sized.current.width,
      y: ((event.clientY - box.top) / box.height) * sized.current.height,
    };
  };
  const onKey = (event: KeyboardEvent) => {
    if (!human) return;
    const shortcut = (event.metaKey || event.ctrlKey) && !event.altKey && !event.shiftKey;
    const letter = event.key.toLowerCase();
    // Pasting arrives as its own event, with the text the person chose to paste.
    if (shortcut && letter === "v") return;
    event.preventDefault();
    if (shortcut && letter === "c") {
      copyFromPage(threadId, tab.id).catch((failure: unknown) =>
        pushToast(`Couldn't copy from the page. ${toFailure(failure).message}`),
      );
      return;
    }
    if (event.key.length === 1 && !event.metaKey && !event.ctrlKey) send({ kind: "text", text: event.key });
    else send({ kind: "key", text: event.key });
  };
  const onPaste = (event: ClipboardEvent) => {
    if (!human) return;
    event.preventDefault();
    const text = event.clipboardData.getData("text/plain");
    if (text) send({ kind: "text", text });
  };
  const onWheel = (event: WheelEvent) => send({ kind: "scroll", ...point(event), delta_y: event.deltaY });
  const fitted = useRef({ width: tab.width, height: tab.height });
  useEffect(() => {
    const element = area.current;
    if (!element || !follow) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      clearTimeout(timer);
      timer = setTimeout(() => {
        const size = pageSize(entry.contentRect.width, entry.contentRect.height);
        if (Math.abs(size.width - fitted.current.width) + Math.abs(size.height - fitted.current.height) < RESIZE_SLACK) return;
        const before = sized.current;
        fitted.current = size;
        // Input sent from now on reaches the page after this resize, so it maps to the new size.
        sized.current = size;
        resizePreview(threadId, tab.id, size).catch((failure: unknown) => {
          if (sized.current === size) sized.current = before;
          pushToast(`Couldn't resize the page. ${toFailure(failure).message}`);
        });
      }, RESIZE_DELAY_MS);
    });
    observer.observe(element);
    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, [threadId, tab.id, pushToast, follow]);
  // The box is sized from the stage's pixels, which no CSS rule can hand it.
  useEffect(() => {
    const element = area.current;
    if (!element || !boxed) return;
    const measure = () => {
      const rect = element.getBoundingClientRect();
      setStageSize((previous) =>
        previous.width === rect.width && previous.height === rect.height
          ? previous
          : { width: rect.width, height: rect.height },
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [boxed]);
  const click = (event: MouseEvent) => {
    if (!picking) {
      send({ kind: "click", ...point(event) });
      return;
    }
    pickPreviewElement(threadId, tab.id, point(event)).then(
      (picked) => onPicked?.(picked),
      (failure: unknown) => pushToast(`Couldn't read that element. ${toFailure(failure).message}`),
    );
  };
  const picture = frame ? (
    <img
      ref={stage}
      className="browser-frame"
      data-preview
      data-human={human || undefined}
      tabIndex={0}
      alt={`Page at ${tab.url}`}
      src={`data:image/jpeg;base64,${frame}`}
      onClick={click}
      onKeyDown={onKey}
      onPaste={onPaste}
      onWheel={onWheel}
    />
  ) : null;
  return (
    <div ref={area} className="browser-stage" data-picking={picking || undefined}>
      {picture && resize ? (
        <div className="browser-box" style={{ width: box.width, height: box.height }}>
          {picture}
          {/* One stream per tab: the service clears the cue itself on navigation or a resize, and a
            remount here would drop a cue sent while the new stream connects. */}
          <AgentCursor key={`${threadId}:${tab.id}`} threadId={threadId} tabId={tab.id} image={stage} />
          {resize.handles ? (
            <ResizeHandles
              page={page}
              stage={stageSize}
              scale={box.scale}
              onPreview={resize.onPreview}
              onCommit={resize.onCommit}
            />
          ) : null}
        </div>
      ) : picture ? (
        <>{picture}<AgentCursor key={`${threadId}:${tab.id}`} threadId={threadId} tabId={tab.id} image={stage} /></>
      ) : (
        <p className="browser-note">{lost ? "Browser connection was refused." : "Connecting..."}</p>
      )}
      {tab.crashed ? (
        <p className="browser-note" role="alert">
          This page keeps crashing. Close the tab and open it again.
        </p>
      ) : null}
    </div>
  );
}
