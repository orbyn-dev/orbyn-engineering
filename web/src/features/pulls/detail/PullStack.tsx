import { Layers } from "lucide-react";
import { useState } from "react";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../state/ui";
import { Button } from "../../../ui/controls";
import { pullsApi } from "../../../api/pulls";

/** Where this pull request sits among the ones stacked on each other. */
export function Stack({ projectId, number, onOpen }: { projectId: string; number: number; onOpen?(number: number): void }) {
  const stack = useAsync(`pull-stack:${projectId}:${number}`, (signal) => pullsApi.stack(projectId, number, signal));
  const [method, setMethod] = useState("squash");
  const pushToast = useUi((state) => state.pushToast);
  if (!stack.data || stack.data.layers.length < 2) return null;
  const { layers, position } = stack.data;
  const act = (action: string) => {
    pullsApi.stackAction(projectId, number, action, action === "merge" ? method : undefined).then(
      (results) => {
        const failed = results.find((result) => !result.ok);
        pushToast(failed ? `Stopped at #${failed.number}. ${failed.message ?? ""}` : "Stack updated");
        stack.reload();
      },
      (failure: unknown) => pushToast(`Couldn't do that. ${toFailure(failure).message}`),
    );
  };
  return (
    <nav className="pull-stack" aria-label="Stack">
      <Layers size={14} aria-hidden />
      <span>
        {position + 1} of {layers.length}
      </span>
      {layers.map((layer) => (
        <button
          type="button"
          key={layer.number}
          aria-current={layer.number === number ? "true" : undefined}
          onClick={() => onOpen?.(layer.number)}
        >
          #{layer.number}
        </button>
      ))}
      <select aria-label="Stack merge method" value={method} onChange={(event) => setMethod(event.target.value)}>
        <option value="squash">Squash</option>
        <option value="merge">Merge</option>
        <option value="rebase">Rebase</option>
      </select>
      <Button small onClick={() => act("merge")}>
        Merge stack
      </Button>
      <Button small onClick={() => act("rebase")}>
        Rebase stack
      </Button>
    </nav>
  );
}
