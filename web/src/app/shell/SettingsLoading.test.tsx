import { fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsLoading } from "./Shell";

afterEach(() => {
  document.querySelectorAll("dialog").forEach((dialog) => dialog.remove());
  vi.restoreAllMocks();
});

describe("the Settings chunk's loading state", () => {
  it("closes Settings on Escape, because nothing of the page that handles the key is up yet", () => {
    const onClose = vi.fn();
    render(<SettingsLoading onClose={onClose} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("leaves Escape to a dialog that owns it", () => {
    const dialog = document.createElement("dialog");
    dialog.setAttribute("open", "");
    document.body.append(dialog);
    const onClose = vi.fn();
    render(<SettingsLoading onClose={onClose} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
  });

  it("ignores every other key", () => {
    const onClose = vi.fn();
    render(<SettingsLoading onClose={onClose} />);
    fireEvent.keyDown(document, { key: "Enter" });
    expect(onClose).not.toHaveBeenCalled();
  });
});
