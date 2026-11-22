import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../api/client";
import { useUi } from "../../../product/state/ui";
import { Toasts } from "../../../product/ui/Toasts";
import { saveBlob } from "../../../lib/download";
import * as transcriptApi from "./api";
import { startTranscriptExport } from "./exportAction";

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return { ...original, exportTranscript: vi.fn() };
});

vi.mock("../../../lib/download", () => ({ saveBlob: vi.fn(), saveTextFile: vi.fn() }));

const document = {
  format: "orbyn-transcript",
  version: 1,
  exported_at: "2026-11-22T10:00:00Z",
  thread: { title: "Fix login", engine: "claude", model: null, created_at: "2026-11-20T09:00:00Z" },
  messages: [],
  omitted_earlier: 0,
};

afterEach(() => {
  useUi.setState({ toasts: [] });
  vi.mocked(transcriptApi.exportTranscript).mockReset();
  vi.mocked(saveBlob).mockReset();
});

it("shows the exporting toast, saves the file under the title, and then says the download started", async () => {
  vi.mocked(transcriptApi.exportTranscript).mockResolvedValue(document);
  render(<Toasts />);
  startTranscriptExport({ id: "t1", title: 'Fix: "a/b" loop?' });
  expect(useUi.getState().toasts).toMatchObject([{ text: "Exporting transcript…", busy: true }]);
  await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Download started"]));
  expect(saveBlob).toHaveBeenCalledTimes(1);
  const [name, blob] = vi.mocked(saveBlob).mock.calls[0]!;
  expect(name).toBe("Fix a b loop.orbyn.json");
  expect(blob).toBeInstanceOf(Blob);
  expect(useUi.getState().toasts[0]?.busy).not.toBe(true);
});

it("says it could not export, and Try again repeats it with the same file name", async () => {
  const user = userEvent.setup();
  vi.mocked(transcriptApi.exportTranscript)
    .mockRejectedValueOnce(new ApiFailure("unavailable", "The service is not there.", 503))
    .mockResolvedValueOnce(document);
  render(<Toasts />);
  startTranscriptExport({ id: "t1", title: "Fix login" });
  await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Couldn’t export the transcript."]));
  expect(saveBlob).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Try again" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Download started"]));
  expect(transcriptApi.exportTranscript).toHaveBeenCalledTimes(2);
  expect(saveBlob).toHaveBeenCalledWith("Fix login.orbyn.json", expect.any(Blob));
});

it("never puts an error message in the failure toast", async () => {
  vi.mocked(transcriptApi.exportTranscript).mockRejectedValue(new ApiFailure("unavailable", "Secret message text", 503));
  render(<Toasts />);
  startTranscriptExport({ id: "t1", title: "Fix login" });
  await waitFor(() => expect(useUi.getState().toasts).toHaveLength(1));
  expect(screen.queryByText(/Secret message text/)).toBeNull();
});
