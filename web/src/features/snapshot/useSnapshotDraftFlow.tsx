import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useUi } from "../../product/state/ui";
import { useSnapshot } from "./runtime";
import { SnapshotButton } from "./SnapshotButton";
import type { SnapShotDraft, SnapShotTarget } from "./types";

export interface SnapshotDraftFlow {
  visible: boolean;
  capturing: boolean;
  button: ReactNode;
  cancel(): void;
  /** The target this composer registered, for a caller that needs its own chip list. */
  target: SnapShotTarget | null;
}

/**
 * Hooks one composer to the Snap Shot runtime: the button and the shortcut land in this composer, and the taken PNG goes to
 * `receive`. Only one composer registers at a time; a side chat does not.
 */
export function useSnapshotDraftFlow({
  kind,
  threadId,
  label,
  disabled = false,
  receive,
}: {
  kind: "thread" | "draft";
  threadId: string | null;
  label: string;
  disabled?: boolean;
  receive(draft: SnapShotDraft): Promise<void>;
}): SnapshotDraftFlow {
  const status = useSnapshot((state) => state.status);
  const state = useSnapshot((state) => state.state);
  const capture = useSnapshot((state) => state.capture);
  const discard = useSnapshot((store) => store.discard);
  const captureFromPerson = useSnapshot((store) => store.captureFromPerson);
  const register = useSnapshot((store) => store.registerTarget);
  const shortcut = useUi((user) => user.snapshotShortcut);
  const [capturing, setCapturing] = useState(false);
  const targetRef = useRef<SnapShotTarget | null>(null);
  const receiveRef = useRef(receive);
  receiveRef.current = receive;
  const labelRef = useRef(label);
  labelRef.current = label;

  const target = useMemo<SnapShotTarget>(() => ({
    kind,
    get label() {
      return labelRef.current;
    },
    receive: (draft) => receiveRef.current(draft),
    phase: (phase) => setCapturing(phase === "capturing"),
  }), [kind]);
  targetRef.current = target;

  useEffect(() => {
    if (disabled || status !== "ready") return;
    return register(target);
  }, [disabled, register, status, target]);

  useEffect(() => {
    if (capture === null) setCapturing(false);
  }, [capture]);

  const visible = !disabled && status === "ready" && state !== null && state.enabled;
  return {
    visible,
    capturing,
    target: disabled ? null : target,
    button: visible ? (
      <SnapshotButton
        onClick={() => void captureFromPerson()}
        capturing={capturing}
        platform={state.platform}
        shortcut={shortcut}
        threadId={threadId}
      />
    ) : null,
    cancel() {
      const id = useSnapshot.getState().capture?.id;
      if (id) void discard(id);
      setCapturing(false);
    },
  };
}
