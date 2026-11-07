import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useRef, useState, type ReactNode } from "react";
import { expect, it, vi } from "vitest";
import { usePopover } from "./usePopover";

function Pop({ label, anchor, onClose, children }: { label: string; anchor: () => HTMLElement | null; onClose(): void; children?: ReactNode }) {
  const root = useRef<HTMLDivElement>(null);
  usePopover({ kind: "fields", root, trigger: anchor, onClose });
  return (
    <div ref={root} role="dialog" aria-label={label}>
      <button>{`${label} inside`}</button>
      {children}
    </div>
  );
}

it("stays open for a press inside a popover opened from it, and closes for an unrelated popover", async () => {
  const closeA = vi.fn();
  const user = userEvent.setup();
  function Page() {
    const [inner, setInner] = useState(false);
    const [other, setOther] = useState(true);
    const triggerInner = useRef<HTMLButtonElement>(null);
    const triggerOther = useRef<HTMLButtonElement>(null);
    return (
      <>
        <button ref={triggerOther} onClick={() => setOther(true)}>
          open other
        </button>
        <Pop label="A" anchor={() => null} onClose={closeA}>
          <button ref={triggerInner} onClick={() => setInner(true)}>
            open inner
          </button>
        </Pop>
        {inner ? <Pop label="Inner" anchor={() => triggerInner.current} onClose={() => setInner(false)} /> : null}
        {other ? <Pop label="Other" anchor={() => triggerOther.current} onClose={() => setOther(false)} /> : null}
      </>
    );
  }
  render(<Page />);
  await user.click(screen.getByRole("button", { name: "open inner" }));
  await user.click(screen.getByRole("button", { name: "Inner inside" }));
  expect(closeA).not.toHaveBeenCalled();
  // An unrelated popover that is already open is not A's: pressing in it leaves A.
  await user.click(screen.getByRole("button", { name: "Other inside" }));
  expect(closeA).toHaveBeenCalled();
});
