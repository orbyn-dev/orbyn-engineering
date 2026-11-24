import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { useUi } from "../../../product/state/ui";
import { displayColour, readThemeFile } from "../../../styles/themes";
import { AppearanceSection } from "./Appearance";
import { SettingsSubPageContext } from "../subPage";
import { useRoute } from "../../../product/state/route";
import { installMockFetch } from "../../../test-support";

beforeEach(() => {
  installMockFetch();
});

/** What Settings does around the page: the address names the sub-page. */
function Host() {
  const route = useRoute();
  return <SettingsSubPageContext.Provider value={route.name === "settings" ? route.sub ?? null : null}><AppearanceSection /></SettingsSubPageContext.Provider>;
}

async function openEditor() {
  window.location.hash = "#/settings/appearance";
  const user = userEvent.setup();
  render(<Host />);
  await user.click(await screen.findByRole("button", { name: "Edit…" }));
  await screen.findByRole("button", { name: "Import…" });
  return user;
}

it("keeps the previous colour and names the reason when a token value isn't a colour", async () => {
  useUi.setState({ customColors: { "--color-primary": "#3b82f6" } });
  const user = await openEditor();
  const field = screen.getByRole("textbox", { name: "Button" });
  await user.clear(field); await user.type(field, "#9a57"); await user.tab();
  expect(screen.getByText("Button: “#9a57” is not a colour. Use #rgb, #rrggbb or rgba().")).toBeVisible();
  expect(useUi.getState().customColors["--color-primary"]).toBe("#3b82f6");
  await user.clear(field); await user.type(field, "rgba(0,0,0,.5)"); await user.tab();
  expect(useUi.getState().customColors["--color-primary"]).toBe("rgba(0,0,0,.5)");
});

it("says why an import can't be read and changes nothing", async () => {
  useUi.setState({ customColors: {} });
  const user = await openEditor();
  const bad = new File(['{\n  "colors": {\n    "--color-primary": "nope"\n  }\n}'], "light.json", { type: "application/json" });
  await user.upload(screen.getByLabelText("Import theme file"), bad);
  expect(await screen.findByText("Couldn’t import light.json. Line 3: “Button” is not a colour. Nothing changed.")).toBeVisible();
  expect(useUi.getState().customColors).toEqual({});
});

it("reads a VS Code theme's colours and refuses text that isn't JSON", () => {
  expect(readThemeFile('{"colors":{"editor.background":"#101010"}}')).toEqual({ colors: { "--color-background": "#101010" } });
  expect(readThemeFile("nope")).toEqual({ error: "It isn't valid JSON." });
});

it("writes the colours the stylesheet holds the way the editor shows them", () => {
  expect(displayColour("rgb(0 0 0 / 3.5%)")).toBe("rgba(0,0,0,.035)");
  expect(displayColour("rgb(0 0 0 / 60%)")).toBe("rgba(0,0,0,.60)");
  expect(displayColour("rgb(0 0 0 / 92%)")).toBe("rgba(0,0,0,.92)");
  expect(displayColour("rgb(255 255 255 / 100%)")).toBe("#ffffff");
  expect(displayColour("#f6f6f6")).toBe("#f6f6f6");
});

it("lists one row per colour token in the design's order, with the Button token on the page's button colour", async () => {
  useUi.setState({ customColors: {} });
  await openEditor();
  const names = screen.getAllByRole("textbox").map((field) => field.getAttribute("aria-label"));
  expect(names).toEqual(["Window", "Surface", "Raised", "Selected", "Line", "Line, strong", "Text 1", "Text 2", "Text 3", "Text 4",
    "Button", "Button text", "Amber", "Amber text", "Green", "Blue", "Red"]);
});

/** An element of the app behind Settings that paints with a theme colour, which jsdom has to be told. */
function appElement(colour: string, text = "") {
  const element = document.createElement("div");
  element.id = "behind";
  element.style.backgroundColor = colour;
  element.textContent = text;
  document.body.appendChild(element);
  return element;
}
const mark = (element: Element, type: string, init: MouseEventInit = {}) => act(() => { element.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, ...init })); });

it("starts picking from Inspect, hides Settings, and Esc ends it with a note that nothing was picked", async () => {
  const user = await openEditor();
  await user.click(screen.getByRole("button", { name: "Inspect" }));
  expect(document.documentElement.dataset.inspecting).toBe("true");
  expect(screen.getByRole("region", { name: "Pick a colour" })).toHaveTextContent("Click a part of the app to find its colour.");
  await user.keyboard("{Escape}");
  expect(document.documentElement.dataset.inspecting).toBeUndefined();
  expect(screen.getByText("Nothing picked. The colours are as they were.")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(screen.queryByText("Nothing picked. The colours are as they were.")).toBeNull();
});

it("ends picking with Cancel and changes no colour", async () => {
  const before = useUi.getState().customColors;
  const user = await openEditor();
  await user.click(screen.getByRole("button", { name: "Inspect" }));
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.getByText("Nothing picked. The colours are as they were.")).toBeVisible();
  expect(useUi.getState().customColors).toBe(before);
});

it("picks the colour an element uses, selects its row and offers Pick again and Reset this colour", async () => {
  document.documentElement.style.setProperty("--color-background", "#ffffff");
  useUi.setState({ customColors: { "--color-background": "#fafafa" } });
  const user = await openEditor();
  const behind = appElement("#ffffff");
  await user.click(screen.getByRole("button", { name: "Inspect" }));
  mark(behind, "click");
  const card = await screen.findByRole("status", { name: "Picked colour" });
  expect(card).toHaveTextContent("It uses Surface");
  expect(card).toHaveTextContent("--color-background");
  expect(document.documentElement.dataset.inspecting).toBeUndefined();
  expect(screen.getByRole("textbox", { name: "Surface" }).closest(".tkn")).toHaveAttribute("data-picked", "true");
  await user.click(screen.getByRole("button", { name: "Reset this colour" }));
  expect(useUi.getState().customColors["--color-background"]).toBeUndefined();
  await user.click(screen.getByRole("button", { name: "Pick again" }));
  expect(document.documentElement.dataset.inspecting).toBe("true");
  behind.remove();
  document.documentElement.style.removeProperty("--color-background");
});

it("keeps inspecting and says so when the element's colour belongs to the content", async () => {
  const user = await openEditor();
  const behind = appElement("#ffffff");
  behind.setAttribute("data-content-colour", "project");
  await user.click(screen.getByRole("button", { name: "Inspect" }));
  mark(behind, "click");
  expect(document.documentElement.dataset.inspecting).toBe("true");
  expect(screen.getByRole("region", { name: "Pick a colour" })).toHaveTextContent("This colour belongs to the content, not the theme.");
  await user.keyboard("{Escape}");
  behind.remove();
});

it("does not let the click reach the part of the app it picked", async () => {
  const user = await openEditor();
  const behind = appElement("#ffffff");
  const reached = vi.fn();
  behind.addEventListener("click", reached);
  await user.click(screen.getByRole("button", { name: "Inspect" }));
  mark(behind, "click");
  expect(reached).not.toHaveBeenCalled();
  await user.keyboard("{Escape}");
  behind.remove();
});
