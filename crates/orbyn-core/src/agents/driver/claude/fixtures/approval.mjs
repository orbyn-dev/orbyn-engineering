#!/usr/bin/env node
// Synthetic CLI only. No provider, account, credentials or network access.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const args = process.argv.slice(2);
const value = (flag) => args[args.indexOf(flag) + 1];
for (const flag of ["-p", "--safe-mode", "--strict-mcp-config", "--no-session-persistence", "--disable-slash-commands", "--no-chrome"]) assert.ok(args.includes(flag));
assert.equal(value("--tools"), "");
assert.equal(value("--disallowedTools"), "*");
assert.equal(value("--max-turns"), "1");
assert.equal(value("--permission-mode"), "dontAsk");
assert.equal(value("--output-format"), "json");
assert.equal(value("--effort"), "medium");
assert.equal(value("--model"), "claude-opus-5-5");
assert.equal(process.env.CLAUDE_CODE_MAX_OUTPUT_TOKENS, "1024");
assert.deepEqual(JSON.parse(readFileSync(value("--mcp-config"), "utf8")), { mcpServers: {} });
assert.deepEqual(JSON.parse(readFileSync(value("--settings"), "utf8")), { disableAllHooks: true, fastMode: false });
assert.ok(!args.includes("--resume") && !args.includes("--continue"));
let prompt = "";
for await (const chunk of process.stdin) prompt += chunk;
assert.ok(!args.includes(prompt));
if (prompt === "hang") {
  setInterval(() => {}, 1000);
} else if (prompt === "malformed") {
  process.stdout.write("not-json");
} else {
  const frame = { type: "result", subtype: "success", is_error: false, num_turns: 1,
    result: '{"approve":true,"reason":"Fits the brief."}', modelUsage: { "claude-opus-5-5": {} },
    total_cost_usd: 0.001,
    usage: { input_tokens: 10, output_tokens: 10, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
  };
  // The named prompts push one reported number past its limit (or exactly onto it); the spec in the Rust test sets 1024 output tokens, 16384 reported tokens and 0.05 dollars.
  const variants = {
    "over-budget": () => { frame.total_cost_usd = 1; },
    "over-output": () => { frame.usage.output_tokens = 1025; },
    "over-total": () => { frame.usage.input_tokens = 16380; },
    "at-the-limits": () => { frame.usage.output_tokens = 1024; frame.usage.input_tokens = 15360; frame.total_cost_usd = 0.05; },
    "two-turns": () => { frame.num_turns = 2; },
    "error-result": () => { frame.is_error = true; },
    "error-subtype": () => { frame.subtype = "error_max_turns"; },
    "other-model": () => { frame.modelUsage = { "claude-sonnet-5-5": {} }; },
    "two-models": () => { frame.modelUsage = { "claude-opus-5-5": {}, "claude-sonnet-5-5": {} }; },
  };
  (variants[prompt] ?? (() => {}))();
  process.stdout.write(JSON.stringify(frame));
}
