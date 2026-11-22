import { runHost } from "./host.mjs";
import { acpFamily } from "./families/acp.mjs";
import { claudeFamily } from "./families/claude.mjs";
import { codexFamily } from "./families/codex.mjs";
import { cursorFamily, isBenignSdkRejection } from "./families/cursor.mjs";
import { fake } from "./families/fake.mjs";
import { museFamily } from "./families/muse.mjs";
import { opencodeFamily } from "./families/opencode.mjs";
import { importSdk } from "./sdkLocation.mjs";

const families = {
  fake,
  // The person's own `codex app-server`, over its documented JSON-RPC. No SDK or binary is bundled.
  codex: codexFamily(),
  // MIT SDK, bundled. It drives the person's own `opencode serve`; no binary is shipped.
  opencode: opencodeFamily({ loadSdk: () => import("@opencode/client") }),
  // MIT SDK, bundled. It drives the person's own `muse serve`; no binary is shipped.
  muse: museFamily({ loadSdk: () => import("@muse-code/sdk") }),
  // The person's own ACP agent. The core resolves the agent and its binary (`open.options.agent`);
  // the host neither ships nor fetches one, and speaks the protocol in src/families/acp.mjs.
  acp: acpFamily(),
};
// The SDKs are dependencies. `ORBYN_CLAUDE_SDK_MODULE` and `ORBYN_CURSOR_SDK_MODULE` name another place to load them from (an
// absolute path to the package's entry file, used by the tests that check a missing SDK); a plain name is imported as usual.
// Without an override the host loads the folder shipped beside it, else `node_modules` in development (src/sdkLocation.mjs).
const sdkImport = (override, name) => importSdk(override, name);
// The Claude SDK path is opt-in until the SDK is bundled (docs/sdk-default-plan.md).
if (process.env.ORBYN_ENGINE_HOST_CLAUDE === "1") {
  families.claude = claudeFamily({
    loadSdk: () =>
      sdkImport(process.env.ORBYN_CLAUDE_SDK_MODULE, "@anthropic-ai/claude-agent-sdk").catch((cause) => {
        throw new Error("The Claude Agent SDK is not available in this build.", { cause });
      }),
  });
}
// Gated until the SDK is bundled and real-run checked (docs/sdk-default-plan.md). The key, if any, is only CURSOR_API_KEY in the environment the core passes at spawn.
if (process.env.ORBYN_ENGINE_HOST_CURSOR === "1") {
  families.cursor = cursorFamily({
    loadSdk: () =>
      sdkImport(process.env.ORBYN_CURSOR_SDK_MODULE, "@cursor/sdk").catch((cause) => {
        throw Object.assign(new Error("The Cursor SDK is not available in this build.", { cause }), { kind: "unsupported" });
      }),
  });
}
const name = process.argv[2];
const family = families[name];
if (!family) {
  process.stderr.write(`unknown family: ${name ?? "(none)"}\n`);
  process.exit(2);
}

let output = process.stdout;
if (name === "cursor") {
  // Fd 1 carries frames only: the SDK's own output goes to stderr before any SDK code loads (protocol, Transport).
  const rawWrite = process.stdout.write.bind(process.stdout);
  process.stdout.write = process.stderr.write.bind(process.stderr);
  console.log = console.info = console.debug = console.warn = console.error;
  output = { write: rawWrite };
  // The SDK leaves some promises unhandled after a run closes; only those are survivable.
  process.on("unhandledRejection", (reason) => {
    if (isBenignSdkRejection(reason)) return;
    process.stderr.write(`unhandled rejection: ${reason instanceof Error ? reason.name : typeof reason}\n`);
    process.exit(1);
  });
}

runHost({
  family,
  input: process.stdin,
  output,
  onExit: (code) => process.exit(code),
});
