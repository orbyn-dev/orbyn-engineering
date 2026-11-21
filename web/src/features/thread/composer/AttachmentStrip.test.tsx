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
