import { ApiFailure, request } from "../../api/client";
import type { EngineHostControl } from "../../generated/EngineHostControl";
import type { EngineHostStatus } from "../../generated/EngineHostStatus";
import { BACKGROUND_OPEN, type HostControlOptions } from "./contract";

/**
 * The person's stop and restart of a family's engine host: `POST /api/engine-host/{family}/stop` and `/restart`. Both are Person routes, so
 * `request` sends the person header on them (`web/src/api/personKey.ts`). One call is one request; the answer (a redacted status) is not
 * used, because the screens read `GET /api/engine-host` again. A host with a background task open answers 409 `background_open`: the
 * failure keeps that code and the service's fixed sentence, and only `stopBackground: true` (sent after the person confirmed) goes through.
 * An unknown family or one off the host answers fixed words, which are cut here and drawn as plain text.
 */
const CAP = 200;

const path = (family: string, action: "stop" | "restart") => `/api/engine-host/${encodeURIComponent(family)}/${action}`;

export interface HostControl {
  stop(family: string, options?: HostControlOptions): Promise<void>;
  restart(family: string, options?: HostControlOptions): Promise<void>;
}

export function createHostControl(): HostControl {
  const send = async (family: string, action: "stop" | "restart", options: HostControlOptions = {}): Promise<void> => {
    const body: EngineHostControl = { stop_background: options.stopBackground === true };
    try {
      await request<EngineHostStatus>("POST", path(family, action), body);
    } catch (failure) {
      if (failure instanceof ApiFailure) {
        const message = failure.message.length > CAP ? `${failure.message.slice(0, CAP)}…` : failure.message;
        // `detail` is dropped: only the code, the status and the cut sentence leave.
        throw new ApiFailure(failure.code, message, failure.status);
      }
      throw failure;
    }
  };
  return { stop: (family, options) => send(family, "stop", options), restart: (family, options) => send(family, "restart", options) };
}

/** True for the 409 that asks the person whether background work may end. */
export const isBackgroundOpen = (failure: unknown): failure is ApiFailure => failure instanceof ApiFailure && failure.code === BACKGROUND_OPEN;
