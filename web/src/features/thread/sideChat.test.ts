import { beforeEach, expect, it } from "vitest";
import { useUi } from "../../product/state/ui";
import { showSideChat } from "./sideChat";

beforeEach(() => {
  useUi.setState({ panelOpen: false, panelTab: null, sideChat: null, panelTabOrder: ["diff", "browser", "activity", "side", "tools"], panelTabsClosed: ["diff", "browser", "activity", "side", "tools"] });
});

it("opens the Side chat tab, focuses it and shows the panel, when the panel starts with no tab", () => {
  showSideChat("side-1");
  const ui = useUi.getState();
  expect(ui.sideChat).toBe("side-1");
  expect(ui.panelOpen).toBe(true);
  expect(ui.panelTab).toBe("side");
  expect(ui.panelTabsClosed).not.toContain("side");
  expect(ui.panelTabsClosed).toContain("diff");
});
