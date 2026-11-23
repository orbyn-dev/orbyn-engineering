import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { LiveLessons } from "./LiveLessons";
import { LessonUndoToast } from "./UndoToast";
import { clearLessonUndo, showLessonUndo, useLessonUndo } from "./undo";
import { lesson, list, project, stubFetch, writes, type Answer, type Call, type Write } from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; list?: Answer; write?: (one: Write) => Answer };

const notFound = { status: 404, body: { code: "not_found", message: "No such route." } } as const;
const toastName = "Lesson deleted. Undo for 10 seconds";

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    list: { status: 200, body: list() },
    write: () => ({ status: 200, body: lesson() }),
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
  clearLessonUndo();
});

describe("the delete's Undo toast", () => {
  it("counts the service's window down and goes away when it shuts", () => {
    vi.useFakeTimers();
    try {
      showLessonUndo({ id: "les-1", until: Date.now() + 10_000, seconds: 10 });
      render(<LessonUndoToast />);
      const toast = screen.getByRole("status", { name: toastName });
      expect(within(toast).getByText("Lesson deleted ·")).toBeInTheDocument();
      expect(within(toast).getByRole("button", { name: "Undo" })).toBeInTheDocument();
      expect(within(toast).getByText("10 s")).toBeInTheDocument();
      act(() => {
        vi.advanceTimersByTime(3000);
      });
      expect(within(toast).getByText("7 s")).toBeInTheDocument();
      act(() => {
        vi.advanceTimersByTime(7000);
      });
      expect(screen.queryByRole("status", { name: toastName })).toBeNull();
      expect(useLessonUndo.getState().pending).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("hides an Undo whose window had already shut", () => {
    showLessonUndo({ id: "les-1", until: Date.now() - 1, seconds: 10 });
    render(<LessonUndoToast />);
    expect(screen.queryByRole("status", { name: toastName })).toBeNull();
    expect(useLessonUndo.getState().pending).toBeNull();
  });

  it("is drawn over the list and takes the delete back, reading the list again", async () => {
    answers.write = ({ method, url }) =>
      method === "POST" && url === "/api/crew/lessons/les-1/undo" ? { status: 200, body: lesson() } : notFound;
    showLessonUndo({ id: "les-1", until: Date.now() + 10_000, seconds: 10 });
    const user = userEvent.setup();
    render(<LiveLessons view="list" />);
    const toast = await screen.findByRole("status", { name: toastName });
    await user.click(within(toast).getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(writes(calls, "POST")).toHaveLength(1));
    expect(writes(calls, "POST")[0]?.url).toBe("/api/crew/lessons/les-1/undo");
    await waitFor(() => expect(useLessonUndo.getState().pending).toBeNull());
    await waitFor(() => expect(calls.filter(({ url }) => url.includes("/api/crew/lessons?"))).toHaveLength(2));
  });

  it("says the delete is final when the service no longer takes it back", async () => {
    answers.write = () => ({ status: 404, body: { code: "not_found", message: "That delete is final now." } });
    showLessonUndo({ id: "les-1", until: Date.now() + 10_000, seconds: 10 });
    const user = userEvent.setup();
    render(<LiveLessons view="list" />);
    const toast = await screen.findByRole("status", { name: toastName });
    await user.click(within(toast).getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(useUi.getState().toasts.map((one) => one.text)).toContain("That delete is final now."));
    expect(useLessonUndo.getState().pending).toBeNull();
  });

  it("is not drawn when nothing was deleted", async () => {
    render(<LiveLessons view="list" />);
    await screen.findByRole("heading", { name: /^This repository · orbyn-engineering/ });
    expect(screen.queryByRole("status", { name: toastName })).toBeNull();
  });
});
