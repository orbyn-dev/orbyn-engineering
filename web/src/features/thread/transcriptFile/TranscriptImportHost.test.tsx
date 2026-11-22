import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { Project } from "../../../api/types";
import { useTranscriptImport, pickTranscript } from "../../../product/state/transcriptImport";
import { TranscriptImportHost } from "./TranscriptImportHost";
import * as transcriptApi from "./api";

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

const file = () =>
  new File([JSON.stringify({ format: "orbyn-transcript", version: 1, thread: { title: "Redirect loop on logout" }, messages: [] })], "Redirect loop on logout.orbyn.json", {
    type: "application/vnd.orbyn.transcript+json",
  });

afterEach(() => {
  useTranscriptImport.setState({ request: null });
});

it("opens the picker for a project and shows the sheet for the file it returns", async () => {
  vi.mocked(transcriptApi.readProjectPreferences).mockResolvedValue([]);
  vi.mocked(transcriptApi.importTranscript).mockReturnValue(new Promise(() => undefined));
  const { container } = render(<TranscriptImportHost />);
  const input = container.querySelector<HTMLInputElement>("input[data-transcript-file]")!;
  expect(input).toHaveAttribute("accept", ".orbyn.json,application/json");
  expect(input).toHaveAttribute("hidden");
  act(() => pickTranscript(project));
  fireEvent.change(input, { target: { files: [file()] } });
  expect(await screen.findByRole("dialog", { name: "Import transcript" })).toBeVisible();
  expect(screen.getByText("Redirect loop on logout.orbyn.json")).toBeVisible();
  await waitFor(() => expect(useTranscriptImport.getState().request).not.toBeNull());
});
