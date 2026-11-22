import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure, NETWORK_ERROR } from "../../../api/client";
import { PERSON_HEADER } from "../../../api/personKey";
import { realPhotoConversionApi } from "./photoConversionClient";

const file = (name = "IMG_4021.HEIC", type = "image/heic") => new File(["x"], name, { type });
const signal = () => new AbortController().signal;

const converted = (over: Record<string, unknown> = {}) =>
  new Response(JSON.stringify({
    id: "a1",
    name: "IMG_4021.jpg",
    size: 2_202_009,
    original_name: "IMG_4021.HEIC",
    original_size: 3_000_000,
    action: "converted",
    ...over,
  }), { status: 200 });

const refused = (status: number, code: string) =>
  new Response(JSON.stringify({ code, message: `${code} words` }), { status });

beforeEach(() => sessionStorage.setItem("orbyn.token", "tok"));
afterEach(() => vi.unstubAllGlobals());

describe("realPhotoConversionApi", () => {
  it("posts the raw photo to the thread's convert route with its name as the label, the session and no person key", async () => {
    const fetchMock = vi.fn(async () => converted());
    vi.stubGlobal("fetch", fetchMock);
    const controller = new AbortController();
    const photo = file("IMG 4021 #1.HEIC", "image/heic");

    await expect(realPhotoConversionApi.convert("t/1", photo, controller.signal)).resolves.toEqual({
      id: "a1",
      name: "IMG_4021.jpg",
      size: 2_202_009,
      original_name: "IMG_4021.HEIC",
      original_size: 3_000_000,
      action: "converted",
    });

    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe("/api/threads/t%2F1/attachments/convert?name=IMG%204021%20%231.HEIC");
    expect(init.method).toBe("POST");
    expect(init.credentials).toBe("same-origin");
    expect(init.body).toBe(photo);
    expect(init.signal).toBe(controller.signal);
    const headers = init.headers as Record<string, string>;
    expect(headers.Authorization).toBe("Bearer tok");
    expect(headers["Content-Type"]).toBe("image/heic");
    expect(headers[PERSON_HEADER]).toBeUndefined();
  });

  it.each([
    [422, "unreadable_image"],
    [422, "still_too_large"],
    [413, "file_too_large"],
    [415, "unsupported_type"],
  ])("keeps the service's %s refusal code for %s", async (status, code) => {
    vi.stubGlobal("fetch", vi.fn(async () => refused(status, code)));
    const failure = await realPhotoConversionApi.convert("t1", file(), signal()).catch((caught: unknown) => caught);
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(failure).toMatchObject({ code, status, message: `${code} words` });
  });

  it("reads the generated bigint sizes as numbers", async () => {
    const answer = new Response(null, { status: 200 });
    vi.spyOn(answer, "json").mockResolvedValue({
      id: "a1",
      name: "IMG_4021.png",
      size: 2_202_009n,
      original_name: "IMG_4021.png",
      original_size: 3_000_000n,
      action: "none",
    });
    vi.stubGlobal("fetch", vi.fn(async () => answer));

    const photo = await realPhotoConversionApi.convert("t1", file("IMG_4021.png", "image/png"), signal());
    expect(photo.size).toBe(2_202_009);
    expect(photo.original_size).toBe(3_000_000);
    expect(typeof photo.size).toBe("number");
    expect(typeof photo.original_size).toBe("number");
    expect(photo.action).toBe("none");
  });

  it("rethrows an abort instead of turning it into a network failure", async () => {
    const aborted = new DOMException("Aborted", "AbortError");
    vi.stubGlobal("fetch", vi.fn(async () => { throw aborted; }));
    await expect(realPhotoConversionApi.convert("t1", file(), signal())).rejects.toBe(aborted);
  });

  it("maps a request that never reached the service to NETWORK_ERROR", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("Failed to fetch"); }));
    await expect(realPhotoConversionApi.convert("t1", file(), signal())).rejects.toMatchObject({
      code: NETWORK_ERROR,
      status: 0,
    });
  });
});
