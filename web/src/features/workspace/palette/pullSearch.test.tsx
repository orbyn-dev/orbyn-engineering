import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { installMockFetch } from "../../../test-support";
import { CommandPalette } from "./CommandPalette";

const KEY = "orbyn.palette.frecency";

beforeEach(() => {
  installMockFetch();
  window.localStorage.clear();
});

async function firstPull() {
  const response = await fetch("/api/pulls?state=open&q=");
  const first = (await response.json()).pulls[0] as { project_id: string; pull: { title: string; number: number; head: string } };
  const word = first.pull.title.split(/\s+/).find((one) => one.length > 3) ?? first.pull.title;
  return { ...first, word };
}

async function search(text: string) {
  const user = userEvent.setup();
  render(<CommandPalette start="root" onClose={() => undefined} />);
  await user.type(await screen.findByRole("combobox"), text);
  return user;
}

describe("pull requests in the command palette", () => {
  it("lists a pull request by a word of its title and writes nothing until one is picked", async () => {
    const first = await firstPull();
    await search(first.word);
    expect(await screen.findByText("Pull requests")).toBeVisible();
    expect(screen.getByRole("button", { name: new RegExp(`#${first.pull.number}$`) })).toBeVisible();
    expect(window.localStorage.getItem(KEY)).toBeNull();
  });

  it("remembers a picked pull request by id and time only", async () => {
    const first = await firstPull();
    const user = await search(first.word);
    await user.click(await screen.findByRole("button", { name: new RegExp(`#${first.pull.number}$`) }));
    const stored = window.localStorage.getItem(KEY) ?? "";
    expect(JSON.parse(stored)).toHaveProperty([`pull:${first.project_id}:${first.pull.number}`]);
    expect(stored).not.toContain(first.pull.title);
    expect(stored).not.toContain(first.pull.head);
  });
});

/** The pull request list answers as the test says; every other call goes to the mock service. */
function pullsAnswer(answer: () => Promise<Response>) {
  const real = globalThis.fetch;
  const calls: string[] = [];
  vi.stubGlobal("fetch", vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.includes("/api/pulls")) { calls.push(url); return answer(); }
    return real(input, init);
  }));
  return calls;
}

describe("while the pull request list is read, and when it cannot be (palette-prs-loading, palette-prs-unreadable)", () => {
  it("says it is looking under the Pull requests heading, with the other groups usable, and says nothing before two characters", async () => {
    pullsAnswer(() => new Promise<Response>(() => undefined));
    const user = userEvent.setup();
    render(<CommandPalette start="root" onClose={() => undefined} />);
    const field = await screen.findByRole("combobox");
    await user.type(field, "n");
    expect(screen.queryByText("Looking for pull requests…")).toBeNull();
    await user.type(field, "e");
    expect(await screen.findByText("Looking for pull requests…")).toBeVisible();
    expect(screen.getByText("Pull requests")).toBeVisible();
    expect(screen.queryByText("No results")).toBeNull();
    expect(screen.getAllByRole("button", { name: /New thread/ })[0]).toBeVisible();
  });

  it("says what failed and that the rest works, with Retry that reads the list once more", async () => {
    let fail = true;
    const mock = pullsAnswer(async () => (fail ? new Response(JSON.stringify({ code: "offline", message: "offline" }), { status: 503 }) : new Response(JSON.stringify({ pulls: [] }), { status: 200 })));
    const user = userEvent.setup();
    render(<CommandPalette start="root" onClose={() => undefined} />);
    await user.type(await screen.findByRole("combobox"), "ne");
    const line = await screen.findByText("Couldn’t read pull requests. Threads and commands still work.");
    expect(line).toBeVisible();
    expect(screen.getAllByRole("button", { name: /New thread/ })[0]).toBeVisible();
    const before = mock.length;
    fail = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(mock.length).toBe(before + 1));
    await waitFor(() => expect(screen.queryByText(/Couldn’t read pull requests/)).toBeNull());
  });

  it("never shows the service's own words for the failure", async () => {
    pullsAnswer(async () => new Response(JSON.stringify({ code: "x", message: "<img src=x onerror=alert(1)> token ghp_secret" }), { status: 500 }));
    const user = userEvent.setup();
    render(<CommandPalette start="root" onClose={() => undefined} />);
    await user.type(await screen.findByRole("combobox"), "ne");
    await screen.findByText(/Couldn’t read pull requests/);
    expect(document.body.textContent).not.toContain("ghp_secret");
  });
});
