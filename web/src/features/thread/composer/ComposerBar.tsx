import { ArrowUp, Archive, ListChecks, Paperclip, SlidersHorizontal, Square } from "lucide-react";
import type { RefObject } from "react";
import type { EngineStatus, ModelInfo, PermissionMode, Thread } from "../../../api/types";
import type { StashedPrompt } from "../../../product/state/ui";
import { IconButton } from "../../../ui/controls";
import { ContextMeter } from "./ComposerExtras";
import { ACCESS, EffortTrigger, type useTraitsMenu } from "./ComposerTraits";
import { ModelPickerTrigger } from "./ModelPicker";

/** The row under the field: attach, stash, plan, model, effort, access and send or stop. */
export function ComposerBar({
  bar,
  filePicker,
  thread,
  engine,
  engineName,
  installed,
  model,
  draft,
  stashes,
  stashOpen,
  planControls,
  planMode,
  narrowBar,
  contextMeter,
  showContext,
  unmanaged,
  access,
  busy,
  stopping,
  sending,
  canSend,
  serviceDown,
  traits,
  upload,
  setStashOpen,
  setPlanMode,
  openOverflow,
  pickModel,
  stop,
}: {
  bar: RefObject<HTMLDivElement | null>;
  filePicker: RefObject<HTMLInputElement | null>;
  thread: Thread;
  engine: EngineStatus | null;
  engineName: string;
  installed: EngineStatus[];
  model: ModelInfo | null;
  draft: string;
  stashes: StashedPrompt[];
  stashOpen: boolean;
  planControls: boolean;
  planMode: boolean;
  narrowBar: boolean;
  contextMeter: boolean;
  showContext: boolean;
  unmanaged: boolean;
  access: PermissionMode;
  busy: boolean;
  stopping: boolean;
  sending: boolean;
  canSend: boolean;
  serviceDown: boolean;
  traits: Pick<ReturnType<typeof useTraitsMenu>, "openAt">;
  upload(files: FileList | File[] | null): void;
  setStashOpen(open: boolean): void;
  setPlanMode(on: boolean): Promise<void>;
  openOverflow(element: HTMLElement): void;
  pickModel(next: { engine: string; model: string | null }): Promise<void>;
  stop(): Promise<void>;
}) {
  const AccessIcon = ACCESS[access].icon;
  return (
    <div ref={bar} className="composer-bar">
      <input
        ref={filePicker}
        type="file"
        multiple
        hidden
        aria-label="Attach files"
        onChange={(event) => upload(event.target.files)}
      />
      <IconButton
        label="Attach files"
        onClick={() => filePicker.current?.click()}
      >
        <Paperclip size={16} aria-hidden />
      </IconButton>
      {stashes.length > 0 ? (
        <button
          type="button"
          className="composer-control"
          aria-label={`${stashes.length} stashed prompts`}
          onClick={() => setStashOpen(!stashOpen)}
        >
          <Archive size={14} aria-hidden />
          <span>{stashes.length}</span>
        </button>
      ) : null}
      {(planControls || planMode) && !narrowBar ? (
        <button
          type="button"
          className="composer-control"
          aria-pressed={planMode}
          data-on={planMode || undefined}
          disabled={busy || !thread.lifecycle}
          onClick={() => void setPlanMode(!planMode)}
        >
          <ListChecks size={14} aria-hidden />
          <span>Plan</span>
        </button>
      ) : null}
      <span className="composer-spacer" />
      {contextMeter ? <ContextMeter threadId={thread.id} disabled={busy} /> : null}
      {narrowBar ? (
        <IconButton label="Composer controls" onClick={(event) => openOverflow(event.currentTarget)}>
          <SlidersHorizontal size={16} aria-hidden />
        </IconButton>
      ) : null}
      <span className="composer-controls" data-folded={narrowBar || undefined}>
      {engine ? (
        <ModelPickerTrigger
          engines={installed}
          selected={{ engine: thread.engine, model: thread.model }}
          disabled={busy}
          onPick={(next) => void pickModel(next)}
        />
      ) : (
        <span className="composer-control">{thread.model ?? engineName}</span>
      )}
      <EffortTrigger
        model={model}
        effort={thread.effort ?? null}
        access={unmanaged ? null : access}
        disabled={busy}
        onOpen={(element) => traits.openAt(element, false)}
      />
      {!showContext && !unmanaged ? (
        <button
          type="button"
          className="composer-control composer-access"
          aria-haspopup="menu"
          aria-label={`Permissions: ${ACCESS[access].label}`}
          disabled={busy || !thread.lifecycle}
          onClick={(event) => traits.openAt(event.currentTarget, true)}
        >
          <AccessIcon size={14} aria-hidden />
          <span>{ACCESS[access].label}</span>
        </button>
      ) : null}
      </span>
      {busy && !draft.trim() ? (
        <button
          type="button"
          className="send-btn stop-btn"
          aria-label="Stop"
          disabled={stopping}
          onClick={() => void stop()}
        >
          <Square size={12} fill="currentColor" aria-hidden />
        </button>
      ) : (
        <button
          type="submit"
          className="send-btn"
          aria-label={serviceDown ? "Send, unavailable" : busy ? "Queue" : "Send"}
          aria-busy={sending || undefined}
          disabled={!canSend}
        >
          <ArrowUp size={16} aria-hidden />
        </button>
      )}
    </div>
  );
}
