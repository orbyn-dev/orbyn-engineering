import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

/**
 * Replays a T3 Code Cursor transcript (MIT, see test/fixtures/NOTICE) as a fake @cursor/sdk.
 * These transcripts record the SDK's public surface (Agent.create/resume, agent.send with onDelta,
 * run.wait/steer/cancel, agent.close), not an internal worker protocol, so the replay is a thin player:
 * every expect_outbound line is asserted against the call the family makes, and the emit_inbound lines
 * that follow are delivered (interaction updates through onDelta, run.completed through run.wait).
 * One cursor runs over the whole transcript, shared by every agent, so a close then resume replays too.
 */
export function replayCursorSdk(file) {
  const lines = readFileSync(file, "utf8").trim().split("\n").map((l) => JSON.parse(l)).filter((l) => l.type !== "transcript_start");
  const calls = { opens: [], sends: [], steers: [], cancels: 0, closes: [] };
  let i = 0;
  let current; // the run being played: {onDelta, resolveWait, resolveSteer}
  let driving = false;
  const tick = () => new Promise((resolve) => setImmediate(resolve));
  const atExpect = (type) => lines[i]?.type === "expect_outbound" && lines[i].frame.type === type;
  async function until(type) {
    for (let n = 0; n < 5000 && !atExpect(type); n++) await tick();
    assert.ok(atExpect(type), `transcript is at ${lines[i]?.label} when ${type} was called`);
  }
  async function drive() {
    if (driving) return;
    driving = true;
    try {
      while (i < lines.length && lines[i].type === "emit_inbound") {
        const { frame } = lines[i];
        i++;
        if (frame.type === "interaction.update") {
          await current.onDelta({ update: frame.update });
          await tick();
        } else if (frame.type === "run.started") {
          // the run object was handed to the family when send() resolved
        } else if (frame.type === "run.steered") {
          current.resolveSteer?.(frame.outcome);
        } else if (frame.type === "run.completed") {
          current.resolveWait(frame.result);
        } else if (frame.type === "agent.opened") {
          // handled by create/resume
        } else assert.fail(`unhandled inbound ${frame.type}`);
      }
    } finally {
      driving = false;
    }
  }
  const open = async (operation, agentId, options) => {
    await until("agent.open");
    const f = lines[i].frame;
    assert.equal(f.operation, operation);
    if (operation === "resume") assert.equal(agentId, f.agentId, "resumed agent id");
    assert.equal(options.model?.id, f.options.model.id, "model");
    assert.equal(options.mode, f.options.mode, "mode");
    assert.equal(options.local.sandboxOptions.enabled, f.options.local.sandboxEnabled, "sandbox");
    assert.equal(options.local.autoReview ?? false, f.options.local.autoReview, "autoReview");
    assert.deepEqual(options.local.settingSources, f.options.local.settingSources, "settingSources");
    assert.equal(typeof options.local.cwd, "string", "cwd");
    calls.opens.push(options);
    i++;
    const opened = lines[i].frame; // agent.opened
    i++;
    return agentFor(opened.agentId);
  };
  const agentFor = (agentId) => ({
    agentId,
    async send(message, options) {
      await until("run.start");
      const f = lines[i].frame;
      assert.equal(message, f.message, "prompt");
      assert.equal(options.model?.id, f.options.model.id, "send model");
      assert.equal(options.mode, f.options.mode, "send mode");
      calls.sends.push({ message, options });
      i++;
      const started = lines[i].frame; // run.started
      i++;
      let resolveWait;
      const wait = new Promise((r) => (resolveWait = r));
      current = { onDelta: options.onDelta, resolveWait };
      const run = {
        id: started.runId,
        agentId: started.agentId,
        wait: () => wait,
        async steer(text) {
          await until("run.steer");
          assert.equal(text, lines[i].frame.text, "steer text");
          calls.steers.push(text);
          i++;
          const outcome = new Promise((r) => (current.resolveSteer = r));
          void drive();
          return outcome;
        },
        async cancel() {
          await until("run.cancel");
          calls.cancels++;
          i++;
          void drive();
        },
      };
      setImmediate(() => void drive());
      return run;
    },
    async close() {
      await until("agent.close");
      calls.closes.push(lines[i].frame.agentId);
      assert.equal(lines[i].frame.agentId, agentId);
      i++;
    },
  });
  return {
    calls,
    done: () => i === lines.length,
    sdk: {
      Agent: {
        create: (options) => open("create", undefined, options),
        resume: (agentId, options) => open("resume", agentId, options),
      },
    },
  };
}
