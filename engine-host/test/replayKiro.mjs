import { readFileSync } from "node:fs";
import { replayAcp } from "./replayAcp.mjs";

// Replays the recorded Kiro 2.32.0 to 2.33.1 ACP sessions (T3 Code, MIT, Copyright (c) 2026 T3
// Tools Inc., commit 9168ca73d02003557725430b515b44e1551544af; see test/fixtures/NOTICE). The
// family runs with `agent.id = "kiro"`, so the Kiro entry supplies the V3 launch args, the
// config ids and the autopilot rule while the generic ACP client does the wire.
//
// What the replay ignores, and why:
// - T3's `<t3_code_instructions>`/`<runtime_info>` prompt wrapper: replayAcp compares only the
//   `<user_request>` body, because this host never sends T3's instructions (acp.mjs header).
// - T3's `initialize` client capabilities (`fs`, `terminal`, session notices/compaction and
//   `_meta.kiro.hooks`): this host advertises only form elicitation, deliberately; replayAcp
//   compares `protocolVersion` and `clientInfo.name` only.
// - T3's `autopilot` config writes: provider-kiro turns autopilot on only for a full-access
//   thread. This host always writes Kiro's supervised value when the option is offered, so the
//   recorded autopilot request (and the response paired with it) is dropped and a host write
//   with no recorded frame is answered with `{}` instead of matching. The Kiro tests assert
//   every real autopilot write is `off`; no replay substitutes an `on`.
const KIRO_AGENT = Object.freeze({ command: "kiro-cli", args: [], env: {}, id: "kiro" });

export function isAutopilotWrite(frame) {
  return frame?.method === "session/set_config_option" && frame?.params?.configId === "autopilot";
}

export function readKiro(source) {
  return (Array.isArray(source) ? source : readFileSync(source, "utf8").trim().split("\n").map((line) => JSON.parse(line))).slice(1);
}

/** The values Kiro sessions wrote for one config id, in recorded order. */
export function kiroConfigWrites(source, configId) {
  return readKiro(source)
    .filter((entry) => entry.type === "expect_outbound" && entry.frame?.method === "session/set_config_option" && entry.frame.params?.configId === configId)
    .map((entry) => entry.frame.params.value);
}

/** The first `session/fork` request and the session id its reply named. */
export function kiroFork(source) {
  const entries = readKiro(source);
  const request = entries.find((entry) => entry.type === "expect_outbound" && entry.frame?.method === "session/fork");
  const response = entries.find((entry) => entry.type === "emit_inbound" && entry.frame?.kind === "response" && entry.frame.method === "session/fork");
  return { request: request?.frame, sessionId: response?.frame.result?.sessionId };
}

/** The text Kiro's `_session/steer` carried. */
export function kiroSteerText(source) {
  const request = readKiro(source).find((entry) => entry.type === "expect_outbound" && entry.frame?.method === "_session/steer");
  return request?.frame.params?.message;
}

/** The first Kiro `userMessageId` a prompt was assigned, for a fork from that turn. */
export function kiroFirstUserMessageId(source) {
  for (const entry of readKiro(source)) {
    if (entry.type !== "emit_inbound" || entry.frame?.kind !== "notification") continue;
    const kiro = entry.frame.params?.update?._meta?.kiro;
    if (kiro?.kind === "user_message_id_assigned" && typeof kiro.userMessageId === "string") return kiro.userMessageId;
  }
  return undefined;
}

export function replayKiro(source) {
  return replayAcp(source, {
    ignoreExpectedOutbound: isAutopilotWrite,
    ignoreActualOutbound: isAutopilotWrite,
    respondToIgnoredRequest: (frame) => (isAutopilotWrite(frame) ? {} : undefined),
  });
}

export { KIRO_AGENT };
