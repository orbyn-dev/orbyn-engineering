import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

describe("filter and find in the preview", () => {
  it("work on the mock's failed block", async () => {
    window.location.hash = "#/preview/command-blocks?failed";
    render(<CommandBlocksPreview />);
    const user = userEvent.setup();
    const card = await screen.findByRole("group", { name: "Command npm test" });
    await within(card).findByText(/sign-in/);
    await user.click(within(card).getByRole("button", { name: "Find in output" }));
    await user.type(within(card).getByRole("textbox", { name: "Find in output" }), "sign-in");
    expect(within(card).getByText("1 of 1")).toBeInTheDocument();
    expect(card.querySelector("mark")?.textContent).toBe("sign-in");
    await user.click(within(card).getByRole("button", { name: "Close find" }));

    await user.click(within(card).getByRole("button", { name: "Filter output" }));
    await user.type(within(card).getByRole("textbox", { name: "Filter output" }), "Tests");
    expect(within(card).getByText("1 of 6 lines")).toBeInTheDocument();
    expect(card.querySelector(".cb-out")?.textContent).toBe(" Tests  1 failed | 47 passed (48)");
  });
});
