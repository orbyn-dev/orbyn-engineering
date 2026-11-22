import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Thread } from "../../api/types";
import type { SentAttachment } from "../../product/timeline/types";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { TimelineThread } from "./timelineThread";
import { SentFiles, sentPaths } from "./SentFiles";

const thread: Thread = { id: "t", project_id: "p", title: "Thread", engine: "codex", model: null,
  cwd: "/repo", worktree: null, skill_profile: null, scratch: false, active_run: null,
  last_run_state: null, created_at: "", updated_at: "" };

const answer = (body: Blob) => vi.fn(async () => ({ ok: true, status: 200, blob: async () => body }));
const view = (text: string, attachments: readonly SentAttachment[] = [{ id: "a1", name: "notes.txt", mime: "text/plain", size: 5 }]) =>
  render(
    <TimelineThread value="t">
      <SentFiles text={text} attachments={attachments} />
    </TimelineThread>,
  );

beforeEach(() => useWorkspace.setState({ threads: { t: thread } }));
afterEach(() => vi.restoreAllMocks());

it("reads typed mentions as paths, not email addresses", () => {
  expect(sentPaths("look at @src/lib/a.ts and @README.md, then a@b.com and @plain")).toEqual([
    "src/lib/a.ts",
    "README.md",
  ]);
  expect(sentPaths("@src/lib/a.ts again @src/lib/a.ts")).toEqual(["src/lib/a.ts"]);
});

it("shows what the message carried and reveals a typed path", async () => {
  const user = userEvent.setup();
  view("see @src/lib/a.ts");
  expect(screen.getByRole("button", { name: "Open notes.txt" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Show src/lib/a.ts" }));
  expect(useUi.getState().fileReveal).toMatchObject({ path: "src/lib/a.ts", line: null, threadId: "t", cwd: "/repo" });
});

it("previews a text attachment from the service", async () => {
  const user = userEvent.setup();
  const fetchMock = answer(new Blob(["hello"], { type: "text/plain" }));
  vi.stubGlobal("fetch", fetchMock);
  view("read it");
  await user.click(screen.getByRole("button", { name: "Open notes.txt" }));
  expect(await screen.findByText("hello")).toBeInTheDocument();
  expect(fetchMock).toHaveBeenCalledWith("/api/attachments/a1/content", expect.objectContaining({ credentials: "same-origin" }));
});

const png = () => answer(new Blob([new Uint8Array([1])], { type: "image/png" }));
const shots = [
  { id: "a2", name: "Phone screen", mime: "image/png", size: 1 },
  { id: "a3", name: "Network list", mime: "image/png", size: 1 },
  { id: "a4", name: "login-error.log", mime: "text/plain", size: 9 },
];

it("draws a sent image as a thumbnail in its message's gallery and releases the object URL", async () => {
  const user = userEvent.setup();
  const revoke = vi.fn();
  let n = 0;
  URL.createObjectURL = vi.fn(() => `blob:shot${++n}`);
  URL.revokeObjectURL = revoke;
  vi.stubGlobal("fetch", png());
  const mounted = view("look", shots.slice(0, 1));
  const thumb = await screen.findByRole("button", { name: "Expand Phone screen" });
  expect(within(thumb).getByRole("img", { name: "Phone screen" })).toHaveAttribute("src", "blob:shot1");
  await user.click(thumb);
  expect(await screen.findByRole("dialog", { name: "Phone screen" })).toBeInTheDocument();
  expect(screen.queryByText("1 of 1")).toBeNull();
  mounted.unmount();
  await waitFor(() => expect(revoke).toHaveBeenCalledWith("blob:shot1"));
});

it("says where a sent Snap Shot came from, as text", async () => {
  URL.createObjectURL = vi.fn(() => "blob:snap");
  URL.revokeObjectURL = vi.fn();
  vi.stubGlobal("fetch", png());
  view("look", [{
    id: "a9",
    name: "Snap Shot · Safari.png",
    mime: "image/png",
    size: 1,
    source: { kind: "snap_shot", captured_at: "2026-11-22T14:32:08Z", app_name: "Safari", window_title: "Acme · Sign in" },
  }]);
  expect(await screen.findByText("Snap Shot · Safari · Acme · Sign in")).toBeVisible();
});

it("pages through the message's images and leaves a file that is not a picture as a chip", async () => {
  const user = userEvent.setup();
  let n = 0;
  URL.createObjectURL = vi.fn(() => `blob:shot${++n}`);
  URL.revokeObjectURL = vi.fn();
  vi.stubGlobal("fetch", png());
  view("look", shots);
  expect(screen.getByRole("button", { name: "Open login-error.log" })).toBeInTheDocument();
  await user.click(await screen.findByRole("button", { name: "Expand Network list" }));
  expect(await screen.findByText("2 of 2")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Next" }));
  expect(screen.getByText("1 of 2")).toBeInTheDocument();
});

it("says so in the thumbnail when an image cannot be loaded", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, status: 404, json: async () => ({ code: "not_found", message: "Gone." }) })));
  view("look", shots.slice(0, 1));
  expect(await screen.findByText("Image unavailable · Phone screen")).toBeInTheDocument();
});

it("says why a chip could not be opened", async () => {
  const user = userEvent.setup();
  vi.stubGlobal("fetch", vi.fn(async () => ({
    ok: false,
    status: 404,
    json: async () => ({ code: "not_found", message: "That attachment is gone." }),
  })));
  view("read it");
  await user.click(screen.getByRole("button", { name: "Open notes.txt" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("That attachment is gone.");
});
