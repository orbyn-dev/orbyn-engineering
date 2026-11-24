import { createInterface } from "node:readline";
import { PROTOCOL, decodeFrame, encodeFrame, serializeError } from "./frame.mjs";

const SDK_PROBE_MS = 3000;

/** True when the family's SDK cannot be loaded, false when it can, undefined for a family with no SDK or no answer in time. */
async function probeSdk(family) {
  if (typeof family.probeSdk !== "function") return undefined;
  let timer;
  const timeout = new Promise((resolve) => {
    timer = setTimeout(() => resolve(undefined), SDK_PROBE_MS);
  });
  try {
    return await Promise.race([
      Promise.resolve().then(() => family.probeSdk()).then((available) => !available, () => undefined),
      timeout,
    ]);
  } finally {
    clearTimeout(timer);
  }
}

/**
 * Runs the protocol over `input`/`output`. `family` is `{name, sdk, capabilities, create(ctx)}`
 * where create returns `{handle(request) -> result}`; a handler may call ctx.send(frame).
 */
export function runHost({ family, input, output, version = "0.1.0", onExit }) {
  const write = (frame) => output.write(encodeFrame(frame));
  let initialised = false;
  const handler = family.create({ send: write, log: (event) => write({ type: "log", level: "info", event }) });
  const open = new Set();

  write({
    type: "hello",
    protocol: PROTOCOL,
    host: version,
    family: family.name,
    sdk: family.sdk,
    capabilities: family.capabilities,
  });

  const lines = createInterface({ input });
  const handleLine = async (line) => {
    if (line.trim() === "") return;
    let request;
    try {
      request = decodeFrame(line);
    } catch (cause) {
      write({ type: "log", level: "warn", event: { type: "malformedFrame", message: String(cause?.message) } });
      return;
    }
    const { id, type } = request;
    const fail = (cause, method = type) =>
      write({ type: "response", id, ok: false, error: { method, cause: serializeError(cause) } });
    try {
      if (type === "init") {
        if (request.protocol !== PROTOCOL) {
          const err = new Error(`protocol ${request.protocol} does not match host ${PROTOCOL}`);
          err.name = "ProtocolMismatch";
          return fail(err);
        }
        initialised = true;
        // A family that can tell whether its SDK is in this build says so now, so the app knows before the first run. A probe that
        // fails to answer in time says nothing: only a run's own refusal is then known.
        const sdkMissing = await probeSdk(family);
        return write({ type: "response", id, ok: true, result: sdkMissing === undefined ? { ready: true } : { ready: true, sdkMissing } });
      }
      if (!initialised) {
        const err = new Error("request before init");
        err.name = "NotInitialised";
        return fail(err);
      }
      if (type === "ping") return write({ type: "response", id, ok: true, result: null });
      if (type === "close") {
        write({ type: "response", id, ok: true, result: null });
        await handler.close?.();
        return onExit?.(0);
      }
      if (!family.capabilities.includes(type) && !["open", "send", "respond", "cancel", "closeSession"].includes(type)) {
        const err = new Error(`unsupported request ${type}`);
        err.name = "UnsupportedRequest";
        return fail(err);
      }
      open.add(id);
      const result = await handler.handle(request);
      write({ type: "response", id, ok: true, result: result ?? null });
    } catch (cause) {
      fail(cause);
    } finally {
      open.delete(id);
    }
  };

  lines.on("line", (line) => void handleLine(line));
  lines.on("close", async () => {
    await handler.close?.();
    onExit?.(0);
  });
}
