import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../../api/client";
import type { Run, Thread } from "../../../api/types";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { Composer } from "./Composer";
import { setPhotoConversionApi } from "./photoConversion";
import { setTranscriptsApi, TRANSCRIPT_TYPE } from "./transcriptDrop";

const thread: Thread = {
  id: "t1", project_id: "p", title: "Thread", engine: "claude", model: "opus", cwd: "/r", worktree: null, skill_profile: null,
  scratch: false, active_run: null, last_run_state: null, created_at: "", updated_at: "",
};

beforeEach(() => {
  setTranscriptsApi(null);
  useComposerChips.setState({ chips: {}, reviewDrafts: {} });
  useUi.setState({ drafts: {} });
  useWorkspace.setState({ approvals: { data: [], error: null, loading: false } });
  vi.spyOn(api, "thread").mockResolvedValue(thread);
  vi.spyOn(api, "queue").mockResolvedValue([]);
});
afterEach(() => {
  setTranscriptsApi(null);
  setPhotoConversionApi(null);
  vi.restoreAllMocks();
});

const box = () => document.querySelector(".composer-box") as HTMLElement;
const held = (items: Array<{ type: string }>) => ({ types: ["Files"], items: items.map((item) => ({ kind: "file", ...item })), files: [] });

it("draws only its dashed outline, and says nothing, while a file is over the box in a build with no transcript client", () => {
  render(<Composer thread={thread} engine={null} showContext />);
  fireEvent.dragOver(box(), { dataTransfer: held([{ type: "" }]) });
  expect(box()).toHaveAttribute("data-dragging");
  expect(screen.queryByText(/Drop to add/)).toBeNull();
});

it("says what a drop will do: the transcript when the type shows one, the file when it cannot tell", () => {
  setTranscriptsApi({ read: vi.fn() });
  render(<Composer thread={thread} engine={null} showContext />);
  fireEvent.dragOver(box(), { dataTransfer: held([{ type: "" }]) });
  expect(screen.getByRole("status", { name: "" })).toHaveTextContent("Drop to add the file");
  fireEvent.dragOver(box(), { dataTransfer: held([{ type: TRANSCRIPT_TYPE }]) });
  expect(screen.getByText("Drop to add this thread’s transcript")).toBeInTheDocument();
  fireEvent.dragLeave(box());
  expect(screen.queryByText(/Drop to add/)).toBeNull();
});

it("turns a dropped transcript into a chip, sends the other files as attachments and puts the transcript id in the run", async () => {
  const read = vi.fn().mockResolvedValue({ id: "tr1", title: "Redirect loop on logout", message_count: 214 });
  setTranscriptsApi({ read });
  const upload = vi.spyOn(api, "uploadAttachment").mockResolvedValue({ id: "a1", name: "login-error.log" } as never);
  const start = vi.spyOn(api, "startRun").mockResolvedValue({ id: "r1" } as Run);
  render(<Composer thread={thread} engine={null} showContext />);
  fireEvent.drop(box(), { dataTransfer: { types: ["Files"], getData: () => "", files: [new File(["x"], "redirect-loop.orbyn.json"), new File(["x"], "login-error.log")] } });
  expect(await screen.findByText("Redirect loop on logout")).toBeInTheDocument();
  expect(screen.getByText("· 214 messages")).toBeInTheDocument();
  await waitFor(() => expect(upload).toHaveBeenCalledTimes(1));
  expect(upload.mock.calls[0]?.[1].name).toBe("login-error.log");
  await userEvent.type(screen.getByRole("textbox"), "compare{Enter}");
  await waitFor(() => expect(start).toHaveBeenCalledWith("t1", "compare", ["a1", "tr1"]));
  await waitFor(() => expect(screen.queryByText("Redirect loop on logout")).toBeNull());
});

it("holds send while a transcript is being read", async () => {
  setTranscriptsApi({ read: vi.fn().mockReturnValue(new Promise(() => {})) });
  const start = vi.spyOn(api, "startRun").mockResolvedValue({ id: "r1" } as Run);
  render(<Composer thread={thread} engine={null} showContext />);
  fireEvent.drop(box(), { dataTransfer: { types: ["Files"], getData: () => "", files: [new File(["x"], "a.orbyn.json")] } });
  await userEvent.type(screen.getByRole("textbox"), "hi{Enter}");
  expect(start).not.toHaveBeenCalled();
});

it("uploads a dropped transcript as an ordinary attachment when no client is installed", async () => {
  const upload = vi.spyOn(api, "uploadAttachment").mockResolvedValue({ id: "a1", name: "a.orbyn.json" } as never);
  render(<Composer thread={thread} engine={null} showContext />);
  fireEvent.drop(box(), { dataTransfer: { types: ["Files"], getData: () => "", files: [new File(["x"], "a.orbyn.json")] } });
  await waitFor(() => expect(upload).toHaveBeenCalledTimes(1));
});

it("keeps a HEIC photo's send held until it is converted, then sends the converted file's id", async () => {
  let finish!: (value: unknown) => void;
  setPhotoConversionApi({ convert: () => new Promise((done) => { finish = done; }) as never });
  const start = vi.spyOn(api, "startRun").mockResolvedValue({ id: "r1" } as Run);
  render(<Composer thread={thread} engine={null} showContext />);
  const heic = new File(["x"], "IMG_4021.HEIC", { type: "image/heic" });
  await userEvent.upload(screen.getByLabelText("Attach files", { selector: "input" }), heic);
  expect(await screen.findByText("Converting photo…")).toBeInTheDocument();
  await userEvent.type(screen.getByRole("textbox"), "look{Enter}");
  expect(start).not.toHaveBeenCalled();
  finish({ id: "p1", name: "IMG_4021.jpg", size: 2_000_000, original_name: "IMG_4021.HEIC", original_size: 3_000_000, action: "converted" });
  expect(await screen.findByText(/Converted to JPEG/)).toBeInTheDocument();
  await userEvent.type(screen.getByRole("textbox"), "{Enter}");
  await waitFor(() => expect(start).toHaveBeenCalledWith("t1", "look", ["p1"]));
});
