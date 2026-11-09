import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Project } from "../../../api/types";
import { popupPlace, POPUP_HEIGHT, POPUP_WIDTH, ProjectFilter } from "./ProjectFilter";

const window1280 = { width: 1280, height: 800 };

describe("popupPlace", () => {
  it("opens under the trigger, left edges aligned", () => {
    expect(popupPlace({ left: 173, bottom: 112 }, window1280)).toEqual({ left: 173, top: 116 });
  });
  it("keeps the popup inside the window sideways, never flush at x=0", () => {
    expect(popupPlace({ left: 1200, bottom: 100 }, window1280).left).toBe(1280 - POPUP_WIDTH - 8);
    expect(popupPlace({ left: -50, bottom: 100 }, window1280).left).toBe(8);
  });
  it("keeps it inside the window vertically too", () => {
    expect(popupPlace({ left: 100, bottom: 790 }, window1280).top).toBe(800 - POPUP_HEIGHT - 8);
    expect(popupPlace({ left: 100, bottom: -40 }, window1280).top).toBe(8);
  });
});

describe("ProjectFilter", () => {
  it("follows its trigger when the window is resized", () => {
    const projects = [{ id: "p1", name: "alpha", path: "/a" }] as unknown as Project[];
    render(<ProjectFilter projects={projects} />);
    const trigger = screen.getByRole("button", { name: /Filter threads by project/ });
    let bottom = 100;
    trigger.getBoundingClientRect = () => ({ left: 50, right: 70, top: bottom - 20, bottom, width: 20, height: 20, x: 50, y: bottom - 20, toJSON: () => ({}) });
    fireEvent.click(trigger);
    const popup = () => document.querySelector(".filter-popup") as HTMLElement;
    expect(popup().style.top).toBe("104px");
    bottom = 240;
    act(() => void window.dispatchEvent(new Event("resize")));
    expect(popup().style.top).toBe("244px");
  });
});
