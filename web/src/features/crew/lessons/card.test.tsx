import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../product/state/workspace";
import { LessonCardPage } from "./LessonCardPage";
import { moment } from "./model";
import { markAdded } from "./saved";
import { clearLessonUndo, useLessonUndo } from "./undo";
import { bodyOf, detail, lesson, list, project, stubFetch, writes, type Answer, type Call, type Write } from "./support";

let calls: Call[] = [];
let answers: { preview?: Answer; projects?: Answer; list?: Answer; detail?: Answer; write?: (one: Write) => Answer };

const notFound = { status: 404, body: { code: "not_found", message: "No such route." } } as const;

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    detail: { status: 200, body: detail() },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
  clearLessonUndo();
});

const open = () => render(<LessonCardPage id="les-1" repo="p-app" />);
/** The card's own read has answered: its action row is drawn only then (the heading is drawn while the read runs). */
const loaded = () => screen.findByRole("button", { name: "Edit" });
const dialog = () => screen.findByRole("dialog", { hidden: true });

describe("a lesson's card on the real route", () => {
  it("draws nothing and reads no lesson route while Crew Preview is off", async () => {
    answers.preview = { status: 404, body: notFound.body };
    const { container } = open();
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(calls.some(({ url }) => url.includes("/api/crew/lessons"))).toBe(false);
  });

  it("draws nothing at all when the card's read answers 404 not_found", async () => {
    answers.detail = notFound;
    const { container } = open();
    await waitFor(() => expect(calls.some(({ url }) => /\/api\/crew\/lessons\/les-1/.test(url))).toBe(true));
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(screen.queryByText("Can’t read the lesson")).toBeNull();
  });

  it("draws the lesson, its evidence ids, how it is used and the person's actions", async () => {
    answers.detail = {
      status: 200,
      body: detail({
        lesson: lesson({ updated_at: "2026-11-09T10:00:00Z" }),
        evidence: [{ kind: "task", id: "5b2d8a3" }, { kind: "escape", id: "9be4410" }],
      }),
    };
    open();
    await loaded();
    expect(
      screen.getByText("Checks like repo_required failed on test files before: check the fixture path first."),
    ).toBeInTheDocument();
    expect(screen.getByText("Repeated failure")).toBeInTheDocument();
    expect(screen.getByText("Source").nextElementSibling).toHaveTextContent("Task 5b2d8a3, Escape 9be4410");
    expect(screen.getByText(/The same failing check, ended the same way/)).toBeInTheDocument();
    expect(screen.getByText("Updated").nextElementSibling).toHaveTextContent(moment("2026-11-09T10:00:00Z"));
    expect(screen.getByText("This repository", { selector: ".cx-apchip" })).toBeInTheDocument();
    expect(screen.getByText("orbyn-engineering", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByText("On: goes into briefs")).toBeInTheDocument();
    expect(screen.getByText(/Used in 3 briefs · last /)).toBeInTheDocument();
    expect(screen.getByText(/among at most 5 lessons/)).toBeInTheDocument();
    expect(screen.getByText(/Against the answer key when a task starts/)).toBeInTheDocument();
    expect(screen.getByText(/Lessons are added after routing/)).toBeInTheDocument();
    expect(screen.getByText(/nothing reads these samples yet/)).toBeInTheDocument();
    expect(screen.getByText("On this device only. Nothing is written to your repository")).toBeInTheDocument();
    for (const name of ["Edit", "Turn off", "Promote to all repositories", "Delete"])
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to Lessons" })).toHaveAttribute(
      "href",
      "#/settings/crew/lessons?repo=p-app",
    );
    expect(writes(calls, "POST")).toHaveLength(0);
    expect(writes(calls, "PATCH")).toHaveLength(0);
  });

  it("draws the evidence text of a confirmed escape, a revert and a person's correction", async () => {
    const cases = [
      {
        source: "escape",
        chip: "Confirmed escape",
        happened: `You confirmed this escape on ${moment(lesson().created_at)}. The text is the escape’s own recorded reason.`,
        evidence: [{ kind: "escape", id: "9be4410" }],
        line: "Escape 9be4410",
      },
      {
        source: "revert",
        chip: "Revert",
        happened: `You reverted this merge on ${moment(lesson().created_at)}. The text is the reason that was recorded with the revert. A revert you make is a source; a revert Crew makes itself writes no lesson until you confirm it as an escape.`,
        evidence: [{ kind: "merge", id: "3b9f2c1" }],
        line: "Revert 3b9f2c1",
      },
      {
        source: "person",
        chip: "Your correction",
        happened: /The lesson is the words you typed, capped, and it is marked as yours\./,
        evidence: [{ kind: "task", id: "5d1c8e0" }],
        line: "Task 5d1c8e0",
      },
    ] as const;
    for (const one of cases) {
      answers.detail = {
        status: 200,
        body: detail({
          lesson: lesson({ source: one.source, updated_at: "2026-11-09T10:00:00Z" }),
          evidence: [...one.evidence],
        }),
      };
      const { unmount } = open();
      await loaded();
      expect(screen.getByText(one.chip)).toBeInTheDocument();
      expect(screen.getByText(one.happened)).toBeInTheDocument();
      expect(screen.getByText(one.line)).toBeInTheDocument();
      unmount();
    }
  });

  it("draws an unknown source as its raw word in the plain chip, and does not throw", async () => {
    answers.detail = {
      status: 200,
      body: detail({
        lesson: lesson({ source: "future-kind" }),
        evidence: [{ kind: "task", id: "5b2d8a3" }],
      }),
    };
    open();
    await loaded();
    const chip = screen.getByText("future-kind");
    expect(chip).toHaveClass("cx-apchip");
    expect(chip).not.toHaveClass("cx-dim");
    expect(chip).not.toHaveClass("cx-no");
  });

  it("draws evidence of an unknown kind as its capitalised kind and id", async () => {
    answers.detail = {
      status: 200,
      body: detail({
        lesson: lesson({ source: "future-kind" }),
        evidence: [{ kind: "future-kind", id: "7c4e1d9" }],
      }),
    };
    open();
    await loaded();
    expect(screen.getByText("Future-kind 7c4e1d9")).toBeInTheDocument();
  });

  it("links a task to its thread and an escape to the Escapes page, and leaves a revert as plain text", async () => {
    const linked = [
      { evidence: [{ kind: "task", id: "5b2d8a3" }], text: "Task 5b2d8a3", href: "#/threads/5b2d8a3" },
      { evidence: [{ kind: "escape", id: "9be4410" }], text: "Escape 9be4410", href: "#/settings/crew/escapes" },
    ] as const;
    for (const one of linked) {
      answers.detail = { status: 200, body: detail({ evidence: [...one.evidence] }) };
      const { unmount } = open();
      await loaded();
      expect(screen.getByRole("link", { name: one.text })).toHaveAttribute("href", one.href);
      unmount();
    }
    answers.detail = {
      status: 200,
      body: detail({ lesson: lesson({ source: "revert" }), evidence: [{ kind: "merge", id: "3b9f2c1" }] }),
    };
    open();
    await loaded();
    expect(screen.getByText("Revert 3b9f2c1")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Revert 3b9f2c1" })).toBeNull();
  });

  it("says a lesson added by hand has no task, escape or revert behind it", async () => {
    answers.detail = {
      status: 200,
      body: detail({ lesson: lesson({ id: "les-added-1", source: "person" }), evidence: [] }),
    };
    open();
    await loaded();
    expect(screen.getByText("Added by you")).toBeInTheDocument();
    expect(screen.getByText("Source")).toBeInTheDocument();
    expect(screen.getByText("Added by you · no task, escape or revert behind it")).toBeInTheDocument();
    expect(screen.queryByText("Your correction")).toBeNull();
  });

  it("says Saved once on the card the Add page opened", async () => {
    markAdded("les-1");
    open();
    await loaded();
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    expect(screen.getByText(/The lesson is on this device\. Briefs that start from now on get this text\./)).toBeInTheDocument();
  });

  it("says a hand-added lesson is not in the next brief when the read says it is not", async () => {
    markAdded("les-1");
    answers.detail = {
      status: 200,
      body: detail({ lesson: lesson({ source: "person", edited_by_person: true, in_next_brief: false }) }),
    };
    open();
    await loaded();
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    expect(screen.getByText(/It is not in the next brief; the card says why\./)).toBeInTheDocument();
    expect(screen.queryByText(/Briefs that start from now on get this text\./)).toBeNull();
  });

  it("does not claim a hidden lesson goes into briefs, and says it is over the cap", async () => {
    answers.detail = { status: 200, body: detail({ lesson: lesson({ hidden_by_cap: true, in_next_brief: false }) }) };
    open();
    await loaded();
    expect(screen.getByText("On, but over the cap: readable under Hidden, never in a brief")).toBeInTheDocument();
    expect(screen.queryByText("On: goes into briefs")).toBeNull();
  });

  it("says a lesson that became a skill is not in the next brief", async () => {
    answers.detail = {
      status: 200,
      body: detail({ lesson: lesson({ skill: "Fixture first", in_next_brief: false }) }),
    };
    open();
    await loaded();
    expect(screen.getByText("On, not in the next brief: it is now a skill.")).toBeInTheDocument();
    expect(screen.queryByText("On: goes into briefs")).toBeNull();
  });

  it("says a lesson over the five-lesson limit is not in the next brief", async () => {
    answers.list = { status: 200, body: list({ use_lessons: true }) };
    answers.detail = { status: 200, body: detail({ lesson: lesson({ in_next_brief: false }) }) };
    open();
    await loaded();
    expect(
      await screen.findByText("On, not in the next brief: this repository is over the 5-lesson limit for a brief."),
    ).toBeInTheDocument();
    expect(screen.queryByText("On: goes into briefs")).toBeNull();
  });

  it("says the repository's lessons are switched off when its list answers use_lessons:false", async () => {
    answers.list = { status: 200, body: list({ use_lessons: false }) };
    answers.detail = { status: 200, body: detail({ lesson: lesson({ in_next_brief: false }) }) };
    open();
    await loaded();
    expect(
      await screen.findByText("On, not in the next brief: lessons are switched off for this repository."),
    ).toBeInTheDocument();
    expect(screen.queryByText("On, not in the next brief: this repository is over the 5-lesson limit for a brief.")).toBeNull();
  });

  it("says only that the lesson is not in the next brief while the lessons list is still loading", async () => {
    answers.list = "never";
    answers.detail = { status: 200, body: detail({ lesson: lesson({ in_next_brief: false }) }) };
    open();
    await loaded();
    expect(screen.getByText("On, not in the next brief.")).toBeInTheDocument();
    expect(
      screen.queryByText("On, not in the next brief: this repository is over the 5-lesson limit for a brief."),
    ).toBeNull();
    expect(
      screen.queryByText("On, not in the next brief: lessons are switched off for this repository."),
    ).toBeNull();
  });

  it("says only that the lesson is not in the next brief when the lessons list read fails", async () => {
    answers.list = { status: 500, body: { code: "unreadable", message: "The lessons on this device can’t be read." } };
    answers.detail = { status: 200, body: detail({ lesson: lesson({ in_next_brief: false }) }) };
    open();
    await loaded();
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/lessons?"))).toBe(true));
    expect(screen.getByText("On, not in the next brief.")).toBeInTheDocument();
    expect(
      screen.queryByText("On, not in the next brief: this repository is over the 5-lesson limit for a brief."),
    ).toBeNull();
    expect(
      screen.queryByText("On, not in the next brief: lessons are switched off for this repository."),
    ).toBeNull();
  });

  it("says a lesson is now a skill even while the lessons list has not answered", async () => {
    answers.list = "never";
    answers.detail = {
      status: 200,
      body: detail({ lesson: lesson({ skill: "Fixture first", in_next_brief: false }) }),
    };
    open();
    await loaded();
    expect(screen.getByText("On, not in the next brief: it is now a skill.")).toBeInTheDocument();
    expect(screen.queryByText("On, not in the next brief.")).toBeNull();
  });

  it("says a lesson is off and left out of two briefs, and offers Turn on", async () => {
    answers.detail = {
      status: 200,
      body: detail({ lesson: lesson({ state: "disabled", left_out_of: 2, in_next_brief: false }) }),
    };
    open();
    expect(await screen.findByText("This lesson is off.")).toBeInTheDocument();
    expect(screen.getByText(/Left out of 2 briefs: it matched that task’s checks\./)).toBeInTheDocument();
    expect(screen.getByText("2 briefs. That task started without it")).toBeInTheDocument();
    expect(screen.getByText("Off", { selector: ".cx-apchip" })).toBeInTheDocument();
    expect(screen.getByText("Off: stays here and does not go into a brief")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Turn on" })).toBeInTheDocument();
  });

  it("turns the lesson off with one Person write and draws the service's answer", async () => {
    answers.write = ({ method, url }) =>
      method === "POST" && url === "/api/crew/lessons/les-1/disable"
        ? { status: 200, body: lesson({ state: "disabled", in_next_brief: false, rev: 2 }) }
        : notFound;
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Turn off" }));
    expect(await screen.findByText("This lesson is off.")).toBeInTheDocument();
    expect(writes(calls, "POST")).toHaveLength(1);
    expect(writes(calls, "POST")[0]?.url).toBe("/api/crew/lessons/les-1/disable");
    expect(bodyOf(writes(calls, "POST")[0]?.init)).toEqual({ rev: 1 });
    expect(screen.getByRole("button", { name: "Turn on" })).toBeInTheDocument();
  });

  it("turns a disabled lesson back on", async () => {
    answers.detail = { status: 200, body: detail({ lesson: lesson({ state: "disabled", in_next_brief: false }) }) };
    answers.write = ({ method, url }) =>
      method === "POST" && url === "/api/crew/lessons/les-1/enable"
        ? { status: 200, body: lesson({ state: "active", rev: 2 }) }
        : notFound;
    const user = userEvent.setup();
    open();
    await screen.findByText("This lesson is off.");
    await user.click(screen.getByRole("button", { name: "Turn on" }));
    await waitFor(() => expect(screen.queryByText("This lesson is off.")).toBeNull());
    expect(bodyOf(writes(calls, "POST")[0]?.init)).toEqual({ rev: 1 });
    expect(screen.getByText("On: goes into briefs")).toBeInTheDocument();
  });

  it("edits the text with PATCH, counting characters, and says Saved.", async () => {
    answers.write = ({ method, url, body }) =>
      method === "PATCH" && url === "/api/crew/lessons/les-1"
        ? {
            status: 200,
            body: lesson({ text: String(body?.text), edited_by_person: true, rev: 2, updated_at: "2026-11-08T09:12:00Z" }),
          }
        : notFound;
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    expect(field).toHaveValue("Checks like repo_required failed on test files before: check the fixture path first.");
    expect(screen.getByText("84 of 300 characters")).toBeInTheDocument();
    expect(
      screen.getByText(/Editing changes the lesson’s text\. Saving runs the secret scan again\./),
    ).toBeInTheDocument();
    expect(screen.getByText("Nothing is saved until you press Save.")).toBeInTheDocument();
    await user.clear(field);
    await user.type(field, "Ask before changing a test’s expected value.");
    expect(screen.getByText("44 of 300 characters")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    expect(screen.getByText(/The lesson is changed on this device\./)).toBeInTheDocument();
    expect(screen.getByText("Ask before changing a test’s expected value.")).toBeInTheDocument();
    expect(writes(calls, "PATCH")).toHaveLength(1);
    expect(bodyOf(writes(calls, "PATCH")[0]?.init)).toEqual({ text: "Ask before changing a test’s expected value.", rev: 1 });
  });

  it("does not say briefs get the text when the saved lesson is not in the next brief", async () => {
    answers.detail = { status: 200, body: detail({ lesson: lesson({ hidden_by_cap: true, in_next_brief: false }) }) };
    answers.write = ({ method, url, body }) =>
      method === "PATCH" && url === "/api/crew/lessons/les-1"
        ? {
            status: 200,
            body: lesson({
              text: String(body?.text),
              edited_by_person: true,
              hidden_by_cap: true,
              in_next_brief: false,
              rev: 2,
            }),
          }
        : notFound;
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.clear(field);
    await user.type(field, "Ask before changing a test’s expected value.");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    expect(screen.getByText(/It is not in the next brief; the card says why\./)).toBeInTheDocument();
    expect(screen.queryByText(/Briefs that start from now on get this text\./)).toBeNull();
  });

  it("keeps the typed text and says the scan refused it, in the contract's exact sentence", async () => {
    answers.write = () => ({
      status: 400,
      body: { code: "secret_refused", message: "Not saved: this looks like it holds a secret. Edit it and save again." },
    });
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.clear(field);
    await user.type(field, "The token is sk-live-xxxx.");
    await user.click(screen.getByRole("button", { name: "Save" }));
    const refusal = await screen.findByRole("alert");
    expect(refusal).toHaveTextContent("Not saved: this looks like it holds a secret. Edit it and save again.");
    expect(screen.getByRole("textbox", { name: "Lesson text" })).toHaveValue("The token is sk-live-xxxx.");
    expect(screen.queryByText("Saved.")).toBeNull();
  });

  it("keeps the typed text when the save failed, and Try again sends it again", async () => {
    answers.write = () => ({ status: 500, body: { code: "unreadable", message: "The lessons on this device can’t be read." } });
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.clear(field);
    await user.type(field, "A new sentence.");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Can’t save the lesson")).toBeInTheDocument();
    expect(screen.getByText("The lessons on this device can’t be read.")).toBeInTheDocument();
    answers.write = ({ method, body }) =>
      method === "PATCH" ? { status: 200, body: lesson({ text: String(body?.text), rev: 2 }) } : notFound;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    expect(writes(calls, "PATCH")).toHaveLength(2);
  });

  it("Try again after a failed edit sends the text as it is now, not the text that failed", async () => {
    answers.write = () => ({ status: 500, body: { code: "unreadable", message: "The lessons on this device can’t be read." } });
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.clear(field);
    await user.type(field, "First sentence.");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Can’t save the lesson")).toBeInTheDocument();
    await user.clear(field);
    await user.type(field, "Second sentence.");
    answers.write = ({ method, body }) =>
      method === "PATCH" ? { status: 200, body: lesson({ text: String(body?.text), rev: 2 }) } : notFound;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    const sent = writes(calls, "PATCH").map(({ init }) => bodyOf(init).text);
    expect(sent).toEqual(["First sentence.", "Second sentence."]);
  });

  it("reads the lesson again after a stale edit, so Try again sends the fresh revision", async () => {
    answers.write = () => ({ status: 409, body: { code: "stale", message: "This lesson changed since you opened it." } });
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.clear(field);
    await user.type(field, "My sentence.");
    answers.detail = {
      status: 200,
      body: detail({
        lesson: lesson({ rev: 5 }),
        evidence: [{ kind: "task", id: "task-9" }],
      }),
    };
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("This lesson changed since you opened it.")).toBeInTheDocument();
    expect(await screen.findByText("Task task-9")).toBeInTheDocument();
    answers.write = ({ method, body }) =>
      method === "PATCH" ? { status: 200, body: lesson({ text: String(body?.text), rev: 6 }) } : notFound;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    expect(writes(calls, "PATCH").map(({ init }) => bodyOf(init).rev)).toEqual([1, 5]);
    expect(field).toHaveValue("My sentence.");
  });

  it("says the same failing check happened on two or more tasks, not exactly two", async () => {
    open();
    await loaded();
    expect(screen.getByText(/on 2 or more tasks of this repository within 60 days/)).toBeInTheDocument();
  });

  it("drops the Saved line once a later write changes the lesson", async () => {
    answers.write = ({ method, url, body }) =>
      method === "PATCH"
        ? { status: 200, body: lesson({ text: String(body?.text), edited_by_person: true, rev: 2 }) }
        : url === "/api/crew/lessons/les-1/promote"
          ? { status: 200, body: lesson({ scope: "global", repo: null, text: "Edited once.", rev: 3 }) }
          : notFound;
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Edit" }));
    const field = await screen.findByRole("textbox", { name: "Lesson text" });
    await user.clear(field);
    await user.type(field, "Edited once.");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Promote to all repositories" }));
    await user.click(within(await dialog()).getByRole("button", { name: "Promote lesson" }));
    expect(await screen.findByRole("button", { name: "Move back to one repository" })).toBeInTheDocument();
    expect(screen.queryByText("Saved.")).toBeNull();
  });

  it("promotes through its confirm, with where the lesson would apply, and draws the answer", async () => {
    answers.write = ({ method, url }) =>
      method === "POST" && url === "/api/crew/lessons/les-1/promote"
        ? { status: 200, body: lesson({ scope: "global", repo: null, rev: 2 }) }
        : notFound;
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Promote to all repositories" }));
    const box = await dialog();
    expect(within(box).getByText("Promote to all repositories?")).toBeInTheDocument();
    expect(within(box).getByText("Would apply to")).toBeInTheDocument();
    expect(within(box).getByText("This repository (orbyn-engineering)")).toBeInTheDocument();
    expect(within(box).getByText(/Crew never promotes one by itself/)).toBeInTheDocument();
    await user.click(within(box).getByRole("button", { name: "Promote lesson" }));
    expect(await screen.findByRole("button", { name: "Move back to one repository" })).toBeInTheDocument();
    expect(writes(calls, "POST")).toHaveLength(1);
    expect(bodyOf(writes(calls, "POST")[0]?.init)).toEqual({ rev: 1 });
    expect(screen.getByText("All repositories", { selector: ".cx-apchip" })).toBeInTheDocument();
  });

  it("refuses a promotion the contract refuses, sending nothing, and offers Edit", async () => {
    answers.detail = { status: 200, body: detail({ lesson: lesson({ names_path: true }) }) };
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Promote to all repositories" }));
    expect(await screen.findByText("Can’t promote this lesson")).toBeInTheDocument();
    expect(
      screen.getByText("It names a path, so it can’t apply to all repositories. Edit the text first."),
    ).toBeInTheDocument();
    expect(writes(calls, "POST")).toHaveLength(0);
    expect(screen.queryByRole("dialog", { hidden: true })).toBeNull();
    await user.click(within(screen.getByRole("alert")).getByRole("button", { name: "Edit" }));
    expect(await screen.findByRole("textbox", { name: "Lesson text" })).toBeInTheDocument();
  });

  it("moves a global lesson back to this repository through its confirm", async () => {
    answers.detail = { status: 200, body: detail({ lesson: lesson({ scope: "global", repo: null, rev: 3 }) }) };
    answers.write = ({ method, url }) =>
      method === "POST" && url === "/api/crew/lessons/les-1/demote"
        ? { status: 200, body: lesson({ scope: "repo", repo: "p-app", rev: 4 }) }
        : notFound;
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Move back to one repository" }));
    const box = await dialog();
    expect(within(box).getByText("Move back to one repository?")).toBeInTheDocument();
    expect(within(box).getByText("Would apply to")).toBeInTheDocument();
    expect(within(box).getByText("This repository (orbyn-engineering)")).toBeInTheDocument();
    await user.click(within(box).getByRole("button", { name: "Move lesson back" }));
    expect(await screen.findByRole("button", { name: "Promote to all repositories" })).toBeInTheDocument();
    expect(bodyOf(writes(calls, "POST")[0]?.init)).toEqual({ repo: "p-app", rev: 3 });
  });

  it("deletes at once, with no confirm, and leaves the list to draw its Undo toast", async () => {
    answers.write = ({ method, url }) =>
      method === "DELETE" && url.startsWith("/api/crew/lessons/les-1?rev=1")
        ? { status: 200, body: { id: "les-1", undo_until: new Date(Date.now() + 10_000).toISOString() } }
        : notFound;
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(window.location.hash).toBe("#/settings/crew/lessons?repo=p-app"));
    expect(screen.queryByRole("dialog", { hidden: true })).toBeNull();
    expect(writes(calls, "DELETE")).toHaveLength(1);
    expect(writes(calls, "DELETE")[0]?.url).toBe("/api/crew/lessons/les-1?rev=1");
    const pending = useLessonUndo.getState().pending;
    expect(pending?.id).toBe("les-1");
    expect(pending?.seconds).toBe(10);
  });

  it("keeps the lesson and says why when the delete was refused", async () => {
    answers.write = () => ({ status: 409, body: { code: "stale", message: "This lesson changed since you opened it." } });
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("Can’t delete the lesson")).toBeInTheDocument();
    expect(screen.getByText("This lesson changed since you opened it.")).toBeInTheDocument();
    expect(window.location.hash).toBe("");
    expect(useLessonUndo.getState().pending).toBeNull();
  });

  it("says to check the service when a delete could not reach it, as every other write does", async () => {
    answers.write = () => "network";
    const user = userEvent.setup();
    open();
    await loaded();
    await user.click(screen.getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("Can’t delete the lesson")).toBeInTheDocument();
    expect(screen.getByText(/Check that the Orbyn service is running\./)).toBeInTheDocument();
  });

  it("draws nothing for an id the route lacks or the service would refuse, even while the project list is still being read", async () => {
    answers.projects = "never";
    useWorkspace.setState({ projects: { data: null, loading: true, error: null } });
    for (const id of [undefined, "les 1/.."]) {
      const { container, unmount } = render(<LessonCardPage id={id} repo="p-app" />);
      await waitFor(() => expect(calls.some(({ url }) => url.startsWith("/api/projects"))).toBe(true));
      expect(container).toBeEmptyDOMElement();
      unmount();
    }
  });

  it("draws nothing for a repository the route names but the project list does not show", async () => {
    const { container } = render(<LessonCardPage id="les-1" repo="p-nowhere" />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    expect(container).toBeEmptyDOMElement();
    expect(calls.some(({ url }) => url.includes("/api/crew/lessons"))).toBe(false);
  });

  it("uses the first project when the route names no repository", async () => {
    render(<LessonCardPage id="les-1" />);
    await loaded();
    expect(calls.some(({ url }) => url.includes("/api/crew/lessons/les-1") && url.includes("repo=p-app"))).toBe(true);
  });

  it("draws nothing and reads no lesson when the id is not one the service accepts", async () => {
    const { container } = render(<LessonCardPage id="les 1/.." repo="p-app" />);
    await waitFor(() => expect(calls.some(({ url }) => url.includes("/api/crew/preview"))).toBe(true));
    expect(container).toBeEmptyDOMElement();
    expect(calls.some(({ url }) => url.includes("/api/crew/lessons"))).toBe(false);
  });

  it("fails in one red banner that changes nothing, and Try again reads the card again", async () => {
    answers.detail = { status: 500, body: { code: "unreadable", message: "The lessons on this device can’t be read." } };
    const user = userEvent.setup();
    open();
    expect(await screen.findByText("Can’t read the lesson")).toBeInTheDocument();
    answers.detail = { status: 200, body: detail() };
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await loaded()).toBeInTheDocument();
    expect(calls.filter(({ url }) => /\/api\/crew\/lessons\/les-1/.test(url))).toHaveLength(2);
  });
});
