import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { Project } from "../../../api/types";
import { useTranscriptImport } from "../../../product/state/transcriptImport";
import { FolderHeader } from "./listRows";

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

const transfer = (file: File, type = "application/json") => ({
  dataTransfer: {
    files: [file],
    types: ["Files"],
    items: [{ kind: "file", type, getAsFile: () => file }],
    dropEffect: "copy",
  },
});

const transcript = () =>
  new File([JSON.stringify({ format: "orbyn-transcript", version: 1, thread: {}, messages: [] })], "Redirect loop on logout.orbyn.json", {
    type: "application/vnd.orbyn.transcript+json",
  });

const renderFolder = () =>
  render(<FolderHeader project={project} label={project.name} collapsed={false} count={0} needs={0} onToggle={() => undefined} onMenu={() => undefined} />);

afterEach(() => {
  useTranscriptImport.setState({ request: null });
});

it("outlines a project row over a transcript and names what dropping does", async () => {
  const { container } = renderFolder();
  const row = container.querySelector(".folder-head")!;
  fireEvent.dragEnter(row, transfer(transcript()));
  fireEvent.dragOver(row, transfer(transcript()));
  await waitFor(() => expect(row).toHaveAttribute("data-file-over"));
  expect(screen.getByText("Drop to import into orbyn-engineering-app")).toBeVisible();
  expect(screen.getByText("Redirect loop on logout.orbyn.json")).toBeVisible();
});

it("opens the import sheet from a drop on the project row", async () => {
  const { container } = renderFolder();
  const row = container.querySelector(".folder-head")!;
  const file = transcript();
  fireEvent.drop(row, transfer(file));
  await waitFor(() =>
    expect(useTranscriptImport.getState().request).toMatchObject({ project: { id: "p-app" }, file: { name: file.name } }),
  );
});

it("accepts an unknown file type and says a transcript may be dropped", async () => {
  const { container } = renderFolder();
  const row = container.querySelector(".folder-head")!;
  fireEvent.dragEnter(row, transfer(new File(["x"], "mystery.bin", { type: "" }), ""));
  expect(await screen.findByText("Drop to import a transcript")).toBeVisible();
});

it("does nothing for a known non-transcript type", async () => {
  const { container } = renderFolder();
  const row = container.querySelector(".folder-head")!;
  const image = new File(["x"], "photo.png", { type: "image/png" });
  fireEvent.dragEnter(row, transfer(image, "image/png"));
  expect(row).not.toHaveAttribute("data-file-over");
  expect(screen.queryByText(/Drop to import/)).toBeNull();
  fireEvent.drop(row, transfer(image, "image/png"));
  expect(useTranscriptImport.getState().request).toBeNull();
});

it("does nothing when a transcript is dropped anywhere else", () => {
  const elsewhere = document.createElement("div");
  document.body.append(elsewhere);
  fireEvent.drop(elsewhere, transfer(transcript()));
  expect(useTranscriptImport.getState().request).toBeNull();
});
