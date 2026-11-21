import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "../../../../ui/controls";
import { useNow } from "../../../../lib/useNow";
import { blocksApi, type BlockTerminal, type CommandBlock, type CommandBlocksApi } from "./blocksContract";
import { BlockCard, isRunning } from "./BlockCard";

/** How often the list is read again while a command runs. */
const POLL_MS = 2000;

export interface CommandBlocksProps {
  threadId: string;
  terminalId: string;
  /** Types text into the terminal's input, without a newline; absent when the terminal cannot take a write. */
  write?: ((text: string) => void) | undefined;
  /** Sends Ctrl-C to the terminal. */
  interrupt?: (() => void) | undefined;
}

/**
 * The drawer's blocks, one per command. Hidden: with no client installed (the production default) this draws nothing and sends nothing.
 * Every key that comes from the service is kept in a Map or Set, never as an object key.
 */
export function CommandBlocks(props: CommandBlocksProps) {
  const api = blocksApi();
  return api ? <Live api={api} {...props} /> : null;
}

type Load = { status: "loading" } | { status: "error" } | { status: "ready" };

function Live({ api, threadId, terminalId, write, interrupt }: CommandBlocksProps & { api: CommandBlocksApi }) {
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

  const blocks = [...earlier, ...newest.filter((one) => !earlier.some((old) => old.id === one.id))];
  const running = newest.some(isRunning);
  const now = useNow(running);

  // The newest page: at the start, on Try again, and every two seconds while something runs.
  useEffect(() => {
    const controller = new AbortController();
    api.list(terminal, {}, controller.signal).then(
      (answer) => {
        if (controller.signal.aborted) return;
        setNewest(answer.blocks);
        // The cursor for earlier pages belongs to the first read; a later read of the newest page leaves it alone.
        setBefore((old) => (attempt === 0 && tick === 0 ? answer.next_before : old));
        setLoad({ status: "ready" });
      },
      () => {
        if (!controller.signal.aborted) setLoad((old) => (old.status === "ready" ? old : { status: "error" }));
      },
    );
    return () => controller.abort();
  }, [api, terminal, attempt, tick]);

  useEffect(() => {
    if (!running || load.status !== "ready") return;
    const timer = setTimeout(() => setTick((value) => value + 1), POLL_MS);
    return () => clearTimeout(timer);
  }, [running, load.status, tick]);

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

  if (load.status === "loading") return <div className="cb-list" aria-busy="true" />;
  if (load.status === "error" && blocks.length === 0) {
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
    return (
      <div className="cb-list">
        <div className="cb-empty">
          <b>Commands Orbyn runs appear here as blocks</b>
          <span>Each block has the command's exit code and how long it took, and can be copied, run again or sent to the agent.</span>
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
      {blocks.map((block) => (
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
      ))}
    </div>
  );
}

/** One object per terminal, so effects that depend on it do not run on every render. */
function useRefStable(threadId: string, terminalId: string): BlockTerminal {
  const held = useRef<BlockTerminal>({ threadId, terminalId });
  if (held.current.threadId !== threadId || held.current.terminalId !== terminalId) held.current = { threadId, terminalId };
  return held.current;
}
