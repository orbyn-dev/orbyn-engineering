import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../api/client";
import type { EngineStatus, PreferenceView, Project, Thread } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import * as transcriptApi from "./api";
import { ImportTranscriptDialog } from "./ImportTranscriptDialog";

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, importTranscript: vi.fn(), readProjectPreferences: vi.fn() };
});

const project: Project = {
  id: "p-app",
  name: "orbyn-engineering-app",
  path: "/Users/dev/code/orbyn-engineering-app",
  trusted: true,
  is_git: true,
  managed: false,
  scratch: false,
  created_at: "2026-11-01T00:00:00Z",
};

const engine = (id: string, name: string, over: Partial<EngineStatus> = {}): EngineStatus =>
  ({ id, name, installed: true, auth: "logged_in", capabilities: {}, ...over }) as unknown as EngineStatus;

const installEngines = () => {
  useWorkspace.setState({
    engines: {
      data: [engine("claude", "Claude Code"), engine("codex", "Codex"), engine("kimi", "Kimi Code")],
      error: null,
      loading: false,
    },
  });
};

const preferences = (defaultEngine: string, disabled: string[] = []): PreferenceView[] => [
  { key: "default_engine", value: defaultEngine, default: "", source: "default", project_scoped: true },
  { key: "disabled_engines", value: disabled, default: [], source: "default", project_scoped: true },
] as unknown as PreferenceView[];

const transcript = (over: Record<string, unknown> = {}) =>
  JSON.stringify({
    format: "orbyn-transcript",
    version: 1,
    exported_at: "2026-11-22T10:00:00Z",
    thread: { title: "Redirect loop on logout", engine: "codex", model: "opus", created_at: "2026-11-20T09:00:00Z" },
    messages: [{ role: "user", ts: "2026-11-20T09:00:05Z", text: "SHOULD NOT SHOW" }],
    omitted_earlier: 0,
    ...over,
  });

const file = (name: string, body = transcript()) =>
  new File([body], name, { type: "application/vnd.orbyn.transcript+json" });

const answer = (over: Record<string, unknown> = {}) => ({
  dry_run: true,
  thread: null,
  title: "Redirect loop on logout",
  message_count: 214,
  dropped: 0,
  problems: [],
  ...over,
});

const importedThread: Thread = {
  id: "t-new",
  project_id: "p-app",
  title: "Redirect loop on logout",
  engine: "claude",
  model: null,
  cwd: project.path,
  worktree: null,
  skill_profile: null,
  active_run: null,
  last_run_state: null,
  created_at: "2026-11-22T10:00:00Z",
  updated_at: "2026-11-22T10:00:00Z",
  scratch: false,
  source: "orbyn-transcript",
  imported_at: "2026-11-22T10:00:00Z",
};

beforeEach(() => {
  installEngines();
  vi.mocked(transcriptApi.readProjectPreferences).mockResolvedValue(preferences("claude"));
  vi.mocked(transcriptApi.importTranscript).mockReset();
});

afterEach(() => {
  useWorkspace.setState(useWorkspace.getInitialState(), true);
});

const open = (body = file("Redirect loop on logout.orbyn.json"), props: Partial<Parameters<typeof ImportTranscriptDialog>[0]> = {}) => {
  const onClose = vi.fn();
  const onChooseAnother = vi.fn();
  const view = render(
    <ImportTranscriptDialog project={project} file={body} onClose={onClose} onChooseAnother={onChooseAnother} {...props} />,
  );
  return { ...view, onClose, onChooseAnother };
};

it("never reads a file over the cap into memory; the dry run refuses it", async () => {
  vi.mocked(transcriptApi.importTranscript).mockReturnValue(new Promise(() => undefined));
  const big = file("big.orbyn.json");
  Object.defineProperty(big, "size", { value: transcriptApi.MAX_IMPORT_BYTES + 1 });
  const read = vi.spyOn(big, "text");
  open(big);
  await screen.findByText("Reading file…");
  expect(read).not.toHaveBeenCalled();
});

it("reads the file with the project default engine and says so while the dry run is out", async () => {
  vi.mocked(transcriptApi.importTranscript).mockReturnValue(new Promise(() => undefined));
  open();
  expect(await screen.findByText("Reading file…")).toBeVisible();
  expect(screen.getByText("Redirect loop on logout.orbyn.json")).toBeVisible();
  expect(screen.getByRole("button", { name: "Import as new thread" })).toBeDisabled();
  await waitFor(() =>
    expect(transcriptApi.importTranscript).toHaveBeenCalledWith(
      expect.objectContaining({ projectId: "p-app", engine: "claude", dryRun: true }),
      expect.anything(),
    ),
  );
});

it("shows the preview, the file label and the project default note", async () => {
  vi.mocked(transcriptApi.importTranscript).mockResolvedValue(answer() as never);
  open();
  expect(await screen.findByText("Redirect loop on logout")).toBeVisible();
  expect(screen.getByText("Messages")).toBeVisible();
  expect(screen.getByText("214")).toBeVisible();
  expect(screen.getByRole("combobox", { name: "Engine" })).toHaveTextContent("Claude Code");
  expect(screen.getByText("Project default. The file was made with Codex.")).toBeVisible();
  expect(screen.queryByText("SHOULD NOT SHOW")).toBeNull();
});

it("reports dropped messages by place and code, never their text", async () => {
  vi.mocked(transcriptApi.importTranscript).mockResolvedValue(
    answer({
      dropped: 3,
      message_count: 211,
      problems: [
        { kind: "message", index: 7, code: "unknown_role" },
        { kind: "message", index: 31, code: "text_empty" },
        { kind: "message", index: 96, code: "not_an_object" },
        { kind: "title", index: null, code: "title_missing" },
      ],
    }) as never,
  );
  open();
  expect(await screen.findByText("3 dropped")).toBeVisible();
  expect(screen.getByText("Message 8")).toBeVisible();
  expect(screen.getByText("unknown_role")).toBeVisible();
  expect(screen.getByText("Message 32")).toBeVisible();
  expect(screen.getByText("Message 97")).toBeVisible();
  expect(screen.queryByText("title_missing")).toBeNull();
  expect(screen.queryByText("SHOULD NOT SHOW")).toBeNull();
  expect(screen.getByRole("button", { name: "Import as new thread" })).toBeEnabled();
});

it("imports with the chosen engine, closes, adds the thread and opens it", async () => {
  const user = userEvent.setup();
  vi.mocked(transcriptApi.importTranscript)
    .mockResolvedValueOnce(answer() as never)
    .mockResolvedValueOnce({ ...answer(), dry_run: false, thread: importedThread } as never);
  window.location.hash = "#/threads/t-done";
  const { onClose } = open();
  await user.click(await screen.findByRole("button", { name: "Import as new thread" }));
  await waitFor(() => expect(onClose).toHaveBeenCalled());
  expect(useWorkspace.getState().threads["t-new"]).toEqual(importedThread);
  expect(window.location.hash).toBe("#/threads/t-new");
  expect(transcriptApi.importTranscript).toHaveBeenLastCalledWith(
    expect.objectContaining({ projectId: "p-app", engine: "claude", dryRun: false }),
  );
});

it("shows Importing… with everything disabled until the answer", async () => {
  const user = userEvent.setup();
  vi.mocked(transcriptApi.importTranscript)
    .mockResolvedValueOnce(answer() as never)
    .mockReturnValueOnce(new Promise(() => undefined));
  open();
  await user.click(await screen.findByRole("button", { name: "Import as new thread" }));
  expect(await screen.findByRole("button", { name: /Importing/ })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
  expect(screen.getByRole("combobox", { name: "Engine" })).toBeDisabled();
  fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
  expect(screen.getByRole("dialog")).toBeInTheDocument();
});

it("switches to the chosen engine and sends it on import", async () => {
  const user = userEvent.setup();
  vi.mocked(transcriptApi.importTranscript)
    .mockResolvedValueOnce(answer() as never)
    .mockResolvedValueOnce({ ...answer(), dry_run: false, thread: importedThread } as never);
  open();
  await user.click(await screen.findByRole("combobox", { name: "Engine" }));
  await user.click(screen.getByRole("option", { name: /Codex/ }));
  expect(screen.getByRole("combobox", { name: "Engine" })).toHaveTextContent("Codex");
  await user.click(screen.getByRole("button", { name: "Import as new thread" }));
  await waitFor(() =>
    expect(transcriptApi.importTranscript).toHaveBeenLastCalledWith(expect.objectContaining({ engine: "codex", dryRun: false })),
  );
});

it("says the file names no engine", async () => {
  vi.mocked(transcriptApi.importTranscript).mockResolvedValue(answer() as never);
  open(file("Redirect loop on logout.orbyn.json", transcript({ thread: { title: "Redirect loop on logout" } })));
  expect(await screen.findByText("Project default. The file names no engine.")).toBeVisible();
});

it("lists only the installed engines the project allows", async () => {
  const user = userEvent.setup();
  vi.mocked(transcriptApi.readProjectPreferences).mockResolvedValue(preferences("claude", ["codex"]));
  vi.mocked(transcriptApi.importTranscript).mockResolvedValue(answer() as never);
  open();
  await user.click(await screen.findByRole("combobox", { name: "Engine" }));
  const names = screen.getAllByRole("option").map((option) => option.textContent);
  expect(names.join(" ")).toContain("Claude Code");
  expect(names.join(" ")).toContain("Kimi Code");
  expect(names.join(" ")).not.toContain("Codex");
});

it("reads Choose an engine and keeps Import disabled without a project default", async () => {
  const user = userEvent.setup();
  vi.mocked(transcriptApi.readProjectPreferences).mockResolvedValue(preferences(""));
  vi.mocked(transcriptApi.importTranscript).mockResolvedValue(answer() as never);
  open();
  const select = await screen.findByRole("combobox", { name: "Engine" });
  expect(select).toHaveTextContent("Choose an engine");
  expect(screen.getByRole("button", { name: "Import as new thread" })).toBeDisabled();
  await user.click(select);
  await user.click(screen.getByRole("option", { name: /Codex/ }));
  expect(screen.getByRole("button", { name: "Import as new thread" })).toBeEnabled();
});

const refusals: Array<[string, ApiFailure, string, boolean]> = [
  ["too large", new ApiFailure("file_too_large", "Too big.", 413), "A transcript is at most 2 MiB.", true],
  ["too new", new ApiFailure("transcript_too_new", "Newer.", 422), "This transcript was made by a newer Orbyn.", true],
  ["unreadable", new ApiFailure("transcript_unreadable", "Bad file.", 422), "This file is not an Orbyn transcript.", true],
  ["desktop", new ApiFailure("person_required", "No key.", 403), "This action is only available in the desktop app.", false],
  ["no engine", new ApiFailure("engine_unavailable", "Unknown engine.", 404), "That engine is not available here. Choose another.", true],
];

describe.each(refusals)("the %s refusal", (_name, failure, sentence, canChoose) => {
  it("replaces the preview with the sentence and offers another file when the design does", async () => {
    const user = userEvent.setup();
    vi.mocked(transcriptApi.importTranscript).mockRejectedValue(failure);
    const { onChooseAnother } = open();
    expect(await screen.findByText(sentence)).toBeVisible();
    expect(screen.queryByText("214")).toBeNull();
    if (canChoose) {
      await user.click(screen.getByRole("button", { name: "Choose another file…" }));
      expect(onChooseAnother).toHaveBeenCalledTimes(1);
    } else {
      expect(screen.queryByRole("button", { name: "Choose another file…" })).toBeNull();
    }
    await user.click(screen.getByRole("button", { name: "Close" }));
  });
});

it("renders a title from the file as text, never as markup", async () => {
  const user = userEvent.setup();
  vi.mocked(transcriptApi.importTranscript).mockResolvedValue(
    answer({ title: '<img src=x onerror="alert(1)">' }) as never,
  );
  const { container } = open(file("hostile.orbyn.json", transcript({ thread: { title: "<img src=x onerror=alert(1)>" } })));
  expect(await screen.findByText('<img src=x onerror="alert(1)">')).toBeVisible();
  expect(container.querySelector("img")).toBeNull();
  expect(screen.queryByText("SHOULD NOT SHOW")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Cancel" }));
});

it("traps focus, closes on Escape, and returns focus to the trigger", async () => {
  vi.mocked(transcriptApi.importTranscript).mockResolvedValue(answer() as never);
  function Harness() {
    const [shown, setShown] = useState(false);
    return (
      <>
        <button type="button" onClick={() => setShown(true)}>
          Open import
        </button>
        {shown ? (
          <ImportTranscriptDialog
            project={project}
            file={file("Redirect loop on logout.orbyn.json")}
            onClose={() => setShown(false)}
            onChooseAnother={() => undefined}
          />
        ) : null}
      </>
    );
  }
  const user = userEvent.setup();
  render(<Harness />);
  const trigger = screen.getByRole("button", { name: "Open import" });
  Object.defineProperty(trigger, "offsetParent", { configurable: true, get: () => document.body });
  await user.click(trigger);
  expect(await screen.findByRole("dialog", { name: "Import transcript" })).toBeVisible();
  fireEvent(screen.getByRole("dialog"), new Event("cancel", { cancelable: true }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(trigger).toHaveFocus();
});
