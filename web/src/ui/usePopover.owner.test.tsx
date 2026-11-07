import { fireEvent, render, screen } from "@testing-library/react";
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

function Page({ closeA, withOther }: { closeA(): void; withOther: boolean }) {
  const [inner, setInner] = useState(false);
  const triggerInner = useRef<HTMLButtonElement>(null);
  const triggerOther = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button ref={triggerOther}>other trigger</button>
      <Pop label="A" anchor={() => null} onClose={closeA}>
        <button ref={triggerInner} onClick={() => setInner(true)}>
          open inner
        </button>
      </Pop>
      {inner ? <Pop label="Inner" anchor={() => triggerInner.current} onClose={() => setInner(false)} /> : null}
      {withOther ? <Pop label="Other" anchor={() => triggerOther.current} onClose={() => undefined} /> : null}
    </>
  );
}

it("stays open for a press inside a popover opened from it", async () => {
  const closeA = vi.fn();
  const user = userEvent.setup();
  render(<Page closeA={closeA} withOther={false} />);
  await user.click(screen.getByRole("button", { name: "open inner" }));
  fireEvent.pointerDown(screen.getByRole("button", { name: "Inner inside" }));
  expect(closeA).not.toHaveBeenCalled();
});

it("closes for a press inside a second popover that was not opened from it", () => {
  const closeA = vi.fn();
  render(<Page closeA={closeA} withOther />);
  fireEvent.pointerDown(screen.getByRole("button", { name: "Other inside" }));
  expect(closeA).toHaveBeenCalled();
});
