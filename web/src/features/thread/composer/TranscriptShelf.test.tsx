import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { TranscriptShelf } from "./TranscriptShelf";
import type { TranscriptEntry } from "./useTranscripts";

const file = (name: string) => new File(["x"], name);
const ready = (title: string, messageCount: number): TranscriptEntry => ({ key: title, file: file("a.orbyn.json"), state: "ready", id: "i", title, messageCount });

it("says a kept transcript rides on the next message only", () => {
  render(<TranscriptShelf entries={[ready("Tiny", 1)]} remove={vi.fn()} addAsFile={vi.fn()} />);
  expect(screen.getByText("Attached to your next message only.")).toBeTruthy();
});

it("draws nothing for an empty shelf", () => {
  const { container } = render(<TranscriptShelf entries={[]} remove={vi.fn()} addAsFile={vi.fn()} />);
  expect(container).toBeEmptyDOMElement();
});

it("draws a transcript as a thread-style chip with its title, how many messages and a remove, and the full title as its tooltip", async () => {
  const remove = vi.fn();
  const long = "Settings rows jump when a scope changes, second attempt";
  render(<TranscriptShelf entries={[ready("Redirect loop on logout", 214), ready(long, 1204)]} remove={remove} addAsFile={vi.fn()} />);
  const items = screen.getAllByRole("listitem");
  expect(items.map((item) => item.dataset.kind)).toEqual(["transcript", "transcript"]);
  expect(screen.getByText("· 214 messages")).toBeInTheDocument();
  expect(screen.getByText("· 1,204 messages")).toBeInTheDocument();
  expect(screen.getByTitle(long)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: `Remove ${long}` }));
  expect(remove).toHaveBeenCalledWith(long);
});

it("says one message in the singular", () => {
  render(<TranscriptShelf entries={[ready("Tiny", 1)]} remove={vi.fn()} addAsFile={vi.fn()} />);
  expect(screen.getByText("· 1 message")).toBeInTheDocument();
});

it("shows a refused transcript's reason in a red chip and offers the plain file only when it can", async () => {
  const addAsFile = vi.fn();
  const refused = (name: string, reason: string, canAddAsFile: boolean): TranscriptEntry => ({ key: name, file: file(name), state: "refused", reason, canAddAsFile });
  render(<TranscriptShelf entries={[refused("a.orbyn.json", "Made by a newer Orbyn. Update Orbyn to read it.", false), refused("b.orbyn.json", "This isn’t a transcript Orbyn can read.", true)]}
    remove={vi.fn()} addAsFile={addAsFile} />);
  expect(screen.getAllByRole("listitem").every((item) => item.hasAttribute("data-failed"))).toBe(true);
  expect(screen.getByText("Made by a newer Orbyn. Update Orbyn to read it.")).toBeInTheDocument();
  expect(screen.getAllByRole("button", { name: "Add as a file" })).toHaveLength(1);
  await userEvent.click(screen.getByRole("button", { name: "Add as a file" }));
  expect(addAsFile).toHaveBeenCalledWith(expect.objectContaining({ key: "b.orbyn.json" }));
});

it("shows a transcript being read with a spinner and a cancel", () => {
  const remove = vi.fn();
  render(<TranscriptShelf entries={[{ key: "k", file: file("a.orbyn.json"), state: "reading" }]} remove={remove} addAsFile={vi.fn()} />);
  expect(screen.getByText("Reading transcript…")).toBeInTheDocument();
  screen.getByRole("button", { name: "Cancel reading a.orbyn.json" }).click();
  expect(remove).toHaveBeenCalledWith("k");
});
