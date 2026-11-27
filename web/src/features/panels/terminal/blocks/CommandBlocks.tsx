import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "../../../../ui/controls";
import { useNow } from "../../../../lib/useNow";
import { blocksApi, type BlockTerminal, type CommandBlock, type CommandBlocksApi } from "./blocksContract";
import { BlockCard, isRunning } from "./BlockCard";
import { FullscreenStrip } from "./FullscreenStrip";

/** How often the list is read again while a command runs. */
const POLL_MS = 2000;
/** While nothing runs the list is still read now and then, so the first command of a quiet terminal, or the next one, appears. */
const IDLE_POLL_MS = 5000;

export interface CommandBlocksProps {
  threadId: string;
  terminalId: string;
  /** Types text into the terminal's input, without a newline; absent when the terminal cannot take a write. */
  write?: ((text: string) => void) | undefined;
  /** Sends Ctrl-C to the terminal. */
  interrupt?: (() => void) | undefined;
  /** Bumped after the drawer cleared this terminal's blocks: the list is read again from the start, so what was dropped is gone. */
  cleared?: number;
}

/**
 * The drawer's blocks, one per command. The production client draws nothing until it has blocks, so a shell that emits no marks leaves
 * the terminal exactly as it was. Every key that comes from the service is kept in a Map or Set, never as an object key.
 */
export function CommandBlocks(props: CommandBlocksProps) {
  return <Live api={blocksApi()} {...props} />;
}

type Load = { status: "loading" } | { status: "error" } | { status: "ready" };

function Live({ api, threadId, terminalId, write, interrupt, cleared }: CommandBlocksProps & { api: CommandBlocksApi }) {
  const terminal: BlockTerminal = useRefStable(threadId, terminalId);
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [newest, setNewest] = useState<CommandBlock[]>([]);
  const [earlier, setEarlier] = useState<CommandBlock[]>([]);
  const [before, setBefore] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const [attempt, setAttempt] = useState(0);
  const [selected, setSelected] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const paging = useRef(false);

  // The drawer cleared this terminal's blocks: read the list again from the start, so the dropped blocks go, a running block stays,
  // and an earlier page the old list offered is forgotten.
  const [topRead, setTopRead] = useState(0);
  const lastCleared = useRef(cleared);
  useEffect(() => {
    if (lastCleared.current === cleared) return;
    lastCleared.current = cleared;
    setEarlier([]);
    setBefore(null);
    setTopRead((value) => value + 1);
  }, [cleared]);

  const blocks = [...earlier, ...newest.filter((one) => !earlier.some((old) => old.id === one.id))];
  const running = newest.some(isRunning);
  const now = useNow(running);

  // The newest page: at the start, on Try again, and every two seconds while something runs.
  useEffect(() => {
    const controller = new AbortController();
    const fromTop = attempt === 0 && tick === 0 && topRead === 0;
    api.list(terminal, {}, controller.signal).then(
      (answer) => {
        if (controller.signal.aborted) return;
        setNewest(answer.blocks);
        // The cursor for earlier pages belongs to the first read; a later read of the newest page leaves it alone. After a clear the
        // reset above has already put it back, and a cleared list is short enough to need no cursor.
        if (fromTop) setBefore(answer.next_before);
        setLoad({ status: "ready" });
      },
      () => {
        if (!controller.signal.aborted) setLoad((old) => (old.status === "ready" ? old : { status: "error" }));
      },
    );
    return () => controller.abort();
  }, [api, terminal, attempt, tick, topRead]);

  // The newest page again every 2 s while a block runs and every 5 s otherwise (a terminal whose shell emits marks later, or a next
  // command). Only the production client reads while idle: the preview mock stays still, and a hidden page reads nothing.
  const quietClient = api.quietWhenEmpty === true;
  useEffect(() => {
    if (load.status === "loading") return;
    if (load.status === "error" && !quietClient) return;
    if (!running && !quietClient) return;
    const timer = setInterval(() => {
      if (!document.hidden) setTick((value) => value + 1);
    }, running ? POLL_MS : IDLE_POLL_MS);
    return () => clearInterval(timer);
  }, [running, load.status, quietClient]);

  const showEarlier = useCallback(async () => {
    if (paging.current || before === null) return;
    paging.current = true;
    try {
      const answer = await api.list(terminal, { before }, new AbortController().signal);
      setEarlier((old) => [...answer.blocks, ...old]);
      setBefore(answer.next_before);
    } catch {
      setLoad({ status: "error" });
    } finally {
      paging.current = false;
    }
  }, [api, terminal, before]);

  const toggle = (id: string) =>
    setCollapsed((old) => {
      const next = new Set(old);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const showAll = (id: string) => setExpanded((old) => new Set(old).add(id));

  // The production client draws the terminal alone until it has blocks; only the preview mock shows these loading, empty and error frames.
  const quiet = api.quietWhenEmpty === true;
  if (load.status === "loading") return quiet ? null : <div className="cb-list" aria-busy="true" />;
  if (load.status === "error" && blocks.length === 0) {
    if (quiet) return null;
    return (
      <div className="cb-list">
        <div className="cb-empty">
          <span>Couldn't read this terminal's blocks.</span>
          <Button small onClick={() => { setLoad({ status: "loading" }); setAttempt(attempt + 1); }}>
            Try again
          </Button>
        </div>
      </div>
    );
  }
  if (blocks.length === 0) {
    if (quiet) return null;
    return (
      <div className="cb-list">
        <div className="cb-empty">
          <b>Commands you run here appear as blocks</b>
          <span>Each block shows the command, its exit code and how long it took, and can be copied or run again. A shell that does not mark its commands shows none.</span>
        </div>
      </div>
    );
  }
  return (
    <div className="cb-list" role="group" aria-label="Command blocks">
      {before === null ? null : (
        <Button small tone="quiet" onClick={() => void showEarlier()}>
          Show earlier commands
        </Button>
      )}
      {blocks.map((block) =>
        isRunning(block) && block.fullscreen ? (
          <FullscreenStrip key={block.id} command={block.command} />
        ) : (
          <BlockCard
            key={block.id}
            api={api}
            terminal={terminal}
            block={block}
            tick={tick}
            now={now}
            selected={selected === block.id}
            collapsed={collapsed.has(block.id)}
            showAll={expanded.has(block.id)}
            onSelect={() => setSelected(block.id)}
            onToggle={() => toggle(block.id)}
            onShowAll={() => showAll(block.id)}
            onCollapseAll={() => setCollapsed(new Set(blocks.map((one) => one.id)))}
            write={write}
            interrupt={interrupt}
          />
        ),
      )}
    </div>
  );
}

/** One object per terminal, so effects that depend on it do not run on every render. */
function useRefStable(threadId: string, terminalId: string): BlockTerminal {
  const held = useRef<BlockTerminal>({ threadId, terminalId });
  if (held.current.threadId !== threadId || held.current.terminalId !== terminalId) held.current = { threadId, terminalId };
  return held.current;
}
