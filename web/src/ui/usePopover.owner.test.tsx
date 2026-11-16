import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { beforeAll, expect, it, vi } from "vitest";
import { usePopover } from "./usePopover";

beforeAll(() => {
  // Shared cleanup resets hash to empty; establish that fragment without queuing a route event.
  const url = new URL(window.location.href);
  url.hash = "#";
  window.history.replaceState(window.history.state, "", url);
});

function Pop({ label, anchor, onClose, children, active = true, closing = false, rootKey }: { label: string; anchor: () => HTMLElement | null; onClose(): void; children?: ReactNode; active?: boolean; closing?: boolean; rootKey?: string }) {
  const root = useRef<HTMLDivElement>(null);
  usePopover({ kind: "fields", root, trigger: anchor, onClose, active, closing });
  return (
    <div key={rootKey} ref={root} role="dialog" aria-label={label}>
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

function NestedContent() {
  const [inner, setInner] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  return (
    <>
      <button ref={trigger} onClick={() => setInner(true)}>open nested</button>
      {inner ? createPortal(<Pop label="Nested" anchor={() => trigger.current} onClose={() => setInner(false)} />, document.body) : null}
    </>
  );
}

function NestedPage({ closeA, revision }: { closeA(): void; revision: number }) {
  return (
    <Pop label="A" anchor={() => null} onClose={closeA}>
      <span>{revision}</span>
      <NestedContent />
    </Pop>
  );
}

it("keeps a nested portal's owner through an ancestor render, so pressing it does not close the parent", async () => {
  const closeA = vi.fn();
  const user = userEvent.setup();
  const view = render(<NestedPage closeA={closeA} revision={0} />);
  await user.click(screen.getByRole("button", { name: "open nested" }));
  const parent = screen.getByRole("dialog", { name: "A" });
  const nested = screen.getByRole("dialog", { name: "Nested" });
  expect(nested).toHaveAttribute("data-popover-owner", parent.getAttribute("data-popover-root"));
  expect(closeA, "opening the nested portal keeps its parent open").not.toHaveBeenCalled();

  view.rerender(<NestedPage closeA={closeA} revision={1} />);
  expect(closeA, "the ancestor render keeps its parent open").not.toHaveBeenCalled();
  expect(nested, "ownership survives before the nested pointer press").toHaveAttribute("data-popover-owner", parent.getAttribute("data-popover-root"));
  fireEvent.pointerDown(screen.getByRole("button", { name: "Nested inside" }));
  expect(closeA).not.toHaveBeenCalled();
  expect(nested).toHaveAttribute("data-popover-owner", parent.getAttribute("data-popover-root"));
});

function OwnedRoot({ close, active = true, closing = false, rootKey }: { close(): void; active?: boolean; closing?: boolean; rootKey?: string }) {
  const trigger = useRef<HTMLButtonElement>(null);
  return (
    <>
      <Pop label="Owner" anchor={() => null} onClose={() => undefined}>
        <button ref={trigger}>owned trigger</button>
      </Pop>
      <Pop label="Owned" anchor={() => trigger.current} onClose={close} active={active} closing={closing} rootKey={rootKey} />
    </>
  );
}

it("clears markers and listeners while inactive, then restores ownership when reactivated", () => {
  const close = vi.fn();
  const view = render(<OwnedRoot close={close} />);
  const element = screen.getByRole("dialog", { name: "Owned" });
  const owner = screen.getByRole("dialog", { name: "Owner" }).getAttribute("data-popover-root");
  expect(element).toHaveAttribute("data-popover-root");
  expect(element).toHaveAttribute("data-popover-owner", owner);
  view.rerender(<OwnedRoot close={close} active={false} />);
  expect(element).not.toHaveAttribute("data-popover-root");
  expect(element).not.toHaveAttribute("data-popover-owner");
  fireEvent.pointerDown(document.body);
  expect(close).not.toHaveBeenCalled();
  view.rerender(<OwnedRoot close={close} />);
  expect(element).toHaveAttribute("data-popover-root");
  expect(element).toHaveAttribute("data-popover-owner", owner);
});

it("releases a replaced root and cleans the current root on unmount", () => {
  const close = vi.fn();
  const view = render(<OwnedRoot close={close} rootKey="first" />);
  const previous = screen.getByRole("dialog", { name: "Owned" });
  const id = previous.getAttribute("data-popover-root");
  const owner = previous.getAttribute("data-popover-owner");
  view.rerender(<OwnedRoot close={close} rootKey="second" />);
  const current = screen.getByRole("dialog", { name: "Owned" });
  expect(current).not.toBe(previous);
  expect(previous).not.toHaveAttribute("data-popover-root");
  expect(previous).not.toHaveAttribute("data-popover-owner");
  expect(current).toHaveAttribute("data-popover-root", id);
  expect(current).toHaveAttribute("data-popover-owner", owner);
  view.unmount();
  expect(current).not.toHaveAttribute("data-popover-root");
  expect(current).not.toHaveAttribute("data-popover-owner");
});

it("keeps live markers through StrictMode effect replay and clears them on unmount", () => {
  const view = render(<StrictMode><OwnedRoot close={() => undefined} /></StrictMode>);
  const element = screen.getByRole("dialog", { name: "Owned" });
  const owner = screen.getByRole("dialog", { name: "Owner" }).getAttribute("data-popover-root");
  expect(element).toHaveAttribute("data-popover-root");
  expect(element).toHaveAttribute("data-popover-owner", owner);
  view.unmount();
  expect(element).not.toHaveAttribute("data-popover-root");
  expect(element).not.toHaveAttribute("data-popover-owner");
});

it("keeps closing markers while dismiss listeners are inactive, then clears them on unmount", () => {
  const close = vi.fn();
  const view = render(<OwnedRoot close={close} />);
  const element = screen.getByRole("dialog", { name: "Owned" });
  const id = element.getAttribute("data-popover-root");
  const owner = element.getAttribute("data-popover-owner");
  view.rerender(<OwnedRoot close={close} closing />);
  expect(element).toHaveAttribute("data-popover-root", id);
  expect(element).toHaveAttribute("data-popover-owner", owner);
  fireEvent.pointerDown(document.body);
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(close).not.toHaveBeenCalled();
  view.unmount();
  expect(element).not.toHaveAttribute("data-popover-root");
  expect(element).not.toHaveAttribute("data-popover-owner");
});

it("does not erase markers that another hook owns when it unmounts", () => {
  const view = render(<Pop label="A" anchor={() => null} onClose={() => undefined} />);
  const element = screen.getByRole("dialog", { name: "A" });
  element.setAttribute("data-popover-root", "another-root");
  element.setAttribute("data-popover-owner", "another-owner");
  view.unmount();
  expect(element).toHaveAttribute("data-popover-root", "another-root");
  expect(element).toHaveAttribute("data-popover-owner", "another-owner");
});

function MovingTrigger({ owner, closeA, closeB }: { owner: "A" | "B" | "outside"; closeA(): void; closeB(): void }) {
  const first = useRef<HTMLButtonElement>(null);
  const second = useRef<HTMLButtonElement>(null);
  const outer = useRef<HTMLButtonElement>(null);
  return (
    <>
      <Pop label="A" anchor={() => null} onClose={closeA}>
        <button ref={first}>A trigger</button>
      </Pop>
      <Pop label="B" anchor={() => null} onClose={closeB}>
        <button ref={second}>B trigger</button>
      </Pop>
      <button ref={outer}>outside trigger</button>
      <Pop label="Child" anchor={() => owner === "A" ? first.current : owner === "B" ? second.current : outer.current} onClose={() => undefined} />
    </>
  );
}

it("changes owner from A to B and removes it when the trigger moves outside", () => {
  const closeA = vi.fn();
  const closeB = vi.fn();
  const view = render(<MovingTrigger owner="A" closeA={closeA} closeB={closeB} />);
  const parent = screen.getByRole("dialog", { name: "A" });
  const other = screen.getByRole("dialog", { name: "B" });
  const child = screen.getByRole("dialog", { name: "Child" });
  expect(child).toHaveAttribute("data-popover-owner", parent.getAttribute("data-popover-root"));
  view.rerender(<MovingTrigger owner="B" closeA={closeA} closeB={closeB} />);
  expect(child).toHaveAttribute("data-popover-owner", other.getAttribute("data-popover-root"));
  fireEvent.pointerDown(screen.getByRole("button", { name: "Child inside" }));
  expect(closeA).toHaveBeenCalledOnce();
  expect(closeB).not.toHaveBeenCalled();
  view.rerender(<MovingTrigger owner="outside" closeA={closeA} closeB={closeB} />);
  expect(child).not.toHaveAttribute("data-popover-owner");
  fireEvent.pointerDown(screen.getByRole("button", { name: "Child inside" }));
  expect(closeB).toHaveBeenCalledOnce();
});
