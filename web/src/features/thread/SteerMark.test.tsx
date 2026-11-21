import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TimelineRow } from "./items";
import type { MessageItem } from "../../product/timeline/types";

const row = (steer: MessageItem["steer"]) => (
  <TimelineRow
    live
    tools={[]}
    entry={{ kind: "message", key: "steer:m1", seq: 0, ts: "", runId: null, role: "user", text: "Skip the admin pages", done: true, ...(steer ? { steer } : {}) }}
  />
);

describe("mid-turn message marks", () => {
  it.each([
    ["pending", "Sending…"],
    ["delivered", "Delivered"],
    ["queued", "Queued for the next turn"],
    ["failed", "Not delivered · moved to the queue"],
  ] as const)("reads %s under the bubble", (state, text) => {
    render(row(state));
    expect(screen.getByRole("status")).toHaveTextContent(text);
    expect(screen.getByText("Skip the admin pages")).toBeInTheDocument();
  });

  it("draws nothing for an ordinary message", () => {
    render(row(undefined));
    expect(screen.queryByRole("status")).toBeNull();
  });
});
