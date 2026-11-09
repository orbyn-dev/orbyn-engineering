// The one door this feature uses to reach the API client. Named operations and
// hooks live here; no file in the feature calls the transport directly.
import { request } from "../../api/client";
import type { LocalServer, ProjectFileView, QuestionRequest, ScheduledTask, TerminalInfo } from "../../api/types";

const q = encodeURIComponent;

export function listScheduledTasks(signal?: AbortSignal) {
  return request<ScheduledTask[]>("GET", "/api/tasks", undefined, signal);
}

export function setScheduledTaskEnabled(taskId: string, enabled: boolean) {
  return request<ScheduledTask>("POST", `/api/tasks/${q(taskId)}/enabled`, { enabled });
}

export function runScheduledTask(taskId: string) {
  return request("POST", `/api/tasks/${q(taskId)}/run`);
}

export function pendingQuestions(threadId: string, signal?: AbortSignal) {
  return request<QuestionRequest[]>("GET", `/api/questions?thread_id=${q(threadId)}&state=pending`, undefined, signal);
}

export function answerQuestion(questionId: string, path: string, body?: unknown) {
  return request("POST", `/api/questions/${q(questionId)}/${path}`, body);
}

export function readThreadFile(threadId: string, path: string, signal?: AbortSignal) {
  return request<{ content: string }>("GET", `/api/threads/${q(threadId)}/files/content?path=${q(path)}`, undefined, signal);
}

export function saveThreadFile(threadId: string, body: { path: string; content: string; base_revision: string | null }) {
  return request("PUT", `/api/threads/${q(threadId)}/files/content`, body);
}

export function listThreadServers(threadId: string, signal?: AbortSignal) {
  return request<LocalServer[]>("GET", `/api/threads/${q(threadId)}/servers`, undefined, signal);
}

export function listThreadScripts(threadId: string, signal?: AbortSignal) {
  return request<ProjectFileView>("GET", `/api/threads/${q(threadId)}/scripts`, undefined, signal);
}

export function listTerminals(threadId: string, signal?: AbortSignal) {
  return request<TerminalInfo[]>("GET", `/api/threads/${q(threadId)}/terminals`, undefined, signal);
}

export { ApiFailure, api, attachmentBlob } from "../../api/client";
export { pullsApi } from "../../api/pulls";
export { inDesktopApp } from "../../api/shell";
