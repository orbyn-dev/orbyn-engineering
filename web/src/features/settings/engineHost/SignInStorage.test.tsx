import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createEngineHostMock, storageFixtures } from "../../../../mock/engineHost";
import { KEYCHAIN_DENIED, KEYCHAIN_LOCKED, type SignInStorage } from "./contract";
import { setEngineHostApi } from "./hostApi";
import { SignInStorageGroup, SignInStorageRow } from "./SignInStorage";

afterEach(() => { setEngineHostApi(null); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const rowOf = (name: string) => screen.getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement;

it("draws nothing, and sends no request, while the service offers no sign-in storage", async () => {
  const fetching = vi.fn();
  vi.stubGlobal("fetch", fetching);
  render(<><SignInStorageGroup /><SignInStorageRow engineId="cursor" /></>);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText("Sign-in storage")).toBeNull();
  expect(fetching).not.toHaveBeenCalled();
});

it("lists each engine with its choice and the one action that fits it", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<SignInStorageGroup />);
  await screen.findByRole("table", { name: "Sign-in storage" });
  expect(screen.getByText("Your choice, one engine at a time. Orbyn only ever uses the system keychain: never a file, and nothing leaves it except to the engine’s own service.")).toBeVisible();
  expect(within(rowOf("Cursor")).getByText("Keychain on")).toBeVisible();
  expect(within(rowOf("Cursor")).getByText("Allowed. Its sign-in is kept in the keychain and goes only to Cursor.")).toBeVisible();
  expect(within(rowOf("Cursor")).getByRole("button", { name: "Revoke…" })).toBeVisible();
  expect(within(rowOf("Codex")).getByText("Declined. Orbyn stores nothing; Codex keeps its own sign-in.")).toBeVisible();
  expect(within(rowOf("Codex")).getByRole("button", { name: "Use keychain…" })).toBeVisible();
  expect(within(rowOf("Antigravity")).getByText("Not asked yet. Orbyn stores nothing until you choose.")).toBeVisible();
  expect(within(rowOf("Antigravity")).getByRole("button", { name: "Choose…" })).toBeVisible();
});

it("asks first and writes nothing on Not now or on closing", async () => {
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<SignInStorageGroup />);
  await user.click(await within(await screen.findByRole("table", { name: "Sign-in storage" })).findByRole("button", { name: "Choose…" }));
  const dialog = await screen.findByRole("dialog", { name: "Keep Antigravity’s sign-in in your keychain?" });
  expect(within(dialog).getByText(/Orbyn would store it in the system keychain, never in a file, and send it only to Antigravity\./)).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Not now" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual([]);
});

it("Use keychain is one write, however fast the clicks, and leaves the row allowed", async () => {
  const mock = createEngineHostMock({ delayMs: 20 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<SignInStorageGroup />);
  await user.click(await within(await screen.findByRole("table", { name: "Sign-in storage" })).findByRole("button", { name: "Choose…" }));
  await user.dblClick(within(await screen.findByRole("dialog")).getByRole("button", { name: "Use keychain" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["allow:antigravity"]);
  expect(within(rowOf("Antigravity")).getByText("Keychain on")).toBeVisible();
});

it("Don’t store is one decline write", async () => {
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<SignInStorageGroup />);
  await user.click(await within(await screen.findByRole("table", { name: "Sign-in storage" })).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Don’t store" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["decline:antigravity"]);
  expect(within(rowOf("Antigravity")).getByText("Declined")).toBeVisible();
});

it("blocks Escape while the write is in flight", async () => {
  const mock = createEngineHostMock({ delayMs: 60 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<SignInStorageGroup />);
  await user.click(await within(await screen.findByRole("table", { name: "Sign-in storage" })).findByRole("button", { name: "Choose…" }));
  const dialog = await screen.findByRole("dialog");
  await user.click(within(dialog).getByRole("button", { name: "Use keychain" }));
  await user.keyboard("{Escape}");
  expect(screen.getByRole("dialog")).toBeVisible();
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

it("says a locked keychain and a denied one apart, and falls back to nothing", async () => {
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<SignInStorageGroup />);
  const choose = async () => {
    await user.click(await within(await screen.findByRole("table", { name: "Sign-in storage" })).findByRole("button", { name: "Choose…" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Use keychain" }));
  };
  mock.failNext(KEYCHAIN_LOCKED);
  await choose();
  const row = () => rowOf("Antigravity");
  expect(await within(row()).findByText("Locked")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("The keychain is locked. Unlock it, then try again.");
  expect(within(row()).getByText("Not saved. Antigravity needs you to sign in again.")).toBeVisible();
  // Try again repeats the same request: one more write.
  mock.failNext(KEYCHAIN_DENIED);
  await user.click(screen.getByRole("button", { name: "Try again" }));
  expect(await within(row()).findByText("Denied")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveTextContent("The system didn’t let Orbyn use the keychain. Allow it in the system’s keychain settings, then try again.");
  expect(within(row()).getByText("Not saved. Orbyn stores nothing.")).toBeVisible();
  expect(mock.writes).toEqual(["allow:antigravity", "allow:antigravity"]);
});

it("shows Revoking while it removes, and Revoke… names the threads it stops", async () => {
  const mock = createEngineHostMock({ delayMs: 40 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<SignInStorageGroup />);
  await user.click(await within(await screen.findByRole("table", { name: "Sign-in storage" })).findByRole("button", { name: "Revoke…" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove Cursor’s sign-in from the keychain?" });
  expect(within(dialog).getByText(/Orbyn deletes the item it stored\. 2 threads are running on Cursor; they stop now and can be resumed after you sign in again\. Orbyn stores nothing after this\./)).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  expect(mock.writes).toEqual([]);
  await user.click(screen.getByRole("button", { name: "Revoke…" }));
  await user.dblClick(within(await screen.findByRole("dialog")).getByRole("button", { name: "Stop 2 threads and remove" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["revoke:cursor"]);
  expect(within(rowOf("Cursor")).getByText("Declined. Orbyn stores nothing; Cursor keeps its own sign-in.")).toBeVisible();
});

it("words Revoke… for no running thread, and for one", async () => {
  setEngineHostApi(createEngineHostMock({ storage: [{ engine_id: "cursor", name: "Cursor", choice: "allowed", allowed_at: new Date(2026, 10, 21).toISOString(), running_threads: 0 }] }));
  const user = userEvent.setup();
  render(<SignInStorageRow engineId="cursor" />);
  await user.click(await screen.findByRole("button", { name: "Revoke…" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).queryByText(/threads? (are|is) running/)).toBeNull();
  expect(within(dialog).getByRole("button", { name: "Remove" })).toBeVisible();
});

it("shows the detail row with the date it was allowed, and Removing while the item goes", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<SignInStorageRow engineId="cursor" />);
  expect(await screen.findByText(/^Allowed 21 Nov\. Kept in the keychain; goes only to Cursor\.$/)).toBeVisible();
  const removing: SignInStorage[] = [{ ...storageFixtures()[0]!, choice: "removing" }];
  setEngineHostApi(createEngineHostMock({ storage: removing }));
  expect(await screen.findByText("Removing it from the keychain…")).toBeVisible();
});

it("never renders a credential, even if the service sent one by mistake", async () => {
  const leaky = [{ ...storageFixtures()[0]!, token: "sk-LEAK-123", secret: "hunter2", sign_in: { access_token: "sk-LEAK-456" } }];
  setEngineHostApi(createEngineHostMock({ storage: leaky as never }));
  const user = userEvent.setup();
  const { container } = render(<SignInStorageGroup />);
  await user.click(await screen.findByRole("button", { name: "Revoke…" }));
  expect(container.ownerDocument.body.innerHTML).not.toMatch(/LEAK|hunter2/);
  expect(location.href).not.toMatch(/LEAK|hunter2/);
});

it("puts no engine name or choice into a URL or the console", async () => {
  const log = vi.spyOn(console, "log").mockImplementation(() => undefined);
  const warn = vi.spyOn(console, "warn").mockImplementation(() => undefined);
  const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
  const mock = createEngineHostMock();
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<SignInStorageGroup />);
  await user.click(await within(await screen.findByRole("table", { name: "Sign-in storage" })).findByRole("button", { name: "Choose…" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Use keychain" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(location.hash + location.search).toBe("");
  for (const spy of [log, warn, error]) expect(spy).not.toHaveBeenCalled();
});
