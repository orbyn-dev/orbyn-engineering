import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createDevicesMock } from "../../../mock/devices";
import { api } from "../../api/client";
import type { Thread } from "../../api/types";
import { useConnection } from "../../product/state/connection";
import { useUi } from "../../product/state/ui";
import { setDevicesApi } from "../devices";
import { PanelHost } from "./PanelHost";

vi.mock("./browser/BrowserPanel", () => ({ BrowserPanel: () => <div>Browser content</div> }));
vi.mock("./diff/DiffPanel", () => ({ DiffPanel: () => <div>Diff content</div> }));
vi.mock("./files/FileTab", () => ({ FileTab: () => <div>Files content</div> }));
vi.mock("./activity/ActivityPanel", () => ({ ActivityPanel: () => null }));

const thread = { id: "thread-1" } as Thread;
const tabs = () => screen.getAllByRole("tab").map((tab) => tab.getAttribute("aria-label"));

beforeEach(() => {
  vi.spyOn(api, "diff").mockResolvedValue([]);
  vi.spyOn(api, "keybindings").mockResolvedValue({ rules: [], issues: [] } as never);
  useConnection.setState({ offline: false, serviceDown: false });
  useUi.setState({ panelOpen: true, panelTab: null, terminalOpen: false, panelTabOrder: ["diff", "browser", "activity", "side", "tools", "linked", "review", "devices"], panelTabsClosed: ["tools", "linked", "review"] });
});
afterEach(() => { setDevicesApi(null); vi.unstubAllGlobals(); });

const host = () => render(<PanelHost thread={thread} mode="docked" closing={false} moves={false} content={{}} maximized={false} onToggleMaximize={() => {}} onReturn={() => {}} />);

it("draws no Devices tab, and no launcher entry, while the service answers 404 for the list", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response("{}", { status: 404 })));
  useUi.setState({ panelTabsClosed: ["tools", "linked", "review"] });
  host();
  await act(async () => { await Promise.resolve(); });
  expect(tabs()).not.toContain("Devices");
  await userEvent.setup().click(screen.getByRole("button", { name: "Open a tab" }));
  expect(screen.queryByRole("menuitem", { name: "Devices" })).toBeNull();
});

it("shows a Devices tab and its list once the service reports devices, and the launcher can open it", async () => {
  setDevicesApi(createDevicesMock());
  useUi.setState({ panelTabsClosed: ["tools", "linked", "review", "devices"] });
  host();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Open a tab" }));
  await user.click(await screen.findByRole("menuitem", { name: "Devices" }));
  expect(await screen.findByRole("list", { name: "Devices" })).toBeVisible();
  await waitFor(() => expect(tabs()).toContain("Devices"));
});
