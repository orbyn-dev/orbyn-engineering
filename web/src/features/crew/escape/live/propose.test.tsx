import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "../../../../product/state/workspace";
import { LiveEscapes } from "./LiveEscapes";
import { bodyOf, checkRow, detail, log, post, posts, project, row, stubFetch, type Answer, type Call } from "./support";

let calls: Call[] = [];
let answers: { preview: Answer; projects: Answer; log: Answer; logConfirmed: Answer; possible: Answer; detail: Answer; decide: Answer; gap: Answer; check?: Answer };

beforeEach(() => {
  calls = [];
  answers = {
    preview: { status: 200, body: { enabled: true } },
    projects: { status: 200, body: [project] },
    log: { status: 200, body: log() },
    logConfirmed: { status: 200, body: log() },
    possible: { status: 200, body: { project: "p-app", as_of: 1, total: 0, rows: [] } },
    detail: { status: 200, body: detail() },
    decide: { status: 200, body: row({ state: "confirmed" }) },
    gap: { status: 200, body: row({ state: "confirmed", gap: "tooling_gap" }) },
  };
  useWorkspace.setState({ projects: { data: [project], loading: false, error: null } });
  stubFetch(answers, calls);
});

const PROPOSED = { status: 200, body: { escape: row({ state: "confirmed" }), check: checkRow({ state: "proposed" }) } };

async function fill(user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement) {
  await user.type(within(dialog).getByLabelText("New test file"), "tests/auth/return-to.test.ts");
  await user.type(within(dialog).getByLabelText("What it asserts"), "An old returnTo link still redirects to its path");
  await user.type(within(dialog).getByLabelText("How it runs"), "npm test -- tests/auth/return-to.test.ts");
}

describe("Propose a check (escape-create-check on the real propose route)", () => {
  it("opens the dialog with what escaped and sends nothing until Propose the check", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Propose a check" }));
    const dialog = await screen.findByRole("dialog", { name: "Propose a check from this escape" });
    expect(within(dialog).getByText("Old returnTo links stopped redirecting")).toBeInTheDocument();
    expect(within(dialog).getByText("task-1")).toBeInTheDocument();
    expect(within(dialog).getByText("claude · web/auth")).toBeInTheDocument();
    expect(within(dialog).getByText("Not set yet")).toBeInTheDocument();
    expect(within(dialog).getByText(/It must fail on the escaped merge and pass on the fix|must fail on the escaped merge and pass on the fix/)).toBeInTheDocument();
    // Nothing to send yet: the three required fields are empty.
    expect(within(dialog).getByRole("button", { name: "Propose the check" })).toBeDisabled();
    await fill(user, dialog);
    expect(within(dialog).getByRole("button", { name: "Propose the check" })).toBeEnabled();
    expect(posts(calls)).toHaveLength(0);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(posts(calls)).toHaveLength(0);
  });

  it("sends the person's own words with what they saw, and leaves the commit out when it is empty", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Propose a check" }));
    const dialog = await screen.findByRole("dialog");
    await fill(user, dialog);
    answers.check = PROPOSED;
    answers.detail = { status: 200, body: detail({ escape: row({ state: "confirmed", gap: "test_gap" }), check: checkRow({ state: "proposed" }) }) };
    await user.click(within(dialog).getByRole("button", { name: "Propose the check" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(post(calls).url).toMatch(/\/api\/crew\/escapes\/esc-1\/check$/);
    expect(bodyOf(post(calls).init)).toEqual({
      project: "p-app",
      seen: { state: "confirmed", gap: null, check: null },
      path: "tests/auth/return-to.test.ts",
      asserts: "An old returnTo link still redirects to its path",
      run: "npm test -- tests/auth/return-to.test.ts",
    });
    // The dialog closes and the page is read again: the proposal shows, and nothing started a worker.
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(await screen.findByRole("button", { name: "Accept: start a worker" })).toBeInTheDocument();
    expect(posts(calls)).toHaveLength(1);
  });

  it("names the commit when the person gives one", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Propose a check" }));
    const dialog = await screen.findByRole("dialog");
    await fill(user, dialog);
    await user.type(within(dialog).getByLabelText(/Commit it must pass on/), "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a");
    answers.check = PROPOSED;
    await user.click(within(dialog).getByRole("button", { name: "Propose the check" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(bodyOf(post(calls).init)).toMatchObject({ fix_commit: "e41a9c0a1b2c3d4e5f60718293a4b5c6d7e8f90a" });
  });

  it("a refusal is the service's own words, the dialog keeps what was typed and the page is read again", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Propose a check" }));
    const dialog = await screen.findByRole("dialog");
    await fill(user, dialog);
    answers.check = { status: 400, body: { code: "bad_request", message: "A check adds one test file, never a policy file." } };
    await user.click(within(dialog).getByRole("button", { name: "Propose the check" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("bad_request: A check adds one test file, never a policy file.");
    expect(within(dialog).getByLabelText("New test file")).toHaveValue("tests/auth/return-to.test.ts");
    expect(posts(calls)).toHaveLength(1);
    await waitFor(() => expect(calls.filter(({ url }) => /\/api\/crew\/escapes\/esc-1$/.test(url))).toHaveLength(2));
    // Try again sends the same words once more.
    answers.check = PROPOSED;
    await user.click(within(dialog).getByRole("button", { name: "Propose the check" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(2));
  });

  it("while the write is in flight the dialog is busy and a second press sends nothing", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Propose a check" }));
    const dialog = await screen.findByRole("dialog");
    await fill(user, dialog);
    answers.check = "never";
    const send = within(dialog).getByRole("button", { name: "Propose the check" });
    await user.click(send);
    await waitFor(() => expect(send).toBeDisabled());
    await user.click(send).catch(() => undefined);
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(posts(calls)).toHaveLength(1);
  });

  it("a stale refusal says look again", async () => {
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Propose a check" }));
    const dialog = await screen.findByRole("dialog");
    await fill(user, dialog);
    answers.check = { status: 409, body: { code: "stale", message: "This escape changed since you opened it." } };
    await user.click(within(dialog).getByRole("button", { name: "Propose the check" }));
    expect(await within(dialog).findByText(/This escape changed since you opened it\. Look again\./)).toBeInTheDocument();
    expect(posts(calls)).toHaveLength(1);
  });

  it("a withdrawn check is proposed again with what the person saw of it", async () => {
    answers.detail = { status: 200, body: detail({ escape: row({ state: "confirmed", gap: "test_gap", gap_by: "person" }), check: checkRow({ state: "withdrawn" }) }) };
    const user = userEvent.setup();
    render(<LiveEscapes view="detail" id="esc-1" />);
    await user.click(await screen.findByRole("button", { name: "Propose a check" }));
    const dialog = await screen.findByRole("dialog");
    await fill(user, dialog);
    answers.check = PROPOSED;
    await user.click(within(dialog).getByRole("button", { name: "Propose the check" }));
    await waitFor(() => expect(posts(calls)).toHaveLength(1));
    expect(bodyOf(post(calls).init)).toMatchObject({ seen: { state: "confirmed", gap: "test_gap", check: "withdrawn" } });
  });
});
