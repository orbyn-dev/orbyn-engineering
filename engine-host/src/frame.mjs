export const PROTOCOL = 1;
export const MAX_FRAME_BYTES = 8 * 1024 * 1024;
const MAX_CAUSE_DEPTH = 8;

function safeGet(target, key) {
  try {
    return Reflect.get(target, key);
  } catch {
    return undefined;
  }
}

function safeString(value) {
  if (typeof value === "string") return value;
  try {
    return String(value);
  } catch {
    return "[Unserializable error]";
  }
}

export function serializeError(cause, depth = 0, seen = new Set()) {
  if (typeof cause === "object" && cause !== null) {
    if (seen.has(cause)) return { name: "Error", message: "[Circular error cause]" };
    seen.add(cause);
    const name = safeGet(cause, "name");
    const message = safeGet(cause, "message");
    const stack = safeGet(cause, "stack");
    const code = safeGet(cause, "code");
    const status = safeGet(cause, "status");
    const kind = safeGet(cause, "kind");
    const inner = depth < MAX_CAUSE_DEPTH ? safeGet(cause, "cause") : undefined;
    return {
      name: typeof name === "string" && name.length > 0 ? name : "Error",
      message: typeof message === "string" ? message : safeString(cause),
      ...(typeof stack === "string" ? { stack } : {}),
      ...(typeof code === "string" ? { code } : {}),
      ...(typeof status === "number" ? { status } : {}),
      ...(typeof kind === "string" ? { kind } : {}),
      ...(inner === undefined ? {} : { cause: serializeError(inner, depth + 1, seen) }),
    };
  }
  return { name: "Error", message: safeString(cause) };
}

function replacer(_key, value) {
  if (typeof value === "bigint") return value.toString();
  if (value instanceof Uint8Array) {
    return Buffer.from(value.buffer, value.byteOffset, value.byteLength).toString("base64");
  }
  return value;
}

export function encodeFrame(frame) {
  const line = `${JSON.stringify(frame, replacer)}\n`;
  if (Buffer.byteLength(line) > MAX_FRAME_BYTES) {
    throw new Error("frame exceeds the 8 MiB limit");
  }
  return line;
}

export function decodeFrame(line) {
  if (Buffer.byteLength(line) > MAX_FRAME_BYTES) throw new Error("frame exceeds the 8 MiB limit");
  const value = JSON.parse(line);
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("frame is not an object");
  }
  return value;
}
