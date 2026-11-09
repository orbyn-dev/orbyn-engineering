import { useCompacting } from "../ThreadStates";
import { composerMarkup } from "./composerMarkup";
import { MentionMenu, type Suggestion } from "./MentionMenu";
import { AttachmentStrip } from "./AttachmentStrip";
import { CodeBar } from "./CodeBar";
import { ComposerContext } from "./ComposerContext";
import { controlsMenuItems } from "./controlsMenu";
import {
  effortButton,
  MAX_HEIGHT_RATIO,
  MENU_LIFT,
  MENU_ROWS,
  NARROW_BAR,
  NO_CHIPS,
  NO_STASHES,
  STYLED_CHARS,
  WIDE,
} from "./composerConstants";
import { loadSuggestions } from "./suggestions";
import { handleComposerKey } from "./composerKeys";
import { handleComposerPaste } from "./composerPaste";
import { useAttachments } from "./useAttachments";
import { ComposerBar } from "./ComposerBar";
import { ComposerField } from "./ComposerField";
import { useCommand as useSharedCommand } from "../../../product/shell/shortcuts";
import { useStopRun } from "./useStopRun";
import { useThreadSettings } from "./useThreadSettings";
import { useStash } from "./useStash";
import { recordUserPrompt } from "../../workspace";
import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ClipboardEvent,
  type DragEvent,
  type KeyboardEvent,
} from "react";
import { api } from "../../../api/client";
import type { EngineStatus, Thread } from "../../../api/types";
import { useLayoutWidth } from "../../../lib/useLayout";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useConnectionKind } from "../../../product/state/connection";
import { useDraftStore } from "../../../product/state/draftStore";
import { navigate } from "../../../product/state/route";
import { useThreadLogs } from "../../../product/state/threadLogs";
import { useUi } from "../../../product/state/ui";
import { Menu } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { useWorkspace } from "../../../product/state/workspace";
import { CompactOffer, ComposerBanners, InlineLimits, StashDrawer, StopNotice } from "./ComposerExtras";
import { localCommand, promptHistory, type RecallPosition } from "./composerInput";
import { offeredModes, shownAccess, useTraitsMenu } from "./ComposerTraits";
import { QuestionPanel } from "../QuestionPanel";
import { SubagentChips } from "../SubagentBar";
import { effortOption, effortValue, resolveModel, useEngineModels } from "../../../product/state/modelStore";
import { QueuedMessages } from "../QueuedMessages";
import { openSideChat } from "../sideChat";
import { FILE_MENTION_TYPE, insertToken, triggerAt } from "../../../product/composer/skillMention";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { openFenceAt } from "./composerCodeBlock";
import { ComposerChipShelf, copyChips } from "./ComposerChips";
import { MAX_PROMPT_CHARS, charactersOver, promptWithChips } from "../../../product/composer/composerContext";

/** Commands go to the thread's own composer; a side chat's stays out of their way. */
const useOwnCommand = (side: boolean, command: string, handler: () => boolean | void) =>
  useSharedCommand(command, () => (side ? false : handler()));

export function Composer({
  thread,
  engine,
  beforeSend,
  onSendFailed,
  showContext,
  side = false,
}: {
  thread: Thread;
  engine: EngineStatus | null;
  /** Runs first and may hold the send, as the hero transition does. */
  beforeSend?(): Promise<void>;
  onSendFailed?(): void;
  /** The checkout, branch and access row, shown when nothing else carries them. */
  showContext: boolean;
  /** This composer belongs to a side chat in the panel. */
  side?: boolean;
}) {
  const draft = useUi((state) => state.drafts[thread.id] ?? "");
  const setDraft = useUi((state) => state.setDraft);
  const addPrompt = useThreadLogs((state) => state.addPrompt);
  const refreshThread = useWorkspace((state) => state.refreshThread);
  const layout = useLayoutWidth();
  const field = useRef<HTMLTextAreaElement>(null);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [caret, setCaret] = useState(0);
  const [highlight, setHighlight] = useState(0);
  const [menuDismissed, setMenuDismissed] = useState(false);
  const { attachments, setAttachments, filePicker, uploading, startUpload, upload } = useAttachments(thread, setError);
  const [dragging, setDragging] = useState(false);
  const [recall, setRecall] = useState<RecallPosition | null>(null);
  const [limitsOpen, setLimitsOpen] = useState(false);
  const shiftPaste = useRef(false);
  const bar = useRef<HTMLDivElement>(null);
  const [narrowBar, setNarrowBar] = useState(false);
  const [overflow, setOverflow] = useState<{ x: number; y: number } | null>(null);
  const openOverflow = (element: HTMLElement) => {
    const rect = element.getBoundingClientRect();
    setOverflow({ x: rect.left, y: rect.top - MENU_LIFT });
  };
  useLayoutEffect(() => {
    const element = bar.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => setNarrowBar((entry?.contentRect.width ?? WIDE) < NARROW_BAR));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const caretAfterEdit = useRef<number | null>(null);
  /** Where a kept selection ends after an edit; null collapses it to the caret. */
  const selectionEndAfterEdit = useRef<number | null>(null);
  const mirror = useRef<HTMLDivElement>(null);
  const followUpBehavior = useUi((state) => state.followUpBehavior);
  const sendShortcut = useUi((state) => state.sendShortcut);
  const persistentContext = useUi((state) => state.persistentComposerContext);
  const skillsInSlash = useUi((state) => state.skillsInSlashMenu);
  const planControls = useUi((state) => state.planControls);
  const contextMeter = useUi((state) => state.contextMeter);
  const stashes = useUi((state) => state.stashes[thread.id] ?? NO_STASHES);
  const chips = useComposerChips((state) => state.chips[thread.id] ?? NO_CHIPS);
  const addChips = useComposerChips((state) => state.addMany);
  const removeChip = useComposerChips((state) => state.remove);
  const consumeChips = useComposerChips((state) => state.consume);
  const log = useThreadLogs((state) => state.logs[thread.id]);
  const history = useMemo(() => promptHistory(log?.items ?? []), [log]);
  const { stopping, stoppedRun, stop } = useStopRun(thread, setError);
  const projectBranch = useWorkspace(
    (state) => state.managed.data?.targets.find((t) => t.project_id === thread.project_id)?.branch ?? null,
  );
  const branch = thread.worktree?.branch ?? projectBranch;
  // Until the workspace status names the branch, a Git project keeps its place.
  const branchPending = useWorkspace(
    (state) =>
      !thread.worktree &&
      state.managed.data === null &&
      state.managed.error === null &&
      (state.projects.data?.find((project) => project.id === thread.project_id)?.is_git ?? false),
  );
  const busy = thread.active_run !== null;
  const outbound = useMemo(() => promptWithChips(draft, chips), [draft, chips]);
  const overBy = charactersOver(outbound);
  const styled = useMemo(() => (draft && draft.length <= STYLED_CHARS ? composerMarkup(draft) : null), [draft]);
  const fence = openFenceAt(draft, caret);
  // The draft is kept while the Orbyn service does not answer, and sending waits for it.
  const serviceDown = useConnectionKind() === "service-down";
  const compacting = useCompacting(thread);
  const canSend = outbound !== "" && !sending && !uploading && overBy === 0 && !serviceDown && !compacting;
  const queue = useAsync(`queue:${thread.id}`, (signal) => api.queue(thread.id, signal));
  const reloadQueue = queue.reload;
  const runId = thread.active_run?.id ?? null;
  useEffect(() => {
    reloadQueue();
  }, [runId, reloadQueue]);

  const trigger = menuDismissed ? null : triggerAt(draft, caret);
  const lookupKey = trigger ? `${thread.id}:${trigger.char}:${trigger.query}` : `${thread.id}:idle`;
  const lookup = useAsync<Suggestion[]>(lookupKey, (signal) =>
    loadSuggestions(trigger, thread, planControls, skillsInSlash, signal),
  );
  const matches = useMemo(() => (trigger ? (lookup.data ?? []).slice(0, MENU_ROWS) : []), [trigger, lookup.data]);
  const menuOpen = matches.length > 0;
  const active = Math.min(highlight, Math.max(matches.length - 1, 0));

  useLayoutEffect(() => {
    const element = field.current;
    if (!element) return;
    element.style.height = "auto";
    const limit = window.innerHeight * MAX_HEIGHT_RATIO;
    element.style.height = `${Math.min(element.scrollHeight + 2, limit)}px`;
    // Before the next paint, so a key pressed right after lands at the caret.
    if (caretAfterEdit.current !== null) {
      element.setSelectionRange(caretAfterEdit.current, selectionEndAfterEdit.current ?? caretAfterEdit.current);
    }
    caretAfterEdit.current = null;
    selectionEndAfterEdit.current = null;
    if (mirror.current) mirror.current.scrollTop = element.scrollTop;
  }, [draft]);

  const pick = (suggestion: Suggestion) => {
    if (!trigger) return;
    if (runLocal(`/${suggestion.value}`)) return;
    const next = insertToken(draft, caret, trigger, suggestion.value, suggestion.insert);
    setDraft(thread.id, next.text);
    setCaret(next.caret);
    caretAfterEdit.current = next.caret;
  };

  const { putThread, setPlanMode, setAccess, setEffort, pickModel } = useThreadSettings(thread, setError);

  /** A terminal selection is evidence, not a command; it is never consumed by one. */
  const hasTerminalContext = chips.some((chip) => chip.kind === "terminal");
  const hasReviewContext = chips.some((chip) => chip.kind === "review");
  const contextName = hasTerminalContext ? "terminal context" : "review comments";

  /** Handles what the composer does itself; the agent never sees these. */
  const runLocal = (text: string): boolean => {
    const command = localCommand(text);
    if (!command) return false;
    if (hasTerminalContext || hasReviewContext) {
      setError(`Remove the ${contextName} before using /${command}. Your message stays here.`);
      return true;
    }
    setDraft(thread.id, "");
    if (command === "plan") void setPlanMode(true);
    if (command === "default") void setPlanMode(false);
    if (command === "usage-limits") setLimitsOpen(true);
    return true;
  };

  const send = async (followUp: "default" | "opposite" = "default") => {
    if (!canSend) return false;
    const typed = draft;
    const prompt = promptWithChips(draft, chips);
    const serviceCommand = /^\/(compact|goal)(?:\s|$)/.exec(typed.trim());
    if ((hasTerminalContext || hasReviewContext) && serviceCommand) {
      setError(`Remove the ${contextName} before using /${serviceCommand[1]}. Your message stays here.`);
      return false;
    }
    if (runLocal(typed)) return false;
    const sideChat = side ? null : /^\/(?:side|btw)(?:\s+([\s\S]*))?$/.exec(typed.trim());
    if ((hasTerminalContext || hasReviewContext) && (sideChat || pendingHere.length > 0)) {
      setError(
        sideChat
          ? `Side chats take text only. Remove the ${contextName}, then send again.`
          : `Remove the ${contextName} before replying to an approval. Your message stays here.`,
      );
      return false;
    }
    if (sideChat) {
      setDraft(thread.id, "");
      void openSideChat(thread, sideChat[1]?.trim() || null);
      return false;
    }
    const queued = followUpBehavior === "queue";
    const steering = busy && (followUp === "default" ? !queued : queued);
    setSending(true);
    setError(null);
    try {
      await beforeSend?.();
      // Claude hears a typed reply as the reason it was declined, so nothing is queued.
      if (pendingHere.length > 0 && engine?.capabilities.decline_feedback) {
        await Promise.all(pendingHere.map((approval) => api.decideApproval(approval.id, false, false, typed.trim())));
        setDraft(thread.id, "");
        void refreshThread(thread.id);
        recordUserPrompt();
        return true;
      }
      await Promise.all(pendingHere.map((approval) => api.decideApproval(approval.id, false, false)));
      const ids = attachments.flatMap((file) => file.id ?? []);
      const run = busy ? null : await api.startRun(thread.id, prompt.trim(), ids);
      if (steering) {
        const steered = await api.steer(thread.id, prompt.trim());
        if (steered.state === "queued") useUi.getState().pushToast("This engine reads it on its next turn");
      } else if (busy) await api.enqueue(thread.id, prompt.trim(), ids);
      setAttachments([]);
      consumeChips(thread.id, chips);
      setRecall(null);
      // Text typed after pressing Send stays; only what was sent is removed.
      const current = useUi.getState().drafts[thread.id] ?? "";
      setDraft(thread.id, current === typed ? "" : current);
      if (run) addPrompt(thread.id, { runId: run.id, text: prompt.trim() });
      if (busy) reloadQueue();
      void refreshThread(thread.id);
      recordUserPrompt();
      return true;
    } catch (failure) {
      onSendFailed?.();
      const known = toFailure(failure);
      if (known.code === "folder_untrusted") {
        navigate({ name: "trust", projectId: thread.project_id, threadId: thread.id });
        return false;
      }
      setError(
        known.code === "thread_busy"
          ? "A run started first. Your message is kept, so send it again to queue it."
          : steering
            ? `Couldn't steer. ${known.message} Your message is kept.`
            : `Couldn't start the run. ${known.message} Your message is kept.`,
      );
      void refreshThread(thread.id);
      return false;
    } finally {
      setSending(false);
    }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) =>
    handleComposerKey(event, {
      thread,
      draft,
      menuOpen,
      matches,
      active,
      history,
      recall,
      planControls,
      planMode,
      busy,
      sendShortcut,
      shiftPaste,
      caretAfterEdit,
      selectionEndAfterEdit,
      setHighlight,
      setMenuDismissed,
      setRecall,
      setDraft,
      pick,
      setPlanMode,
      send,
    });

  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) =>
    handleComposerPaste(event, { thread, shiftPaste, addChips, upload });

  const onDrop = (event: DragEvent) => {
    event.preventDefault();
    setDragging(false);
    const mentioned = event.dataTransfer.getData(FILE_MENTION_TYPE);
    if (mentioned) {
      setDraft(thread.id, `${draft.trimEnd()}${draft.trim() ? " " : ""}@${mentioned} `);
      field.current?.focus();
    } else if (event.dataTransfer.files.length > 0) upload([...event.dataTransfer.files]);
  };

  const droppedHere = useDraftStore((state) => state.dropped[thread.id]);
  useEffect(() => {
    if (!droppedHere) return;
    upload(useDraftStore.getState().takeDropped(thread.id));
  });

  const { stashOpen, setStashOpen, stash, dropStash, restoreStash } = useStash({
    thread,
    draft,
    stashes,
    field,
    setDraft,
  });

  useOwnCommand(side, "composer.focus", () => {
    field.current?.focus();
  });
  useOwnCommand(side, "composer.stash", () => {
    stash();
  });
  useOwnCommand(side, "composer.sendAndNewThread", () => {
    if (!canSend) return false;
    void send().then((sent) => {
      if (sent) navigate({ name: "new", projectId: thread.scratch ? null : thread.project_id });
    });
  });
  useOwnCommand(side, "thread.steerQueuedMessage", () => {
    const oldest = queue.data?.[0];
    if (!oldest || !busy) return false;
    void api
      .steer(thread.id, oldest.prompt)
      .then(() => api.removeQueued(oldest.id))
      .then(reloadQueue, (failure: unknown) => setError(`Couldn't steer. ${toFailure(failure).message}`));
  });

  const approvals = useWorkspace((state) => state.approvals.data);
  const pendingHere = useMemo(
    () => (approvals ?? []).filter((approval) => approval.thread_id === thread.id),
    [approvals, thread.id],
  );

  const unmanaged = engine !== null && !engine.capabilities.managed;
  const planMode = thread.lifecycle?.plan_mode ?? false;
  // A saved mode this engine does not offer runs Supervised, so it shows that way.
  const access = shownAccess(engine, thread.lifecycle?.permission_mode ?? "supervised");
  const engineName = engine?.name ?? thread.engine;
  const allEngines = useWorkspace((state) => state.engines.data);
  const installed = useMemo(() => (allEngines ?? []).filter((entry) => entry.installed), [allEngines]);
  const models = useEngineModels(thread.engine);
  const model = resolveModel(models, thread.model);
  useOwnCommand(side, "thread.editQueuedMessage", () => {
    const last = queue.data?.at(-1);
    if (!last) return false;
    void api.removeQueued(last.id).then(() => {
      setDraft(thread.id, last.prompt);
      reloadQueue();
    });
  });

  useOwnCommand(side, "composer.effort", () => {
    const trigger = effortButton();
    if (!trigger) return false;
    traits.openAt(trigger, false);
  });

  useOwnCommand(side, "composer.mode", () => {
    const trigger = effortButton();
    if (!trigger || unmanaged) return false;
    traits.openAt(trigger, true);
  });
  const effortLevel = effortValue(model, thread.effort ?? null);
  const effortShown = effortOption(model) !== null;
  const stepEffort = (step: 1 | -1) => {
    const values = effortOption(model)?.values ?? [];
    if (values.length === 0 || busy) return false;
    const at = values.findIndex((value) => value.id === effortValue(model, thread.effort ?? null)?.id);
    const next = values[(Math.max(at, 0) + step + values.length) % values.length];
    if (next) void setEffort(next.id);
    return true;
  };
  useOwnCommand(side, "composer.increaseEffort", () => stepEffort(1));
  useOwnCommand(side, "composer.decreaseEffort", () => stepEffort(-1));
  const traits = useTraitsMenu({
    model,
    effort: thread.effort ?? null,
    extras: thread.options ?? {},
    access: unmanaged ? null : access,
    modes: offeredModes(engine),
    onEffort: (value) => void setEffort(value),
    onOption: (option, value) => {
      setError(null);
      api.patchThread(thread.id, { options: { [option]: value } }).then(putThread, (failure: unknown) => {
        setError(`Couldn't change that. ${toFailure(failure).message}`);
      });
    },
    onAccess: (mode) => void setAccess(mode),
  });

  return (
    <form
      className="composer"
      onSubmit={(event) => {
        event.preventDefault();
        void send();
      }}
    >
      <div className="composer-inner">
      {error ? (
        <p className="field-error composer-error" role="alert">
          {error}
        </p>
      ) : null}
      {busy ? <SubagentChips agents={log?.subagents ?? []} /> : null}
      <QueuedMessages
        threadId={thread.id}
        items={queue.data ?? []}
        running={busy}
        onChange={reloadQueue}
        onEdit={(prompt) => setDraft(thread.id, draft.trim() ? `${draft}\n\n${prompt}` : prompt)}
      />
      <QuestionPanel threadId={thread.id} waiting={thread.active_run?.state === "awaiting_approval"} />
      <ComposerBanners engine={engine} />
      <CompactOffer threadId={thread.id} engine={thread.engine} active={busy} items={log?.items ?? []} />
      {runId !== null && runId === stoppedRun ? <StopNotice threadId={thread.id} runId={runId} /> : null}
      {limitsOpen ? <InlineLimits threadId={thread.id} onClose={() => setLimitsOpen(false)} /> : null}
      {stashOpen ? (
        <StashDrawer entries={stashes} onRestore={restoreStash} onDelete={dropStash} onClose={() => setStashOpen(false)} />
      ) : null}
      <div
        className="composer-box"
        data-dragging={dragging || undefined}
        onDragOver={(event) => {
          event.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={onDrop}
      >
        <Presence>
          {menuOpen ? (
            <MentionMenu
              matches={matches}
              active={active}
              marker={trigger?.char ?? ""}
              onPick={pick}
            />
          ) : null}
        </Presence>
        <ComposerChipShelf
          threadId={thread.id}
          chips={chips}
          onUpdate={(chip) => {
            const state = useComposerChips.getState();
            const next = (state.chips[thread.id] ?? []).map((current) => current.id === chip.id ? chip : current);
            if (charactersOver(promptWithChips(useUi.getState().drafts[thread.id] ?? "", next))) return false;
            state.update(thread.id, chip);
            return true;
          }}
          onRemove={(id) => removeChip(thread.id, id)}
          onCopy={(copy) =>
            void copyChips(copy).then((ok) =>
              useUi.getState().pushToast(ok ? "Chip copied" : "Couldn't copy the chip"),
            )
          }
        />
        {fence ? <CodeBar thread={thread} draft={draft} fence={fence} setDraft={setDraft} /> : null}
        {attachments.length > 0 ? (
          <AttachmentStrip attachments={attachments} setAttachments={setAttachments} startUpload={startUpload} />
        ) : null}
        {overBy > 0 ? (
          <p className="composer-over" role="alert">
            {overBy.toLocaleString()} characters over the {MAX_PROMPT_CHARS.toLocaleString()} limit. Your message stays here.
          </p>
        ) : null}
        <ComposerField
          field={field}
          mirror={mirror}
          thread={thread}
          engine={engine}
          draft={draft}
          styled={styled}
          layout={layout}
          compacting={compacting}
          serviceDown={serviceDown}
          pendingHere={pendingHere}
          setDraft={setDraft}
          setCaret={setCaret}
          setHighlight={setHighlight}
          setMenuDismissed={setMenuDismissed}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
        />
        <span id="composer-help" className="sr-only">
          Enter to send, Shift+Enter for a new line
        </span>
        <ComposerBar
          bar={bar}
          filePicker={filePicker}
          thread={thread}
          engine={engine}
          engineName={engineName}
          installed={installed}
          model={model}
          draft={draft}
          stashes={stashes}
          stashOpen={stashOpen}
          planControls={planControls}
          planMode={planMode}
          narrowBar={narrowBar}
          contextMeter={contextMeter}
          showContext={showContext}
          unmanaged={unmanaged}
          access={access}
          busy={busy}
          stopping={stopping}
          sending={sending}
          canSend={canSend}
          serviceDown={serviceDown}
          traits={traits}
          upload={upload}
          setStashOpen={setStashOpen}
          setPlanMode={setPlanMode}
          openOverflow={openOverflow}
          pickModel={pickModel}
          stop={stop}
        />
      </div>
      <ComposerContext
        thread={thread}
        hidden={!showContext && !persistentContext}
        branch={branch}
        branchPending={branchPending}
        unmanaged={unmanaged}
        access={access}
        busy={busy}
        traits={traits}
      />
      {traits.menu}
      <Presence>{overflow ? (
        <Menu
          x={overflow.x}
          y={overflow.y}
          label="Composer controls"
          onClose={() => setOverflow(null)}
          items={controlsMenuItems({
            thread,
            model,
            engineName,
            effortShown,
            effortLevel,
            busy,
            unmanaged,
            access,
            planControls,
            planMode,
            traits,
            setPlanMode,
          })}
        />
      ) : null}</Presence>
      </div>
    </form>
  );
}
