import { describe, expect, it } from "vitest";
import { MAX_IMAGE_CHARS, clampAngle, controlsFor, hingeLabel, plain, platformLabel, postureLabel, screenshotSrc, stateLabel } from "./model";

const ALL = ["screenshot", "rotate", "button", "power_off", "fold", "unfold", "hinge", "hinge_preset"] as const;

describe("text the service chose", () => {
  it("is cut, and never markup", () => {
    expect(plain("a".repeat(200), 10)).toBe(`${"a".repeat(10)}…`);
    expect(plain(7 as unknown as string)).toBe("");
    expect(plain("<img src=x onerror=alert(1)>")).toBe("<img src=x onerror=alert(1)>");
  });

  it("names a service-chosen key without reaching the prototype", () => {
    for (const key of ["__proto__", "constructor", "toString", "hasOwnProperty"]) {
      expect(stateLabel(key)).toBe("Unknown");
      expect(platformLabel(key)).toBe("Unknown");
      expect(postureLabel(key)).toBe(key);
    }
  });

  it("writes the three states, the postures and a missing angle", () => {
    expect([stateLabel("shutdown"), stateLabel("booting"), stateLabel("booted")]).toEqual(["Shut down", "Starting", "Running"]);
    expect(postureLabel(null)).toBe("Not reported");
    expect(postureLabel("tent")).toBe("Tent");
    expect(postureLabel("x".repeat(60))).toBe(`${"x".repeat(24)}…`);
    expect(hingeLabel(null)).toBe("not reported");
    expect(hingeLabel(129.6)).toBe("130°");
  });
});

describe("controlsFor", () => {
  it("gives iOS screenshot and power off only, with Rotate and Home planned and no fold or hinge", () => {
    const controls = controlsFor({ platform: "ios", foldable: true, state: "booted" }, ALL);
    expect(controls).toMatchObject({ screenshot: true, powerOff: true, buttons: false, rotate: false, fold: false, hinge: false });
    expect(controls.planned).toEqual(["Rotate", "Home"]);
  });

  it("gives Android what the service lists, and fold only on a foldable that is running", () => {
    expect(controlsFor({ platform: "android", foldable: true, state: "booted" }, ALL)).toMatchObject({ buttons: true, rotate: true, fold: true, hinge: true });
    expect(controlsFor({ platform: "android", foldable: false, state: "booted" }, ALL)).toMatchObject({ buttons: true, fold: false, hinge: false });
    expect(controlsFor({ platform: "android", foldable: true, state: "shutdown" }, ALL)).toMatchObject({ screenshot: false, powerOff: false, fold: false });
    expect(controlsFor({ platform: "android", foldable: true, state: "booted" }, ["screenshot"])).toMatchObject({ screenshot: true, rotate: false, buttons: false, fold: false });
  });
});

describe("screenshotSrc", () => {
  it("takes a PNG within the cap and nothing else", () => {
    expect(screenshotSrc("iVBORw0KGgoAAAA=")).toBe("data:image/png;base64,iVBORw0KGgoAAAA=");
    expect(screenshotSrc("PHN2Zz4=")).toBeNull();
    expect(screenshotSrc("iVBORw0KGgo\"onerror=x")).toBeNull();
    expect(screenshotSrc(undefined)).toBeNull();
    expect(screenshotSrc(`iVBORw0KGgo${"A".repeat(MAX_IMAGE_CHARS)}`)).toBeNull();
  });
});

it("clamps a hinge angle to 1 to 179", () => {
  expect([clampAngle(0), clampAngle(180), clampAngle(90.4), clampAngle(-5)]).toEqual([1, 179, 90, 1]);
});
