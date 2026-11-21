import { describe, expect, it } from "vitest";
import { ApiFailure } from "../../api/client";
import { displayText, runRefusal } from "./refusal";

describe("runRefusal", () => {
  it("acts on the code of a 409, not on the message", () => {
    expect(runRefusal(new ApiFailure("engine_mode_refused", "x", 409))).toEqual({ kind: "mode", message: "x" });
    expect(runRefusal(new ApiFailure("engine_notice_required", "Cursor needs a notice.", 409))).toEqual({ kind: "notice", message: null });
  });

  it("ignores any other failure, including the same code on another status", () => {
    expect(runRefusal(new ApiFailure("thread_busy", "busy", 409))).toBeNull();
    expect(runRefusal(new ApiFailure("engine_mode_refused", "x", 400))).toBeNull();
    expect(runRefusal(new ApiFailure("engine_mode_refused", "x", 403))).toBeNull();
    expect(runRefusal(new Error("engine_mode_refused"))).toBeNull();
    expect(runRefusal("engine_mode_refused")).toBeNull();
  });

  it("keeps a refusal with no sentence, so the screen can use its own", () => {
    expect(runRefusal(new ApiFailure("engine_mode_refused", "   ", 409))).toEqual({ kind: "mode", message: null });
  });
});

describe("displayText", () => {
  it("is one bounded run of text and never markup that was interpreted", () => {
    expect(displayText("  a\n\n b\t c ")).toBe("a b c");
    expect(displayText("<img src=x onerror=alert(1)>")).toBe("<img src=x onerror=alert(1)>");
    expect(displayText("x".repeat(500))).toHaveLength(280);
    expect(displayText(undefined)).toBeNull();
    expect(displayText(7)).toBeNull();
  });
});
