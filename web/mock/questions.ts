/**
 * The mock's engine questions: the routes of `orbynd` that list the pending questions of a thread and settle one
 * (`GET /api/questions`, `POST /api/questions/{id}/answer` and `/dismiss`), with the checks of
 * `crates/orbyn-core/src/conversation/questions.rs`: every question answered, one answer for a single choice, an answer or
 * file only for a question of the request, and a settled question answers `409 already_decided`.
 */
import type { QuestionRequest } from "../src/generated/QuestionRequest.ts";
import type { MockResponse } from "./api.ts";

const err = (status: number, code: string, message: string): MockResponse => ({ status, body: { code, message } });

export function createQuestionsMock() {
  let requests: QuestionRequest[] = [];
  const replace = (settled: QuestionRequest) => {
    requests = requests.map((request) => (request.id === settled.id ? settled : request));
    return settled;
  };
  return {
    reset() {
      requests = [];
    },
    /** Puts an engine's question in front of its thread: it stays pending until answered or dismissed. */
    ask(request: QuestionRequest) {
      requests = [...requests, structuredClone(request)];
    },
    /** Every question the service holds, settled ones included. */
    get all() {
      return requests;
    },
    handle(method: string, seg: string[], query: URLSearchParams, body: unknown): MockResponse | undefined {
      if (seg[0] !== "api" || seg[1] !== "questions") return undefined;
      if (!seg[2]) {
        if (method !== "GET") return undefined;
        const state = query.get("state");
        const thread = query.get("thread_id");
        return { status: 200, body: requests.filter((request) => (state === null || request.state === state) && (thread === null || request.thread_id === thread)) };
      }
      const request = requests.find((entry) => entry.id === seg[2]);
      if (method !== "POST" || (seg[3] !== "answer" && seg[3] !== "dismiss")) return undefined;
      if (!request) return err(404, "not_found", "Not found.");
      if (request.state !== "pending") return err(409, "already_decided", `Already ${request.state}.`);
      const decided = { decided_by: "person", decided_at: new Date().toISOString() };
      if (seg[3] === "dismiss") return { status: 200, body: replace({ ...request, ...decided, state: "dismissed" }) };
      const { answers = {}, attachments = {} } = (body ?? {}) as { answers?: Record<string, string[]>; attachments?: Record<string, string[]> };
      for (const [key, values] of Object.entries(answers)) {
        const question = request.questions.find((entry) => entry.id === key);
        if (!question) return err(400, "invalid_input", "That question is not part of this request.");
        if (values.length > 1 && !question.multi_select) return err(400, "invalid_input", "That question takes one answer.");
      }
      if (request.questions.some((question) => !(question.id in answers))) return err(400, "invalid_input", "Answer every question.");
      if (Object.keys(attachments).some((key) => !request.questions.some((question) => question.id === key))) {
        return err(400, "invalid_input", "That question is not part of this request.");
      }
      // A secret answer reads `[hidden]` once settled, and is never kept.
      const kept = Object.fromEntries(Object.entries(answers).map(([key, values]) => [key, request.questions.find((entry) => entry.id === key)?.secret ? ["[hidden]"] : values]));
      return { status: 200, body: replace({ ...request, ...decided, state: "answered", answers: kept, attachments: Object.keys(attachments).length > 0 ? attachments : null }) };
    },
  };
}
