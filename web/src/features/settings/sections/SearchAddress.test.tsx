import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import { useUi } from "../../../product/state/ui";
import { SearchEngineRows } from "./SearchEngineRows";
import { SettingGroup } from "../rows/SettingRow";

const mount = () => render(<SettingGroup><SearchEngineRows /></SettingGroup>);
const engine = () => screen.getByRole("combobox", { name: "Search engine" });

beforeEach(() => {
  useUi.getState().setPrefs({ previewSearchEngine: "duckduckgo", previewSearchAddress: "" });
});

it("keeps the six engines and adds Custom after them, with no address row until it is chosen", () => {
  mount();
  expect([...engine().querySelectorAll("option")].map((option) => option.textContent)).toEqual(["DuckDuckGo", "Google", "Bing", "Brave", "Kagi", "Startpage", "Custom…"]);
  expect(screen.queryByRole("textbox", { name: "Search address" })).toBeNull();
});

it("shows an empty address with a placeholder, no error, and says DuckDuckGo is used", async () => {
  const user = userEvent.setup();
  mount();
  await user.selectOptions(engine(), "custom");
  const field = screen.getByRole("textbox", { name: "Search address" });
  expect(field).toHaveValue("");
  expect(field).toHaveAttribute("placeholder", "https://example.com/search?q=%s");
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByText("Not used yet. Searches use DuckDuckGo until an address is added.")).toBeVisible();
});

it("saves a valid address as it is typed and previews what the words open", async () => {
  const user = userEvent.setup();
  mount();
  await user.selectOptions(engine(), "custom");
  await user.type(screen.getByRole("textbox", { name: "Search address" }), "https://find.example/?q=%s");
  expect(useUi.getState().previewSearchAddress).toBe("https://find.example/?q=%s");
  expect(useUi.getState().previewSearchEngine).toBe("custom");
  expect(screen.getByText("https://find.example/?q=orbyn%20designs")).toBeVisible();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("says what to add when there is no %s, saves nothing, and names the engine used meanwhile", async () => {
  const user = userEvent.setup();
  mount();
  await user.selectOptions(engine(), "custom");
  const field = screen.getByRole("textbox", { name: "Search address" });
  await user.type(field, "https://find.example/?q=");
  expect(screen.getByRole("alert")).toHaveTextContent("Add %s where the search words go.");
  expect(field).toHaveAttribute("aria-invalid", "true");
  expect(useUi.getState().previewSearchAddress).toBe("");
  expect(screen.getByText("Not used yet. Searches use DuckDuckGo until this is fixed.")).toBeVisible();
});

it("refuses a bare host with the scheme sentence", async () => {
  const user = userEvent.setup();
  mount();
  await user.selectOptions(engine(), "custom");
  await user.type(screen.getByRole("textbox", { name: "Search address" }), "find.example/?q=%s");
  expect(screen.getByRole("alert")).toHaveTextContent("Start the address with https:// or http://.");
  expect(useUi.getState().previewSearchAddress).toBe("");
});

it("keeps the last valid address in use while a later edit is invalid, and keeps it when a built-in engine is chosen", async () => {
  const user = userEvent.setup();
  useUi.getState().setPrefs({ previewSearchEngine: "custom", previewSearchAddress: "https://find.example/?q=%s" });
  mount();
  const field = screen.getByRole("textbox", { name: "Search address" });
  await user.click(field);
  await user.paste("&r=%s");
  expect(screen.getByRole("alert")).toHaveTextContent("Use %s once");
  await user.clear(field);
  await user.paste("https://find.example/");
  expect(useUi.getState().previewSearchAddress).toBe("https://find.example/?q=%s");
  expect(screen.getByText("Not used yet. Searches use the last saved address until this is fixed.")).toBeVisible();
  await user.selectOptions(engine(), "google");
  expect(screen.queryByRole("textbox", { name: "Search address" })).toBeNull();
  expect(useUi.getState().previewSearchAddress).toBe("https://find.example/?q=%s");
  await user.selectOptions(engine(), "custom");
  expect(screen.getByRole("textbox", { name: "Search address" })).toHaveValue("https://find.example/?q=%s");
});
