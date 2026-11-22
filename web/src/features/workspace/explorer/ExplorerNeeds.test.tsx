import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { Thread } from "../../../api/types";
import { setWaitingThreads } from "../../../product/state/waitingThreads";
import { ThreadStatus } from "./ExplorerRow";

const thread = { id: "t1", project_id: "p1", title: "Fix login", active_run: null, last_run_state: "completed", lifecycle: null } as unknown as Thread;
afterEach(() => { cleanup(); setWaitingThreads(new Set()); });

it("draws the Needs you dot for a thread whose terminal is waiting for the person", () => {
  render(<ThreadStatus thread={thread} />);
  expect(screen.queryByRole("img", { name: "Needs you" })).toBeNull();
  act(() => setWaitingThreads(new Set(["t1"])));
  expect(screen.getByRole("img", { name: "Needs you" })).toBeInTheDocument();
});
