import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { HistoryPop } from "./HistoryPop";
import type { HistoryEntry } from "./history";

const entry = (command: string, at = 1): HistoryEntry => ({ command, ok: null, at });

describe("HistoryPop rows and their result", () => {
  it("marks a command the shell said succeeded or failed, and says nothing for one it never reported", () => {
    const now = Date.now();
    const rows: HistoryEntry[] = [
      { command: "npm test", ok: false, at: now - 4 * 60_000 },
      { command: "npm run build", ok: true, at: now - 2 * 3_600_000 },
      { command: "ls", ok: null, at: now - 3 * 86_400_000 },
    ];
    render(<HistoryPop shell={rows} project={[]} scope="shell" onPick={() => undefined} onClose={() => undefined} />);
    // The check and the cross carry the result; a failed row also says so in words.
    expect(screen.getByLabelText("Failed")).toBeTruthy();
    expect(screen.getByLabelText("Succeeded")).toBeTruthy();
    expect(screen.getAllByLabelText(/Failed|Succeeded/)).toHaveLength(2);
    expect(screen.getByText("4 min ago · 1 failed")).toBeTruthy();
    expect(screen.getByText("2 hr ago")).toBeTruthy();
    expect(screen.getByText("3 d ago")).toBeTruthy();
  });
});

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

describe("HistoryPop with a secret-named assignment", () => {
  it("masks a literal fallback in the value", async () => {
    render(<HistoryPop shell={[entry("export API_TOKEN=${TOKEN:-hunter2}")]} project={[]} scope="shell" onPick={() => undefined} onClose={() => undefined} />);
    expect(await screen.findByText("export API_TOKEN=••••••")).toBeTruthy();
  });
});
