import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createLibraryMock } from "../../../mock/library";
import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import { useConnection } from "../../product/state/connection";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { setLibraryApi } from "../library";
import { PanelHost } from "./PanelHost";

vi.mock("./browser/BrowserPanel", () => ({ BrowserPanel: () => <div>Browser content</div> }));
vi.mock("./diff/DiffPanel", () => ({ DiffPanel: () => <div>Diff content</div> }));
vi.mock("./files/FileTab", () => ({ FileTab: () => <div>Files content</div> }));
vi.mock("./activity/ActivityPanel", () => ({ ActivityPanel: () => null }));

const thread = { id: "thread-1", project_id: "p1" } as Thread;
const tabs = () => screen.getAllByRole("tab").map((tab) => tab.getAttribute("aria-label"));

beforeEach(() => {
  vi.spyOn(api, "diff").mockResolvedValue([]);
  vi.spyOn(api, "keybindings").mockResolvedValue({ rules: [], issues: [] } as never);
  useConnection.setState({ offline: false, serviceDown: false });
  useWorkspace.setState({ threads: { "thread-1": thread } });
  useUi.setState({ panelOpen: true, panelTab: null, terminalOpen: false, panelTabOrder: ["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"], panelTabsClosed: ["tools", "linked", "review", "devices", "library"] });
});
afterEach(() => { setLibraryApi(null); vi.unstubAllGlobals(); });

const host = () => render(<PanelHost thread={thread} mode="docked" closing={false} moves={false} content={{}} maximized={false} onToggleMaximize={() => {}} onReturn={() => {}} />);

it("draws no Library tab, and no launcher entry, while the service answers 404", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 404 })));
  host();
  await act(async () => { await Promise.resolve(); });
  expect(tabs()).not.toContain("Library");
  await userEvent.setup().click(screen.getByRole("button", { name: "Open a tab" }));
  expect(screen.queryByRole("menuitem", { name: "Library" })).toBeNull();
});

it("shows a Library tab once the service reports a library, and the launcher opens the one tab", async () => {
  setLibraryApi(createLibraryMock());
  host();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Open a tab" }));
  await user.click(await screen.findByRole("menuitem", { name: "Library" }));
  expect(await screen.findByRole("tablist", { name: "Library" })).toBeVisible();
  expect(tabs()).toContain("Library");
});
