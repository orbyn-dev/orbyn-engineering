// The one door this feature uses to reach the API client. Named operations and
// hooks live here; no file in the feature calls the transport directly.
import { request } from "../../api/client";
import type {
  AcpRegistryEntry,
  CleanupReport,
  Diagnostics,
  ForgeStatus,
  KeybindingsView,
  OtlpResult,
  ScheduledTask,
  Thread,
} from "../../api/types";
import type { AcpNativeSessionImported } from "../../generated/AcpNativeSessionImported";
import type { AcpNativeSessionPage } from "../../generated/AcpNativeSessionPage";
import type { AcpSessionCapabilities } from "../../generated/AcpSessionCapabilities";
import type { HistoryScan } from "../../generated/HistoryScan";
import type { HistorySessionImported } from "../../generated/HistorySessionImported";
import type { ProjectScopeCatalog } from "../../generated/ProjectScopeCatalog";
import type { RedactionPatternDetail } from "../../generated/RedactionPatternDetail";
import type { RedactionPatternInput } from "../../generated/RedactionPatternInput";
import type { RedactionPatternTest } from "../../generated/RedactionPatternTest";
import type { RedactionPatternTestInput } from "../../generated/RedactionPatternTestInput";
import type { RedactionSettings } from "../../generated/RedactionSettings";
import type { RedactionSurfaceChange } from "../../generated/RedactionSurfaceChange";
import type { ScopedPreferencesResult } from "../../generated/ScopedPreferencesResult";
import type { TriagePlaybook } from "../../generated/TriagePlaybook";

const q = encodeURIComponent;

export function queryPreferences(body: unknown, signal?: AbortSignal) {
  return request<ScopedPreferencesResult>("POST", "/api/settings/preferences/query", body, signal);
}

export function updatePreferences(body: unknown) {
  return request<ScopedPreferencesResult>("PUT", "/api/settings/preferences", body);
}

export function queryProjectScopes(body: unknown, signal?: AbortSignal) {
  return request<ProjectScopeCatalog>("POST", "/api/settings/project-scopes", body, signal);
}

export function listProjectThreads(projectId: string, archived: boolean, signal?: AbortSignal) {
  const suffix = archived ? "&archived=true" : "";
  return request<Thread[]>("GET", `/api/threads?project_id=${q(projectId)}${suffix}`, undefined, signal);
}

export function listScheduledTasks(signal?: AbortSignal) {
  return request<ScheduledTask[]>("GET", "/api/tasks", undefined, signal);
}

export function saveScheduledTask(taskId: string | null, body: unknown) {
  return request<ScheduledTask>(taskId ? "PUT" : "POST", taskId ? `/api/tasks/${q(taskId)}` : "/api/tasks", body);
}

export function deleteScheduledTask(taskId: string) {
  return request("DELETE", `/api/tasks/${q(taskId)}`);
}

export function setScheduledTaskEnabled(taskId: string, enabled: boolean) {
  return request("POST", `/api/tasks/${q(taskId)}/enabled`, { enabled });
}

export function readKeybindings(signal?: AbortSignal) {
  return request<KeybindingsView>("GET", "/api/keybindings", undefined, signal);
}

export function saveKeybinding(body: unknown) {
  return request<KeybindingsView>("POST", "/api/keybindings", body);
}

export function readDiagnostics(signal?: AbortSignal) {
  return request<Diagnostics>("GET", "/api/diagnostics", undefined, signal);
}

export function repairDatabase() {
  return request("POST", "/api/diagnostics/repair");
}

export function stopServiceProcess(pid: number) {
  return request("POST", `/api/diagnostics/processes/${pid}/stop`);
}

export function sendOtlp(url: string) {
  return request<OtlpResult>("POST", "/api/diagnostics/otlp", { url });
}

export function readCleanupReport(signal?: AbortSignal) {
  return request<CleanupReport | null>("GET", "/api/storage/cleanup", undefined, signal);
}

export function runCleanup() {
  return request<CleanupReport>("POST", "/api/storage/cleanup");
}

export function scanHistory() {
  return request<HistoryScan>("GET", "/api/history/sessions");
}

export function importHistorySession(engine: string, sessionId: string) {
  return request<HistorySessionImported>("POST", "/api/history/import", { engine, session_id: sessionId });
}

export function listRegistry(query: string, signal?: AbortSignal) {
  return request<AcpRegistryEntry[]>("GET", `/api/acp/registry?q=${q(query)}`, undefined, signal);
}

export function addRegistryEntry(id: string, distribution: string | undefined) {
  return request("POST", `/api/acp/registry/${q(id)}`, { distribution });
}

export function removeRegistryEntry(id: string) {
  return request("DELETE", `/api/acp/registry/${q(id)}`);
}

export function readTriagePlaybook() {
  return request<TriagePlaybook>("GET", "/api/triage/playbook");
}

export function listForges(signal?: AbortSignal) {
  return request<ForgeStatus[]>("GET", "/api/forges?rescan=true", undefined, signal);
}

export function nativeSessionCapabilities(engineId: string, signal?: AbortSignal) {
  return request<AcpSessionCapabilities>("GET", `/api/engines/${q(engineId)}/native-sessions/capabilities`, undefined, signal);
}

export function listNativeSessions(engineId: string, query: string) {
  return request<AcpNativeSessionPage>("GET", `/api/engines/${q(engineId)}/native-sessions${query}`);
}

export function importNativeSession(engineId: string, body: unknown) {
  return request<AcpNativeSessionImported>("POST", `/api/engines/${q(engineId)}/native-sessions/import`, body);
}

export function deleteNativeSession(engineId: string, body: { project_id: string; session_id: string }) {
  return request("POST", `/api/engines/${q(engineId)}/native-sessions/delete`, body);
}

export { ApiFailure, NETWORK_ERROR, api, authHeaders } from "../../api/client";
export { serviceUrl } from "../../api/origin";
export { inDesktopApp, onShellChange } from "../../api/shell";
export { SseParser } from "../../api/sse";

// Settings › Secrets. Every write answers with the whole view.
export function readSecrets(signal?: AbortSignal) {
  return request<RedactionSettings>("GET", "/api/settings/secrets", undefined, signal);
}

export function changeSecretSurfaces(change: RedactionSurfaceChange) {
  return request<RedactionSettings>("PUT", "/api/settings/secrets/surfaces", change);
}

export function changeSecretFormat(id: string, enabled: boolean) {
  return request<RedactionSettings>("PUT", `/api/settings/secrets/formats/${q(id)}`, { enabled });
}

export function changeSecretProject(id: string, redaction: boolean) {
  return request<RedactionSettings>("PUT", `/api/settings/secrets/projects/${q(id)}`, { redaction });
}

export function saveSecretPattern(id: string | null, input: RedactionPatternInput) {
  return request<RedactionSettings>(id ? "PUT" : "POST", id ? `/api/settings/secrets/patterns/${q(id)}` : "/api/settings/secrets/patterns", input);
}

export function deleteSecretPattern(id: string) {
  return request<RedactionSettings>("DELETE", `/api/settings/secrets/patterns/${q(id)}`);
}

export function trySecretPattern(input: RedactionPatternTestInput, signal?: AbortSignal) {
  return request<RedactionPatternTest>("POST", "/api/settings/secrets/patterns/test", input, signal);
}

/** One pattern with its text: only the edit dialog the person opens reads it. */
export function readSecretPattern(id: string, signal?: AbortSignal) {
  return request<RedactionPatternDetail>("GET", `/api/settings/secrets/patterns/${q(id)}`, undefined, signal);
}
