import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { installMockFetch } from "../../../test-support";
import { wire } from "../../../wire.test-support";
import { useWorkspace } from "../../../product/state/workspace";
import type { Thread } from "../../../api/types";
import { QuitConfirmation } from "./QuitConfirmation";

let mock: ReturnType<typeof installMockFetch>;
/** The saved quit setting, as Settings writes it. */
const setMode = (value: string) => mock.handle("PUT", "/api/preferences", { values: { quit_confirmation: value } });
vi.mock("../../../api/shell", () => ({ inDesktopApp: () => true, readShellStatus: async () => undefined }));

beforeEach(() => {
  mock = installMockFetch();
  useWorkspace.setState({
    threads: { a: { id: "a", active_run: { state: "running" } }, b: { id: "b", active_run: { state: "running" } } } as unknown as Record<string, Thread>,
  });
});

afterEach(() => vi.unstubAllGlobals());

const press = async () => {
  // The key is live only once the platform and the saved mode have been read; the component shows no sign of that.
  await waitFor(() => {
    const paths = wire().filter((call) => call.method === "GET").map((call) => call.path);
    expect(paths).toContain("/api/info");
    expect(paths).toContain("/api/preferences");
  });
  await act(async () => {
    await Promise.resolve(); // let both answers land
    fireEvent.keyDown(window, { key: "q", metaKey: true });
  });
};

it.each([
  ["hold", "Hold to quit", "2 agents are working. Keep holding ⌘Q."],
  ["double_press", "Press ⌘Q again to quit", "2 agents are working and will stop."],
])("says what ⌘Q does in %s mode, with the real agent count", async (value, title, body) => {
  setMode(value);
  render(<QuitConfirmation />);
  await press();
  expect(await screen.findByText(title)).toBeInTheDocument();
  expect(screen.getByText(body)).toBeInTheDocument();
});

it("shows no dialog when quitting is at once", async () => {
  setMode("direct");
  render(<QuitConfirmation />);
  await press();
  expect(screen.queryByRole("status")).toBeNull();
});
