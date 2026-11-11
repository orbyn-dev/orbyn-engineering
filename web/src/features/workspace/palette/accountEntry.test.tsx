import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import { CommandPalette } from "./CommandPalette";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

async function open() {
  const user = userEvent.setup();
  render(<CommandPalette start="root" onClose={() => undefined} />);
  await user.keyboard("{Escape}").catch(() => undefined);
  await user.type(await screen.findByRole("combobox"), "Account");
  return user;
}

describe("the Account entry in the command palette", () => {
  it("is not offered while the gate is off", async () => {
    await open();
    await waitFor(() => expect(mock.handle("GET", "/api/account").body).toMatchObject({ available: false }));
    expect(screen.queryByRole("button", { name: /^Account/ })).toBeNull();
  });

  it("is offered while the gate is on", async () => {
    mock.reset("account");
    await open();
    expect(await screen.findByRole("button", { name: /^Account/ })).toBeVisible();
  });
});
