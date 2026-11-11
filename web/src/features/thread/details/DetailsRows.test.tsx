import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import type { LocalServer } from "../../../api/types";
import { useUi } from "../../../product/state/ui";
import { DetailsDismiss } from "./DetailsDismiss";
import { ChangesRow, LimitsRow, LimitValue, ServersRow, tokensShort } from "./DetailsRows";
import * as api from "../api";

afterEach(() => vi.restoreAllMocks());

const ready = <T,>(data: T) => ({ status: "ready" as const, data, error: null, refreshing: false, reload: () => Promise.resolve() });
const server = (port: number, in_checkout = true): LocalServer => ({ port, pid: port, process: "node", in_checkout });

it("shows every limit window in the same ink: 18% left and 0% left are not amber", () => {
  const { container } = render(
    <LimitValue
      windows={[
        { label: "5h", used_percent: 82 },
        { label: "Weekly", used_percent: 12 },
        { label: "Monthly", used_percent: 100 },
      ]}
    />,
  );
  expect(screen.getByText("5h 18% left")).toBeInTheDocument();
  expect(screen.getByText("Monthly 0% left")).toBeInTheDocument();
  expect(container.querySelector(".amb, .tg-card-bad")).toBeNull();
});

it("shortens token counts as 38k, 1.5k and 1.5M", () => {
  expect([950, 1500, 38_000, 1_500_000, 2_000_000].map(tokensShort)).toEqual(["950", "1.5k", "38k", "1.5M", "2M"]);
});

it("reads Not reported on the Limits row for OpenCode, Claude and a custom engine", () => {
  for (const engine of ["opencode", "claude", "my-own-engine"]) {
    const { unmount } = render(<LimitsRow threadId="t-1" engine={engine} />);
    expect(screen.getByText("Limits").closest(".drow")).toHaveTextContent("Not reported");
    unmount();
  }
});

it("leaves Codex to its own window: no Not reported while it has none", async () => {
  render(<LimitsRow threadId="t-1" engine="codex" />);
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(screen.queryByText("Not reported")).toBeNull();
});

it("Changes says No changes without a chevron, and with changes shows the totals and opens the Changes tab", async () => {
  useUi.setState({ panelOpen: false });
  const toggle = vi.spyOn(useUi.getState(), "togglePanel");
  const dismiss = vi.fn();
  const none = render(<ChangesRow diff={ready([])} />);
  expect(screen.getByRole("button", { name: /Changes/ })).toHaveTextContent("No changes");
  none.unmount();
  const files = [{ path: "a", additions: 3, deletions: 1 }, { path: "b", additions: 1, deletions: 2 }] as never;
  render(
    <DetailsDismiss.Provider value={dismiss}>
      <ChangesRow diff={ready(files)} />
    </DetailsDismiss.Provider>,
  );
  const row = screen.getByRole("button", { name: /Changes/ });
  expect(row).toHaveTextContent("+4 −3 · 2 files");
  await userEvent.setup().click(row);
  expect(toggle).toHaveBeenCalledWith("diff");
  expect(dismiss).toHaveBeenCalled();
});

it("Local servers reads None running, one address, or N servers with each address a link", async () => {
  const open = vi.fn();
  useUi.setState({ showInBrowser: open } as never);
  const none = render(<ServersRow threadId="t" servers={ready([])} />);
  expect(screen.getByText("None running")).toBeInTheDocument();
  none.unmount();
  const one = render(<ServersRow threadId="t" servers={ready([server(5173)])} />);
  await userEvent.setup().click(screen.getByRole("button", { name: /Local servers/ }));
  expect(open).toHaveBeenCalledWith("http://localhost:5173");
  one.unmount();
  render(<ServersRow threadId="t" servers={ready([server(5173), server(6006)])} />);
  expect(screen.getByText("2 servers")).toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole("button", { name: "localhost:6006" }));
  expect(open).toHaveBeenLastCalledWith("http://localhost:6006");
});

it("with more than three servers, reads the count, the first three addresses and +N more; the chevron opens each one's Open, Copy and Stop", async () => {
  const user = userEvent.setup();
  const stop = vi.spyOn(api, "stopThreadServer").mockResolvedValue(undefined);
  const reload = vi.fn(() => Promise.resolve());
  const list = [5173, 3000, 3001, 3002, 3003, 3004, 3005].map((port) => server(port));
  render(<ServersRow threadId="t" servers={{ ...ready(list), reload }} />);
  expect(screen.getByText("7 running")).toBeInTheDocument();
  expect(screen.getByText("localhost:5173 · :3000 · :3001 · +4 more")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: /Local servers/ }));
  const menu = await screen.findByRole("menu", { name: "Local servers" });
  await user.click(within(menu).getByRole("menuitem", { name: "localhost:3000" }));
  const sub = await screen.findAllByRole("menuitem");
  expect(sub.map((item) => item.textContent)).toEqual(expect.arrayContaining(["Open", "Copy", "Stop"]));
  await act(async () => {
    await user.click(screen.getByRole("menuitem", { name: "Stop" }));
  });
  expect(stop).toHaveBeenCalledWith("t", 3000);
  expect(reload).toHaveBeenCalled();
});

it("a server outside the checkout can be opened and copied but not stopped", async () => {
  const user = userEvent.setup();
  const list = [1, 2, 3, 4].map((n) => server(4000 + n, n !== 1));
  render(<ServersRow threadId="t" servers={ready(list)} />);
  await user.click(screen.getByRole("button", { name: /Local servers/ }));
  await user.click(await screen.findByRole("menuitem", { name: "localhost:4001" }));
  expect((await screen.findAllByRole("menuitem")).map((item) => item.textContent)).not.toContain("Stop");
});
