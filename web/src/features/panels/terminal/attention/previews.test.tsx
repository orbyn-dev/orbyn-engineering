import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setAttentionApi } from "./contract";
import AttentionPreview from "./AttentionPreview";
import { attentionPreviewRoute } from "./previews";
import { useAttention } from "./store";

afterEach(() => { setAttentionApi(null); useAttention.getState().reset(); window.location.hash = ""; vi.stubGlobal("__ORBYN_PREVIEWS__", true); });

describe("the dev-only route", () => {
  it("opens for the hash route and its scenario words, and nothing else", () => {
    expect(attentionPreviewRoute("#/preview/attention")).toBe(true);
    expect(attentionPreviewRoute("#/preview/attention?many&narrow")).toBe(true);
    expect(attentionPreviewRoute("#/preview/attentionx")).toBe(false);
    expect(attentionPreviewRoute("#/preview/attention?<script>")).toBe(false);
  });
  it("is off in a production build", () => {
    vi.stubGlobal("__ORBYN_PREVIEWS__", false);
    expect(attentionPreviewRoute("#/preview/attention")).toBe(false);
  });
});

describe("the preview page", () => {
  it("shows the default item from the mock and types nothing", async () => {
    window.location.hash = "#/preview/attention";
    render(<AttentionPreview />);
    expect(await screen.findByText("Claude Code in the terminal “dev” is waiting for an answer.")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Nothing opened or written yet.");
  });
  it("shows a secret prompt with Open terminal alone", async () => {
    window.location.hash = "#/preview/attention?secret";
    render(<AttentionPreview />);
    expect(await screen.findByText("Claude Code asked for a password in the terminal “dev”.")).toBeInTheDocument();
  });
  it("shows the cleared line", async () => {
    window.location.hash = "#/preview/attention?cleared";
    render(<AttentionPreview />);
    expect(await screen.findByText(/Answered in the terminal “dev”/)).toBeInTheDocument();
  });
});
