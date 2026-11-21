import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ApiFailure, api } from "../../../api/client";
import type { Thread } from "../../../api/types";
import { MAX_IMAGE_BYTES, setPhotoConversionApi } from "./photoConversion";
import { useAttachments } from "./useAttachments";

const thread = { id: "t1" } as Thread;
const sized = (name: string, type: string, size: number) => {
  const file = new File(["x"], name, { type });
  Object.defineProperty(file, "size", { value: size });
  return file;
};

afterEach(() => {
  setPhotoConversionApi(null);
  vi.restoreAllMocks();
});

it("uploads a HEIC photo as it always did when no conversion client is installed", async () => {
  const upload = vi.spyOn(api, "uploadAttachment").mockResolvedValue({ id: "a", name: "IMG_1.HEIC" } as never);
  const { result } = renderHook(() => useAttachments(thread, vi.fn()));
  act(() => result.current.upload([sized("IMG_1.HEIC", "image/heic", 5)]));
  await waitFor(() => expect(result.current.attachments[0]?.id).toBe("a"));
  expect(upload).toHaveBeenCalledTimes(1);
  expect(result.current.attachments[0]?.conversion).toBeUndefined();
});

it("converts a HEIC photo through the client, holds send while it works and shows the new name and size", async () => {
  let finish!: (value: never) => void;
  const convert = vi.fn().mockReturnValue(new Promise((done) => { finish = done as never; }));
  setPhotoConversionApi({ convert });
  const upload = vi.spyOn(api, "uploadAttachment");
  const { result } = renderHook(() => useAttachments(thread, vi.fn()));
  act(() => result.current.upload([sized("IMG_4021.HEIC", "image/heic", 3_000_000)]));
  expect(result.current.uploading).toBe(true);
  expect(result.current.attachments[0]?.conversion).toMatchObject({ kind: "convert", phase: "converting", note: "Converting photo…" });
  await act(async () => finish({ id: "p", name: "IMG_4021.jpg", size: 2_202_009, original_name: "IMG_4021.HEIC", original_size: 3_000_000, action: "converted" } as never));
  expect(result.current.uploading).toBe(false);
  expect(result.current.attachments[0]).toMatchObject({ id: "p", name: "IMG_4021.jpg", conversion: { phase: "converted", note: "Converted to JPEG · 2.1 MB" } });
  expect(upload).not.toHaveBeenCalled();
});

it("says shrinking for an oversized PNG and what it came to", async () => {
  const convert = vi.fn().mockResolvedValue({ id: "p", name: "IMG_4022.png", size: 5_033_165, original_name: "IMG_4022.png", original_size: 14 * 1024 * 1024, action: "shrunk" });
  setPhotoConversionApi({ convert });
  const { result } = renderHook(() => useAttachments(thread, vi.fn()));
  act(() => result.current.upload([sized("IMG_4022.png", "image/png", MAX_IMAGE_BYTES + 4 * 1024 * 1024)]));
  expect(result.current.attachments[0]?.conversion?.note).toBe("Shrinking photo · 14 MB…");
  await waitFor(() => expect(result.current.attachments[0]?.conversion?.phase).toBe("converted"));
  expect(result.current.attachments[0]?.conversion?.note).toBe("Shrunk from 14 MB to 4.8 MB");
});

it.each([
  ["unreadable_image", "Couldn’t convert this photo. It may be damaged."],
  ["still_too_large", "Still over 10 MiB after shrinking. Pick a smaller photo."],
])("fails a photo with %s in one sentence, and retries it", async (code, sentence) => {
  const convert = vi.fn().mockRejectedValueOnce(new ApiFailure(code, "server words", 422)).mockResolvedValueOnce({
    id: "p", name: "a.jpg", size: 1, original_name: "a.HEIC", original_size: 2, action: "converted" });
  setPhotoConversionApi({ convert });
  const { result } = renderHook(() => useAttachments(thread, vi.fn()));
  act(() => result.current.upload([sized("a.HEIC", "image/heic", 2)]));
  await waitFor(() => expect(result.current.attachments[0]?.failure).toBe(sentence));
  expect(result.current.uploading).toBe(true);
  act(() => result.current.startUpload(result.current.attachments[0]!));
  await waitFor(() => expect(result.current.attachments[0]?.id).toBe("p"));
  expect(result.current.attachments[0]?.failure).toBeNull();
});

it("cancels a conversion: the chip goes and a late answer is ignored", async () => {
  let finish!: (value: never) => void;
  let signal!: AbortSignal;
  setPhotoConversionApi({ convert: (_id, _file, given) => { signal = given; return new Promise((done) => { finish = done as never; }); } });
  const { result } = renderHook(() => useAttachments(thread, vi.fn()));
  act(() => result.current.upload([sized("a.HEIC", "image/heic", 2)]));
  act(() => result.current.cancel(result.current.attachments[0]!));
  expect(signal.aborted).toBe(true);
  expect(result.current.attachments).toEqual([]);
  await act(async () => finish({ id: "late", name: "x.jpg", size: 1, original_name: "a", original_size: 1, action: "converted" } as never));
  expect(result.current.attachments).toEqual([]);
});
