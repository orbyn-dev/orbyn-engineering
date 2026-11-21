import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
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
