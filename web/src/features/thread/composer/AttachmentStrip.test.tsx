import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { AttachmentStrip, type ShownAttachment } from "./AttachmentStrip";

const entry = (name: string, preview: string | null): ShownAttachment => ({
  key: name, file: new File(["x"], name), name, preview, id: "a", failure: null,
});

it("marks a photo chip so it is drawn 40 px high around its thumbnail, and leaves a file chip alone", () => {
  render(<AttachmentStrip attachments={[entry("shot.png", "blob:shot"), entry("notes.txt", null)]} setAttachments={vi.fn()} startUpload={vi.fn()} />);
  const [photo, file] = screen.getAllByRole("listitem");
  expect(photo).toHaveAttribute("data-photo");
  expect(photo?.querySelector("img.attachment-thumb")).not.toBeNull();
  expect(file).not.toHaveAttribute("data-photo");
});

const converting = (over: Partial<ShownAttachment>): ShownAttachment => ({
  key: "k", file: new File(["x"], "IMG_4021.HEIC"), name: "IMG_4021.HEIC", preview: null, id: null, failure: null,
  conversion: { kind: "convert", phase: "converting", note: "Converting photo…" }, ...over,
});

it("draws a photo being converted as a 40 px chip with a spinner, one line and a cancel that is not a remove", () => {
  const cancel = vi.fn();
  render(<AttachmentStrip attachments={[converting({})]} setAttachments={vi.fn()} startUpload={vi.fn()} cancel={cancel} />);
  const chip = screen.getByRole("listitem");
  expect(chip).toHaveAttribute("data-photo");
  expect(chip).toHaveAttribute("data-conversion", "converting");
  expect(screen.getByText("Converting photo…")).toBeInTheDocument();
  expect(screen.getByLabelText("Working")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /^Remove/ })).toBeNull();
  screen.getByRole("button", { name: "Cancel converting IMG_4021.HEIC" }).click();
  expect(cancel).toHaveBeenCalledWith(expect.objectContaining({ key: "k" }));
});

it("says shrinking, not converting, for an oversized photo", () => {
  render(<AttachmentStrip attachments={[converting({ name: "IMG_4022.PNG", conversion: { kind: "shrink", phase: "converting", note: "Shrinking photo · 14 MB…" } })]}
    setAttachments={vi.fn()} startUpload={vi.fn()} cancel={vi.fn()} />);
  expect(screen.getByRole("button", { name: "Cancel shrinking IMG_4022.PNG" })).toBeInTheDocument();
});

it("shows what was done to a finished photo and offers a remove, never the original", () => {
  render(<AttachmentStrip attachments={[converting({ id: "a", name: "IMG_4021.jpg", conversion: { kind: "convert", phase: "converted", note: "Converted to JPEG · 2.1 MB" } })]}
    setAttachments={vi.fn()} startUpload={vi.fn()} cancel={vi.fn()} />);
  expect(screen.getByText("Converted to JPEG · 2.1 MB")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Remove IMG_4021.jpg" })).toBeInTheDocument();
  expect(screen.queryByLabelText("Working")).toBeNull();
});

it("turns a failed photo red with its reason, a retry and a remove", () => {
  const retry = vi.fn();
  render(<AttachmentStrip attachments={[converting({ failure: "Couldn’t convert this photo. It may be damaged.", conversion: { kind: "convert", phase: "failed", note: "" } })]}
    setAttachments={vi.fn()} startUpload={retry} cancel={vi.fn()} />);
  expect(screen.getByRole("listitem")).toHaveAttribute("data-conversion", "failed");
  expect(screen.getByText("Couldn’t convert this photo. It may be damaged.")).toBeInTheDocument();
  screen.getByRole("button", { name: "Retry IMG_4021.HEIC" }).click();
  expect(retry).toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Remove IMG_4021.HEIC" })).toBeInTheDocument();
});
