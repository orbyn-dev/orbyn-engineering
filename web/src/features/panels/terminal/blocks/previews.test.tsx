import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setBlocksApi } from "./blocksContract";
import CommandBlocksPreview from "./CommandBlocksPreview";
import { commandBlocksPreviewRoute } from "./previews";

afterEach(() => {
  setBlocksApi(null);
  window.location.hash = "";
  vi.stubGlobal("__ORBYN_PREVIEWS__", true);
});

describe("the dev-only preview route", () => {
  it("opens for the hash route and its scenario words, and nothing else", () => {
    expect(commandBlocksPreviewRoute("#/preview/command-blocks")).toBe(true);
    expect(commandBlocksPreviewRoute("#/preview/command-blocks?failed&narrow")).toBe(true);
    expect(commandBlocksPreviewRoute("#/preview/command-blocksx")).toBe(false);
    expect(commandBlocksPreviewRoute("#/preview/command-blocks?<script>")).toBe(false);
    expect(commandBlocksPreviewRoute("#/threads")).toBe(false);
  });

  it("is off in a production build", () => {
    vi.stubGlobal("__ORBYN_PREVIEWS__", false);
    expect(commandBlocksPreviewRoute("#/preview/command-blocks")).toBe(false);
  });
});

describe("the preview page", () => {
  it("draws the mock's blocks as plain text, hostile output included, and writes nothing on its own", async () => {
    window.location.hash = "#/preview/command-blocks";
    const { container } = render(<CommandBlocksPreview />);
    expect(await screen.findByRole("group", { name: "Command npm test" })).toBeInTheDocument();
    expect(await screen.findByText(/onerror=alert\(1\)>\s<script>/)).toBeInTheDocument();
    expect(container.querySelector("img, script")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("Nothing written to the terminal yet.");
  });

  it("shows the empty and the failed states by scenario", async () => {
    window.location.hash = "#/preview/command-blocks?empty";
    render(<CommandBlocksPreview />);
    expect(await screen.findByText("Commands you run here appear as blocks")).toBeInTheDocument();
  });
});
