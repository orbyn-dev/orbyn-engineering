import { runHost } from "./host.mjs";
import { claudeFamily } from "./families/claude.mjs";
import { fake } from "./families/fake.mjs";
import { opencodeFamily } from "./families/opencode.mjs";

const families = {
  fake,
  // MIT SDK, bundled. It drives the person's own `opencode serve`; no binary is shipped.
  opencode: opencodeFamily({ loadSdk: () => import("@opencode/client") }),
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
const name = process.argv[2];
const family = families[name];
if (!family) {
  process.stderr.write(`unknown family: ${name ?? "(none)"}\n`);
  process.exit(2);
}

runHost({
  family,
  input: process.stdin,
  output: process.stdout,
  onExit: (code) => process.exit(code),
});
