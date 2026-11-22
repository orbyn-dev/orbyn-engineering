import assert from "node:assert/strict";
import { PassThrough } from "node:stream";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { runHost } from "../src/host.mjs";
import {
  acpAgentArgs,
  acpFamily,
  acpModelCatalog,
  parseElicitationForm,
} from "../src/families/acp.mjs";
import {
  ACP_AGENTS,
  acpAgentHooks,
  KIRO_ACP_ARGS,
  KIRO_COMPACT_METHOD,
  KIRO_CONFIG_IDS,
} from "../src/families/acpAgents.mjs";

// Per-agent units. The elicitation shapes and the model/config cases come from T3 Code's
// provider-acp-registry droidAcp/ompAcp and provider-acp runtimeModel tests (MIT, Copyright (c)
// 2026 T3 Tools Inc., commit 9168ca73d02003557725430b515b44e1551544af); the Kiro cases mirror
// packages/provider-kiro/src/server/{acpSupport,status}.ts and live Kiro 2.3x sessions. Nothing
// here is a recorded transcript; test/fixtures/kiro/ holds those.

const WORKSPACE = "/work";
const settle = (ms = 40) => new Promise((resolve) => setTimeout(resolve, ms));

// ---------------------------------------------------------------------------------------------
// Droid and OMP elicit forms.
// ---------------------------------------------------------------------------------------------

const droidField = (id, title, description, options) => ({
  type: "string",
  title,
  description,
  oneOf: options.map(([value, label]) => ({ const: value, title: label })),
});

function droidSchema({ extra = {}, required = ["question_1"] } = {}) {
  return {
    type: "object",
    properties: {
      question_1: droidField("question_1", "Fixture choice", "Which fixture choice?", [
        ["option_1", "First fixture choice"],
        ["option_2", "Second fixture choice"],
        ["other", "Other"],
      ]),
      question_1_other: { type: "string" },
      ...extra,
    },
    required,
  };
}

test("droid: a complete form asks the choices without the companion and restores it for other", () => {
  const hooks = acpAgentHooks("droid");
  const form = hooks.extractElicitationForm({ mode: "form", requestedSchema: droidSchema() });
  assert.ok(form);
  assert.deepEqual(form.questions, [
    {
      id: "question_1",
      header: "Fixture choice",
      question: "Which fixture choice?",
      options: [
        { label: "First fixture choice", description: "First fixture choice", value: "option_1" },
        { label: "Second fixture choice", description: "Second fixture choice", value: "option_2" },
      ],
    },
  ]);
  assert.deepEqual(form.respond({ question_1: "option_2" }), {
    action: "accept",
    content: { question_1: "option_2" },
  });
  // The client already offers custom answers, so the hidden *_other companion comes back with
  // the choice set to "other" (T3's extractDroidElicitationForm).
  assert.deepEqual(form.respond({ question_1: "A fixture answer" }), {
    action: "accept",
    content: { question_1: "other", question_1_other: "A fixture answer" },
  });
  assert.deepEqual(form.respond({ question_1: "   " }), { action: "cancel" });
  assert.deepEqual(form.respond({}), { action: "cancel" });
});

test("droid: an attachment suffix on a selected choice stays with the custom text", () => {
  const form = acpAgentHooks("droid").extractElicitationForm({ mode: "form", requestedSchema: droidSchema() });
  assert.ok(form);
  assert.deepEqual(
    form.respond({ question_1: 'option_1\n\nAttached image "shot.png": "file:///tmp/shot.png"' }),
    {
      action: "accept",
      content: {
        question_1: "other",
        question_1_other: 'First fixture choice\n\nAttached image "shot.png": "file:///tmp/shot.png"',
      },
    },
  );
});

test("droid: incomplete or foreign forms return undefined so the generic parser applies", () => {
  const hooks = acpAgentHooks("droid");
  const fallback = (schema) => parseElicitationForm(schema, "The agent asks a question.");
  assert.equal(
    hooks.extractElicitationForm({
      mode: "form",
      requestedSchema: droidSchema({
        extra: { question_2: droidField("question_2", "Second", "Second?", [["option_1", "One"], ["other", "Other"]]) },
        required: ["question_1", "question_2"],
      }),
    }),
    undefined,
    "a form missing question_2's companion is not a complete Droid form",
  );
  assert.ok(
    fallback(
      droidSchema({
        extra: { question_2: droidField("question_2", "Second", "Second?", [["option_1", "One"], ["other", "Other"]]) },
        required: ["question_1", "question_2"],
      }),
    ),
    "the generic parser can still show it",
  );
  assert.equal(
    hooks.extractElicitationForm({
      mode: "form",
      requestedSchema: droidSchema({ required: [] }),
    }),
    undefined,
    "Droid requires every question it asks",
  );
  assert.equal(
    hooks.extractElicitationForm({
      mode: "form",
      requestedSchema: droidSchema({ extra: { free_text: { type: "string" } } }),
    }),
    undefined,
    "an unknown property is not hidden by Droid's form parser",
  );
  assert.equal(
    hooks.extractElicitationForm({ mode: "form", requestedSchema: { properties: {}, required: [] } }),
    undefined,
  );
  assert.equal(hooks.extractElicitationForm({ mode: "url", requestedSchema: droidSchema() }), undefined);
});

const ompField = (title, description, options) => ({
  type: "string",
  title,
  ...(description === undefined ? {} : { description }),
  oneOf: options.map(([value, label, detail]) => ({
    const: value,
    title: label,
    ...(detail === undefined ? {} : { description: detail }),
  })),
});

function ompSchema({ extra = {} } = {}) {
  return {
    type: "object",
    properties: {
      q1: ompField("Which OMP fixture?", "Fixture header", [
        ["alpha", "Alpha", "The first fixture option"],
        ["beta", "Beta"],
      ]),
      q1__other: { type: "string", title: "Other (type your own)" },
      ...extra,
    },
  };
}

test("omp: a complete form uses the question title and stores custom answers under __other", () => {
  const form = acpAgentHooks("omp").extractElicitationForm({ mode: "form", requestedSchema: ompSchema() });
  assert.ok(form);
  assert.deepEqual(form.questions, [
    {
      id: "q1",
      header: "Fixture header",
      question: "Which OMP fixture?",
      options: [
        { label: "Alpha", value: "alpha", description: "The first fixture option" },
        { label: "Beta", value: "beta", description: "Beta" },
      ],
    },
  ]);
  assert.deepEqual(form.respond({ q1: "beta" }), { action: "accept", content: { q1: "beta" } });
  assert.deepEqual(form.respond({ q1: "OMP custom answer" }), {
    action: "accept",
    content: { q1__other: "OMP custom answer" },
  });
  assert.deepEqual(form.respond({ q1: "  " }), { action: "cancel" });
  assert.deepEqual(form.respond({}), { action: "cancel" });
});

test("omp: incomplete or foreign forms return undefined so the generic parser applies", () => {
  const hooks = acpAgentHooks("omp");
  const withoutCompanion = ompSchema();
  delete withoutCompanion.properties.q1__other;
  assert.equal(hooks.extractElicitationForm({ mode: "form", requestedSchema: withoutCompanion }), undefined);
  assert.ok(
    parseElicitationForm(withoutCompanion, "The agent asks a question."),
    "the generic parser handles a form OMP's parser does not recognize",
  );
  assert.equal(
    hooks.extractElicitationForm({ mode: "form", requestedSchema: ompSchema({ extra: { other: { type: "string" } } }) }),
    undefined,
    "unknown fields are not hidden",
  );
  const wrongCompanion = ompSchema();
  wrongCompanion.properties.q1__other.title = "Something else";
  assert.equal(hooks.extractElicitationForm({ mode: "form", requestedSchema: wrongCompanion }), undefined);
  assert.equal(hooks.extractElicitationForm({ mode: "form", requestedSchema: { properties: {} } }), undefined);
});

// ---------------------------------------------------------------------------------------------
// Kimi tool calls.
// ---------------------------------------------------------------------------------------------

test("kimi: streamed JSON arguments leave the bare tool name for a permission frame", () => {
  const normalize = acpAgentHooks("kimi").normalizeToolCall;
  assert.equal(typeof normalize, "function");
  const toolCall = { toolCallId: "t1", title: 'delegate_task: {"mode":"async","task":"Ru', kind: "other" };
  assert.deepEqual(normalize(toolCall), { ...toolCall, title: "delegate_task" });
  assert.deepEqual(normalize({ ...toolCall, title: 'delegate_task: {"mode":"async","task":"Ru...cho child-result-42\'"}' }), {
    ...toolCall,
    title: "delegate_task",
  });
  // Only a bare name or a JSON opening brace is a Kimi-style title; built-ins keep their prose.
  assert.equal(normalize({ ...toolCall, title: "delegate_task" }).title, "delegate_task");
  assert.equal(normalize({ ...toolCall, title: "Read File" }).title, "Read File");
  assert.equal(normalize({ ...toolCall, title: "html_render:" }).title, "html_render:");
  assert.deepEqual(normalize({ toolCallId: "t2" }), { toolCallId: "t2" });
});

// ---------------------------------------------------------------------------------------------
// Model catalogs: Mistral Vibe per-model thinking levels, GLM/Kimi bare names.
// ---------------------------------------------------------------------------------------------

const vibeModels = [
  { value: "mistral-large-4-0-3", name: "mistral-large-4-0-3" },
  { value: "mistral-medium-3.5", name: "mistral-medium-3.5" },
];

function vibeSetup(currentModel, levels, currentLevel = "high") {
  return {
    configOptions: [
      { id: "model", name: "Model", category: "model", type: "select", currentValue: currentModel, options: vibeModels },
      {
        id: "thinking",
        name: "Thinking",
        category: "thinking",
        type: "select",
        currentValue: currentLevel,
        options: levels.map((value) => ({ value, name: value })),
      },
    ],
  };
}

test("mistral-vibe: the thinking levels belong to the session's model, not every model", () => {
  const large = acpModelCatalog(vibeSetup("mistral-large-4-0-3", ["low", "medium", "high"]), "mistral-vibe");
  assert.deepEqual(
    large.models.map((model) => [model.id, model.efforts.map((effort) => effort.id), model.defaultEffort]),
    [
      ["mistral-large-4-0-3", ["low", "medium", "high"], "high"],
      ["mistral-medium-3.5", [], undefined],
    ],
  );
  const medium = acpModelCatalog(
    vibeSetup("mistral-medium-3.5", ["off", "high"], "high"),
    "mistral-vibe",
  );
  assert.deepEqual(
    medium.models.map((model) => [model.id, model.efforts.map((effort) => effort.id)]),
    [
      ["mistral-large-4-0-3", []],
      ["mistral-medium-3.5", ["off", "high"]],
    ],
  );
  // The category is still recognized for any agent (it was already generic); the quirk is the
  // per-model scope above.
  assert.deepEqual(
    acpModelCatalog(vibeSetup("mistral-large-4-0-3", ["low", "high"]), "default").models[1].efforts.map(
      (effort) => effort.id,
    ),
    ["low", "high"],
  );
});

test("glm and kimi: bare model names are their ids and supportsReasoningEffort false drops efforts", () => {
  const setup = {
    configOptions: [
      {
        id: "model",
        name: "Model",
        category: "model",
        type: "select",
        currentValue: "glm-4.7",
        options: [{ value: "glm-4.7" }, { value: "kimi-k3" }],
      },
      {
        id: "effort",
        name: "Effort",
        category: "thought_level",
        type: "select",
        currentValue: "high",
        options: [{ value: "low", name: "Low" }, { value: "high", name: "High" }],
      },
    ],
    models: {
      availableModels: [
        { modelId: "glm-4.7", name: "", _meta: { supportsReasoningEffort: false } },
        { modelId: "kimi-k3", name: "" },
      ],
    },
  };
  const catalog = acpModelCatalog(setup, "glm");
  assert.deepEqual(
    catalog.models.map((model) => [model.id, model.name, model.efforts.map((effort) => effort.id)]),
    [
      ["glm-4.7", "glm-4.7", []],
      ["kimi-k3", "kimi-k3", ["low", "high"]],
    ],
  );
  assert.equal(catalog.models[0].isDefault, true);
  assert.equal(catalog.models[0].defaultEffort, undefined);
});

// ---------------------------------------------------------------------------------------------
// Kiro: launch knowledge, config ids and the autopilot never-on rule.
// ---------------------------------------------------------------------------------------------

const KIRO_AGENT = { command: "kiro-cli", args: [], env: {}, id: "kiro" };

test("kiro: empty args become the V3 launch, and explicit args win", () => {
  assert.deepEqual(KIRO_ACP_ARGS, ["acp", "--agent-engine=v3", "--auth-method=cli"]);
  assert.deepEqual(acpAgentArgs({ command: "kiro-cli", id: "kiro" }), KIRO_ACP_ARGS);
  assert.deepEqual(acpAgentArgs({ command: "kiro-cli", args: [], id: "kiro" }), KIRO_ACP_ARGS);
  assert.deepEqual(acpAgentArgs({ command: "kiro-cli", args: ["--verbose"], id: "kiro" }), ["--verbose"]);
  assert.deepEqual(acpAgentArgs({ command: "kiro-cli", args: [], id: "default" }), []);
  assert.deepEqual(KIRO_CONFIG_IDS, { model: "model", effort: "effortLevel", mode: "mode", autopilot: "autopilot" });
});

test("another agent id keeps the generic launch and the default hooks", () => {
  assert.deepEqual(acpAgentArgs({ command: "agent", args: [], id: "mystery" }), []);
  assert.equal(acpAgentHooks("mystery"), ACP_AGENTS.default);
  assert.equal(acpAgentHooks("").extractElicitationForm, undefined);
  assert.equal(acpAgentHooks(" ").normalizeToolCall, undefined);
  assert.equal(ACP_AGENTS.default.extractElicitationForm, undefined);
  assert.equal(ACP_AGENTS.default.normalizeToolCall, undefined);
});

function kiroOptions(options = {}) {
  const state = options.map((option) => ({ ...option }));
  return {
    state,
    set(configId, value) {
      const option = state.find((candidate) => candidate.id === configId);
      if (option !== undefined) option.currentValue = value;
    },
    current(id) {
      return state.find((option) => option.id === id);
    },
  };
}

function kiroAgent({ extensions = [KIRO_COMPACT_METHOD], options } = {}) {
  const config = kiroOptions(
    options ?? [
      {
        id: "mode",
        name: "Mode",
        category: "mode",
        type: "select",
        currentValue: "vibe",
        options: [
          { value: "vibe", name: "Default" },
          { value: "spec", name: "Spec" },
        ],
      },
      {
        id: "effortLevel",
        name: "Effort",
        category: "thought_level",
        type: "select",
        currentValue: "high",
        options: [
          { value: "low", name: "Low" },
          { value: "high", name: "High" },
        ],
      },
      {
        id: "autopilot",
        name: "Autopilot",
        type: "select",
        currentValue: "on",
        options: [
          { value: "on", name: "Autopilot" },
          { value: "off", name: "Supervised" },
        ],
      },
    ],
  );
  const state = { spawned: 0, closed: 0, written: [], promptId: undefined, sessionOptions: undefined };
  let line;
  let exit;
  const api = {
    send: (frame) => line(JSON.stringify(frame)),
    respond: (id, result) => api.send({ jsonrpc: "2.0", id, result }),
    notify: (method, params) => api.send({ jsonrpc: "2.0", method, params }),
    spawn: (options) => {
      state.spawned++;
      state.sessionOptions = options;
      return {
        write(text) {
          const frame = JSON.parse(text);
          state.written.push(frame);
          const { id, method, params } = frame;
          if (id === undefined) return;
          if (method === "initialize") {
            api.respond(id, {
              protocolVersion: 1,
              agentCapabilities: {
                loadSession: true,
                sessionCapabilities: { close: {}, fork: {} },
                _meta: { kiro: { extensionMethods: extensions } },
              },
              authMethods: [],
            });
            return;
          }
          if (method === "session/new") {
            api.respond(id, { sessionId: "s1", configOptions: config.state });
            return;
          }
          if (method === "session/set_config_option") {
            config.set(params.configId, params.value);
            api.respond(id, { configOptions: config.state });
            return;
          }
          if (method === "session/prompt") {
            state.promptId = id;
            return;
          }
          if (method === "_session/steer") {
            api.respond(id, { queued: true, messageId: "steer-from-kiro" });
            return;
          }
          if (method === "_kiro/session/compact") {
            api.respond(id, { success: true });
            return;
          }
          if (method === "session/close") api.respond(id, {});
        },
        onLine(handler) {
          line = handler;
        },
        onStderr() {},
        onError() {},
        exited: new Promise((resolve) => (exit = resolve)),
        async close() {
          state.closed++;
          exit({ code: 0, signal: null });
        },
      };
    },
    state,
    config,
  };
  return api;
}

function boot({ spawn, ...familyOptions } = {}) {
  const input = new PassThrough();
  const output = new PassThrough();
  const frames = [];
  const all = [];
  const waiters = [];
  createInterface({ input: output }).on("line", (line) => {
    const frame = JSON.parse(line);
    all.push(frame);
    frames.push(frame);
    for (const waiter of [...waiters]) waiter();
  });
  runHost({ family: acpFamily({ spawn, ...familyOptions }), input, output });
  const next = (predicate) =>
    new Promise((resolve) => {
      const check = () => {
        const index = frames.findIndex(predicate);
        if (index < 0) return false;
        resolve(frames.splice(index, 1)[0]);
        const at = waiters.indexOf(check);
        if (at >= 0) waiters.splice(at, 1);
        return true;
      };
      if (!check()) waiters.push(check);
    });
  return { next, send: (frame) => input.write(`${JSON.stringify(frame)}\n`), all };
}

async function handshake(h) {
  await h.next((frame) => frame.type === "hello");
  h.send({ id: 1, type: "init", protocol: 1 });
  await h.next((frame) => frame.id === 1);
}

async function openKiro(h, agent = kiroAgent(), extra = {}, id = 2) {
  h.send({
    id,
    type: "open",
    operation: "create",
    sessionId: "s",
    cwd: WORKSPACE,
    options: { agent: KIRO_AGENT, ...extra },
  });
  const opened = await h.next((frame) => frame.id === id);
  assert.equal(opened.ok, true, JSON.stringify(opened));
  return opened;
}

test("kiro: the process gets the V3 args and session options use Kiro's config ids", async () => {
  const agent = kiroAgent();
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  await openKiro(h, agent, { model: "claude-opus-5.5", effort: "low", mode: "spec" });
  assert.deepEqual(agent.state.sessionOptions.args, KIRO_ACP_ARGS);
  assert.deepEqual(
    agent.state.written
      .filter((frame) => frame.method === "session/set_config_option")
      .map((frame) => [frame.params.configId, frame.params.value]),
    [
      ["mode", "spec"],
      ["model", "claude-opus-5.5"],
      ["effortLevel", "low"],
      ["autopilot", "off"],
    ],
  );
});

test("kiro: autopilot is always written off and never on, even when the request or the agent says on", async () => {
  const agent = kiroAgent();
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  await openKiro(h, agent, { model: "claude-haiku-4.5", autopilot: "on" });
  const autopilot = agent.state.written.filter(
    (frame) => frame.method === "session/set_config_option" && frame.params.configId === "autopilot",
  );
  assert.equal(autopilot.length, 1);
  assert.deepEqual(autopilot[0].params, { sessionId: "s1", configId: "autopilot", value: "off" });
  assert.ok(!agent.state.written.some((frame) => frame.params?.value === "on"), "autopilot on never reaches Kiro");

  // A later advert that says on is corrected again, still with off.
  agent.notify("session/update", {
    sessionId: "s1",
    update: {
      sessionUpdate: "config_option_update",
      configOptions: agent.config.state.map((option) =>
        option.id === "autopilot" ? { ...option, currentValue: "on" } : option,
      ),
    },
  });
  await settle();
  const writes = agent.state.written.filter(
    (frame) => frame.method === "session/set_config_option" && frame.params.configId === "autopilot",
  );
  assert.ok(writes.length >= 2, "the new advert is corrected");
  assert.ok(writes.every((frame) => frame.params.value === "off"));
  assert.ok(!agent.state.written.some((frame) => frame.params?.value === "on"));
});

test("kiro: a model Kiro has not listed is still written, so the agent can report it", async () => {
  const list = [
    {
      id: "model",
      name: "Model",
      category: "model",
      type: "select",
      currentValue: "claude-haiku-4.5",
      options: [{ value: "claude-haiku-4.5", name: "Claude Haiku 4.5" }],
    },
    {
      id: "autopilot",
      name: "Autopilot",
      type: "select",
      currentValue: "off",
      options: [
        { value: "on", name: "Autopilot" },
        { value: "off", name: "Supervised" },
      ],
    },
  ];
  const agent = kiroAgent({ options: list });
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  await openKiro(h, agent, { model: "not-a-kiro-model" });
  assert.deepEqual(
    agent.state.written
      .filter((frame) => frame.method === "session/set_config_option")
      .map((frame) => [frame.params.configId, frame.params.value]),
    [["model", "not-a-kiro-model"]],
  );
});

test("kiro: /compact uses the advertised extension and ends one completed run", async () => {
  const agent = kiroAgent();
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  await openKiro(h, agent);
  h.send({ id: 3, type: "send", sessionId: "s", message: "/compact" });
  const accepted = await h.next((frame) => frame.id === 3);
  assert.equal(accepted.ok, true, JSON.stringify(accepted));
  const done = await h.next((frame) => frame.type === "runDone" && frame.sendId === 3);
  assert.deepEqual(done.result, { reason: "completed", threadDisposition: "reusable" });
  assert.deepEqual(
    agent.state.written.filter((frame) => frame.method === "_kiro/session/compact").map((frame) => frame.params),
    [{ sessionId: "s1" }],
  );
  assert.ok(!agent.state.written.some((frame) => frame.method === "session/prompt"));
});

test("kiro: steer uses _session/steer and leaves the prompt running", async () => {
  const agent = kiroAgent();
  const h = boot({ spawn: agent.spawn });
  await handshake(h);
  await openKiro(h, agent);
  h.send({ id: 3, type: "send", sessionId: "s", message: "first" });
  await h.next((frame) => frame.id === 3);
  h.send({ id: 4, type: "steer", sendId: 3, messageId: "m-1", text: "steer this" });
  assert.deepEqual((await h.next((frame) => frame.id === 4)).result, { outcome: "queued", messageId: "m-1" });
  assert.deepEqual(
    agent.state.written.filter((frame) => frame.method === "_session/steer").map((frame) => frame.params),
    [{ sessionId: "s1", message: "steer this" }],
  );
  assert.ok(!agent.state.written.some((frame) => frame.method === "session/cancel"));
  agent.respond(agent.state.promptId, { stopReason: "end_turn" });
  const done = await h.next((frame) => frame.type === "runDone");
  assert.equal(done.result.reason, "completed");
});
