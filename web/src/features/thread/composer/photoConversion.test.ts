import { afterEach, describe, expect, it } from "vitest";
import { conversionKindOf, MAX_IMAGE_BYTES, photoConversionApi, setPhotoConversionApi, type PhotoConversionApi } from "./photoConversion";

const file = (name: string, type: string, size = 10) => {
  const made = new File(["x"], name, { type });
  Object.defineProperty(made, "size", { value: size });
  return made;
};

afterEach(() => setPhotoConversionApi(null));

describe("photo conversion", () => {
  it("is absent until a client is installed, so a build with no conversion route sends nothing", () => {
    expect(photoConversionApi()).toBeNull();
  });

  it("installs and removes a client", () => {
    const api: PhotoConversionApi = { convert: () => Promise.reject(new Error("unused")) };
    setPhotoConversionApi(api);
    expect(photoConversionApi()).toBe(api);
    setPhotoConversionApi(null);
    expect(photoConversionApi()).toBeNull();
  });

  it.each([
    ["a HEIC by type", file("a.bin", "image/heic"), "convert"],
    ["a HEIF by type", file("a.bin", "image/heif"), "convert"],
    ["a HEIC by name when the system gives no type", file("IMG_1.HEIC", ""), "convert"],
    ["a PNG over the limit", file("a.png", "image/png", MAX_IMAGE_BYTES + 1), "shrink"],
    ["a JPEG over the limit", file("a.jpg", "image/jpeg", MAX_IMAGE_BYTES + 1), "shrink"],
    ["a PNG at the limit", file("a.png", "image/png", MAX_IMAGE_BYTES), null],
    ["a text file over the limit", file("a.txt", "text/plain", MAX_IMAGE_BYTES * 2), null],
    ["a GIF over the limit, which cannot be shrunk without losing frames", file("a.gif", "image/gif", MAX_IMAGE_BYTES + 1), null],
  ])("classifies %s", (_name, input, expected) => {
    expect(conversionKindOf(input)).toBe(expected);
  });
});
