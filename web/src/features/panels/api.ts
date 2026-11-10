// The one door this feature uses to reach the API client. Named operations and
// hooks live here; no file in the feature calls the transport directly.
import { request, request as clientRequest } from "../../api/client";
import type {
  BrowserProfile,
  FileContent,
  FileEntry,
  LocalServer,
  PickedElement,
  PreviewTab,
  ProjectFileView,
  SearchHit,
  TerminalInfo,
} from "../../api/types";
import type { PreviewCopy } from "../../generated/PreviewCopy";

const q = encodeURIComponent;
const thread = (threadId: string) => `/api/threads/${q(threadId)}`;
const previews = (threadId: string) => `${thread(threadId)}/previews`;
const preview = (threadId: string, tabId: string) => `${previews(threadId)}/${q(tabId)}`;

export type PreviewSize = { width: number; height: number };
export type PreviewView = { zoom?: number; appearance?: string; mobile?: boolean };
export type PreviewInput = { kind: string; x?: number; y?: number; delta_y?: number; text?: string };

export function listPreviewTabs(threadId: string, signal?: AbortSignal) {
  return request<PreviewTab[]>("GET", previews(threadId), undefined, signal);
}

export function openPreview(threadId: string, body: { url: string; width: number; height: number; private?: boolean; profile?: string }) {
  return request<PreviewTab>("POST", previews(threadId), body);
}

export function navigatePreview(threadId: string, tabId: string, url: string) {
  return request("POST", `${preview(threadId, tabId)}/navigate`, { url });
}

export function changePreviewView(threadId: string, tabId: string, change: PreviewView) {
  return request("POST", `${preview(threadId, tabId)}/view`, change);
}

export function reloadPreview(threadId: string, tabId: string, hard: boolean) {
  return request("POST", `${preview(threadId, tabId)}/reload`, { hard });
}

export function clearPreview(threadId: string, tabId: string, what: { cookies?: boolean; cache?: boolean }) {
  return request("POST", `${preview(threadId, tabId)}/clear`, what);
}

export function resizePreview(threadId: string, tabId: string, size: PreviewSize) {
  return request("POST", `${preview(threadId, tabId)}/resize`, size);
}

export function popOutPreview(threadId: string, tabId: string) {
  return request("POST", `${preview(threadId, tabId)}/window`);
}

export function stepPreviewHistory(threadId: string, tabId: string, back: boolean) {
  return request("POST", `${preview(threadId, tabId)}/history`, { back });
}

export function closePreview(threadId: string, tabId: string) {
  return request("DELETE", preview(threadId, tabId));
}

export function setPreviewControl(threadId: string, tabId: string, human: boolean) {
  return request("POST", `${preview(threadId, tabId)}/control`, { human });
}

export function sendPreviewInput(threadId: string, tabId: string, body: PreviewInput) {
  return request("POST", `${preview(threadId, tabId)}/input`, body);
}

export function pickPreviewElement(threadId: string, tabId: string, point: { x: number; y: number }) {
  return request<PickedElement>("POST", `${preview(threadId, tabId)}/pick`, point);
}

export function copyPreview(threadId: string, tabId: string) {
  return request<PreviewCopy>("POST", `${preview(threadId, tabId)}/copy`);
}

export function previewRecordingEndpoint(threadId: string, tabId: string) {
  return `${preview(threadId, tabId)}/recording`;
}

/** The transport a recording picks between writes and reads with. */
export const previewTransport = clientRequest;

export function choosePreviewFiles(threadId: string, tabId: string, files: string[]) {
  return request("POST", `${preview(threadId, tabId)}/chooser`, { files });
}

export function listBrowserProfiles(signal?: AbortSignal) {
  return request<BrowserProfile[]>("GET", "/api/browser-profiles", undefined, signal);
}

export function listThreadServers(threadId: string, signal?: AbortSignal) {
  return request<LocalServer[]>("GET", `${thread(threadId)}/servers`, undefined, signal);
}

export function stopThreadServer(threadId: string, pid: number) {
  return request<void>("POST", `${thread(threadId)}/servers/${pid}/stop`);
}

export function readThreadFile(threadId: string, path: string, signal?: AbortSignal) {
  return request<FileContent>("GET", `${thread(threadId)}/files/content?path=${q(path)}`, undefined, signal);
}

export function saveThreadFile(threadId: string, body: { path: string; content: string; base_revision: string | null }) {
  return request("PUT", `${thread(threadId)}/files/content`, body);
}

export function listThreadFiles(threadId: string, path: string, signal?: AbortSignal) {
  return request<FileEntry[]>("GET", `${thread(threadId)}/files?path=${q(path)}`, undefined, signal);
}

export function searchThreadFiles(threadId: string, query: string, signal?: AbortSignal) {
  return request<SearchHit[]>("GET", `${thread(threadId)}/files/search?q=${q(query)}&content=true`, undefined, signal);
}

export function openThreadFileInBrowser(threadId: string, path: string) {
  return request("POST", `${thread(threadId)}/previews/file`, { path });
}

export function listTerminals(threadId: string, signal?: AbortSignal) {
  return request<TerminalInfo[]>("GET", `${thread(threadId)}/terminals`, undefined, signal);
}

export function startTerminal(threadId: string, body: unknown) {
  return request<TerminalInfo>("POST", `${thread(threadId)}/terminals`, body);
}

export function startConsole(threadId: string) {
  return request<TerminalInfo>("POST", `${thread(threadId)}/console`, {});
}

export function closeTerminal(threadId: string, terminalId: string) {
  return request("DELETE", `${thread(threadId)}/terminals/${q(terminalId)}`);
}

export function listThreadScripts(threadId: string, signal?: AbortSignal) {
  return request<ProjectFileView>("GET", `${thread(threadId)}/scripts`, undefined, signal);
}

export { ApiFailure, api, authHeaders } from "../../api/client";
export { serviceSocketUrl, serviceUrl } from "../../api/origin";
export { pullsApi } from "../../api/pulls";
export { inDesktopApp } from "../../api/shell";
export { readToken } from "../../api/token";
