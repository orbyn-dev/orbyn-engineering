import { runHost } from "./host.mjs";
import { acpFamily } from "./families/acp.mjs";
import { claudeFamily } from "./families/claude.mjs";
import { cursorFamily, isBenignSdkRejection } from "./families/cursor.mjs";
import { fake } from "./families/fake.mjs";
import { museFamily } from "./families/muse.mjs";
import { opencodeFamily } from "./families/opencode.mjs";

const families = {
  fake,
  // MIT SDK, bundled. It drives the person's own `opencode serve`; no binary is shipped.
  opencode: opencodeFamily({ loadSdk: () => import("@opencode/client") }),
  // MIT SDK, bundled. It drives the person's own `muse serve`; no binary is shipped.
  muse: museFamily({ loadSdk: () => import("@muse-code/sdk") }),
  // The person's own ACP agent. The core resolves the agent and its binary (`open.options.agent`);
  // the host neither ships nor fetches one, and speaks the protocol in src/families/acp.mjs.
  acp: acpFamily(),
};
// Gated: the Claude SDK path stays off until Anthropic approves in writing. The SDK is not bundled;
// it loads from node_modules in development and is missing in the single executable.
if (process.env.ORBYN_ENGINE_HOST_CLAUDE === "1") {
  families.claude = claudeFamily({
    loadSdk: () =>
      import("@anthropic-ai/claude-agent-sdk").catch((cause) => {
        throw new Error("The Claude Agent SDK is not available in this build.", { cause });
      }),
  });
}
// Gated: the Cursor SDK (licence "SEE LICENSE IN LICENSE.md", about 20 MB with native binaries) is neither bundled
// nor a dependency. In development it loads from node_modules if present; in the single executable it is absent.
// The key, if any, is only CURSOR_API_KEY in the environment the core passes at spawn.
if (process.env.ORBYN_ENGINE_HOST_CURSOR === "1") {
  families.cursor = cursorFamily({
    loadSdk: () =>
      import("@cursor/sdk").catch((cause) => {
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
