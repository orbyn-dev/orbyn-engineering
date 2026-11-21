import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../../api/client";
import type { Run, SlashCommand, Thread } from "../../../api/types";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { LabsSection } from "../../settings/sections/LabsSection";
import { SETTINGS } from "../../settings/settingsSearch";
import { Composer } from "./Composer";
import { controlsMenuItems } from "./controlsMenu";
import { loadSuggestions } from "./suggestions";

// Plan mode is removed, as in T3 (parity B66): every thread builds. The service still has a
// plan_mode field, and these tests hold that the web offers no way to set it.
const thread: Thread = {
  id: "t1", project_id: "p", title: "Thread", engine: "claude", model: "opus", cwd: "/r", worktree: null,
  skill_profile: null, scratch: false, active_run: null, last_run_state: null, created_at: "", updated_at: "",
  lifecycle: { pinned_at: null, archived_at: null, settled_at: null, snoozed_until: null, unread: false, auto_settle: null,
    position: 0, parent_thread_id: null, relation: null, permission_mode: "supervised", plan_mode: false },
};
const run = { id: "r1" } as Run;
const box = () => screen.getByRole("textbox");

beforeEach(() => {
  useComposerChips.setState({ chips: {}, reviewDrafts: {} });
  useUi.setState({ drafts: {} });
  useWorkspace.setState({ approvals: { data: [], error: null, loading: false } });
  vi.spyOn(api, "thread").mockResolvedValue(thread);
  vi.spyOn(api, "queue").mockResolvedValue([]);
});
afterEach(() => vi.restoreAllMocks());

const view = () => render(<Composer thread={thread} engine={null} showContext />);

it("has no Plan button in the composer bar", () => {
  view();
  expect(screen.queryByRole("button", { name: "Plan" })).toBeNull();
});

it("does not switch anything on Shift+Tab in the field", async () => {
  const patch = vi.spyOn(api, "patchThread").mockResolvedValue(thread);
  view();
  box().focus();
  await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
  expect(patch).not.toHaveBeenCalled();
});

it.each(["/plan", "/default"])("sends %s to the engine as ordinary text and never patches the thread", async (command) => {
  const start = vi.spyOn(api, "startRun").mockResolvedValue(run);
  const patch = vi.spyOn(api, "patchThread").mockResolvedValue(thread);
  view();
  await userEvent.type(box(), `${command}{Enter}`);
  await waitFor(() => expect(start).toHaveBeenCalledExactlyOnceWith("t1", command, []));
  expect(patch).not.toHaveBeenCalled();
});

it("does not offer /plan or /default in the slash menu, even when the service lists them", async () => {
  const listed: SlashCommand[] = [
    { name: "plan", description: "Plan without changing anything", source: "orbyn" },
    { name: "default", description: "Leave plan mode", source: "orbyn" },
    { name: "review", description: "Review the change", source: "engine" },
  ];
  vi.spyOn(api, "commands").mockResolvedValue(listed);
  const found = await loadSuggestions({ char: "/", query: "", start: 0 }, thread, false, new AbortController().signal);
  const names = found.map((entry) => entry.value);
  expect(names).toContain("review");
  expect(names).toContain("usage-limits");
  expect(names).not.toContain("plan");
  expect(names).not.toContain("default");
});

it("folds no Plan entry into the narrow controls menu", () => {
  const items = controlsMenuItems({
    thread, model: null, engineName: "Claude", effortShown: false, effortLevel: null, busy: false, unmanaged: false,
    access: "supervised", traits: { openAt: () => {} },
  });
  expect(items.map((item) => item.label)).not.toContain("Plan");
});

it("has no Plan mode switch in Labs or in settings search, and no saved plan preference", () => {
  render(<LabsSection />);
  expect(screen.queryByRole("switch", { name: "Plan mode" })).toBeNull();
  expect(screen.queryByText(/Shift\+Tab/)).toBeNull();
  expect(SETTINGS.map((item) => item.title)).not.toContain("Plan mode");
  expect(useUi.getState()).not.toHaveProperty("planControls");
});
