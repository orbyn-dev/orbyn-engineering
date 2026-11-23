import { describe, expect, it } from "vitest";
import { diskLowOf, diskSentence, floorText, freeText, NO_WORKER_RAN } from "./tool";

const GIB = 1024 ** 3;

describe("the disk refusal's numbers", () => {
  it("writes GiB to one decimal, rounded down, and whole MiB below a GiB", () => {
    expect(freeText(3_400_000_000)).toBe("3.1 GB");
    expect(freeText(GIB)).toBe("1.0 GB");
    expect(freeText(3.19 * GIB)).toBe("3.1 GB");
    expect(freeText(640 * 1024 * 1024)).toBe("640 MB");
    expect(freeText(GIB - 1)).toBe("1023 MB");
  });

  it("writes the floor in whole GB", () => {
    expect(floorText(5)).toBe("5 GB");
    expect(floorText(500)).toBe("500 GB");
    expect(floorText(7.9)).toBe("7 GB");
    expect(diskSentence(3_400_000_000, 5)).toBe("Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again.");
    expect(diskSentence(640 * 1024 * 1024, 5)).toBe("Not started: 640 MB free, Crew needs 5 GB. Free space and try again.");
  });

  it("reads the code and the numbers, never the sentence", () => {
    expect(diskLowOf({ code: "disk_low", message: "A different sentence.", free_bytes: 3_400_000_000, floor_gb: 5 })).toEqual({ free_bytes: 3_400_000_000, floor_gb: 5 });
    expect(diskLowOf({ code: "disk_low", message: "", free_bytes: 640 * 1024 * 1024, floor_gb: 5 })).toEqual({ free_bytes: 640 * 1024 * 1024, floor_gb: 5 });
    expect(diskLowOf({ code: "not_authorized", message: "Not started: 3.1 GB free, Crew needs 5 GB. Free space and try again." })).toBeNull();
    expect(diskLowOf({ code: "disk_low", message: "x", free_bytes: "3400000000", floor_gb: 5 })).toBeNull();
    expect(NO_WORKER_RAN).toBe("No worker ran. Nothing was removed to make room.");
  });
});
