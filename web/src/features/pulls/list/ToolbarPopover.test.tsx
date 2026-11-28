import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, it } from "vitest";
import { ToolbarPopover } from "./ToolbarPopover";

/** An anchor button whose panel holds a disabled field and a link: nothing enabled takes focus on open. */
function Panel() {
  const [open, setOpen] = useState<HTMLElement | null>(null);
  return (
    <>
      <button type="button" onClick={(event) => setOpen(event.currentTarget)}>Open</button>
      {open ? (
        <ToolbarPopover anchor={open} label="Linked threads" onClose={() => setOpen(null)}>
          {() => (
            <div>
              <input aria-label="Filter" disabled />
              <a href="#/threads/thread-1">Thread</a>
            </div>
          )}
        </ToolbarPopover>
      ) : null}
    </>
  );
}

it("takes focus itself when its only input is disabled", async () => {
  const user = userEvent.setup();
  render(<Panel />);
  await user.click(screen.getByRole("button", { name: "Open" }));
  // The disabled field cannot take focus, so the panel itself does: its label is announced and Tab reaches the link.
  expect(await screen.findByRole("dialog", { name: "Linked threads" })).toHaveFocus();
});
