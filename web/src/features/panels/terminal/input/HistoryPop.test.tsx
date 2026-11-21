import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { HistoryPop } from "./HistoryPop";
import type { HistoryEntry } from "./history";

const entry = (command: string, at = 1): HistoryEntry => ({ command, ok: null, at });

describe("HistoryPop with a multi-line command", () => {
  const command = "for f in a b; do\n  echo $f\ndone";
  it("shows one row with the lines joined by a separator, and picks the full text", async () => {
    const onPick = vi.fn();
    const user = userEvent.setup();
    render(<HistoryPop shell={[entry(command)]} project={[]} scope="shell" onPick={onPick} onClose={() => undefined} />);
    const row = await screen.findByText("for f in a b; do ⏎ echo $f ⏎ done");
    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(row.textContent).not.toContain("\n");
    await user.keyboard("{Enter}");
    expect(onPick).toHaveBeenCalledWith(command);
  });
  it("masks a secret on a later line in the row", async () => {
    render(<HistoryPop shell={[entry("echo hi\nexport TOKEN=abcdef")]} project={[]} scope="shell" onPick={() => undefined} onClose={() => undefined} />);
    expect(await screen.findByText("echo hi ⏎ export TOKEN=••••••")).toBeTruthy();
  });
});
