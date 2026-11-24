import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { test } from "node:test";
import { claudeFamily } from "../src/families/claude.mjs";
import { cursorFamily } from "../src/families/cursor.mjs";
import { decodeFrame } from "../src/frame.mjs";
import { runHost } from "../src/host.mjs";

/** Runs a host over streams, sends `init`, and returns init's answer. */
async function initAnswer(family) {
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  output.on("data", (chunk) => {
    for (const line of String(chunk).split("\n")) if (line.trim() !== "") frames.push(decodeFrame(line));
  });
  runHost({ family, input, output, onExit: () => {} });
  input.write(`${JSON.stringify({ id: 1, type: "init", protocol: 1 })}\n`);
  for (let tries = 0; tries < 100; tries += 1) {
    const answer = frames.find((frame) => frame.type === "response" && frame.id === 1);
    if (answer) {
      input.end();
      return answer.result;
    }
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error("init was not answered");
}

test("init says the SDK is missing when the Claude family cannot load it", async () => {
  const family = claudeFamily({ loadSdk: () => Promise.reject(new Error("no such package")) });
  assert.deepEqual(await initAnswer(family), { ready: true, sdkMissing: true });
});

test("init says the SDK is present when the Claude family loads it", async () => {
  const family = claudeFamily({ loadSdk: async () => ({ query() {} }) });
  assert.deepEqual(await initAnswer(family), { ready: true, sdkMissing: false });
});

test("init says the SDK is missing for the Cursor family too", async () => {
  const family = cursorFamily({ loadSdk: () => Promise.reject(new Error("no such package")), env: {} });
  assert.deepEqual(await initAnswer(family), { ready: true, sdkMissing: true });
});

test("a family with no SDK probe leaves the field out", async () => {
  const family = { name: "plain", capabilities: [], create: () => ({ handle: async () => null }) };
  assert.deepEqual(await initAnswer(family), { ready: true });
});

test("a probe that throws leaves the field out and init still answers", async () => {
  const family = {
    name: "odd",
    capabilities: [],
    probeSdk: () => {
      throw new Error("boom");
    },
    create: () => ({ handle: async () => null }),
  };
  assert.deepEqual(await initAnswer(family), { ready: true });
});
