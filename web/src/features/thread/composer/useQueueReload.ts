import { useEffect, useRef } from "react";
import { useThreadLogs } from "../../../product/state/threadLogs";

/** Reads the queued messages again after an engine refused a steer, so its text shows back in the queue. */
export function useQueueReload(threadId: string, reload: () => void): void {
  const version = useThreadLogs((state) => state.queueVersion[threadId] ?? 0);
  const seen = useRef({ threadId, version });
  useEffect(() => {
    const previous = seen.current;
    seen.current = { threadId, version };
    if (previous.threadId === threadId && previous.version !== version) reload();
  }, [threadId, version, reload]);
}
