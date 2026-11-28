/**
 * The personal library contract of docs/execution-profiles-and-library.md, Part 2 (W9), as the service answers it (routes in
 * crates/orbynd/src/routes/library.rs; the generated types in src/generated are the reference and are not edited). Every list answer
 * is a bare array. Nothing here repeats a generated type; only the client's own views are defined.
 */

import type { EnvSet } from "../../generated/EnvSet";
import type { EnvSetDraft } from "../../generated/EnvSetDraft";
import type { ImportAnswer } from "../../generated/ImportAnswer";
import type { LibraryDocument } from "../../generated/LibraryDocument";
import type { LibrarySummary } from "../../generated/LibrarySummary";
import type { Param } from "../../generated/Param";
import type { ProfileGrant } from "../../generated/ProfileGrant";
import type { Prompt } from "../../generated/Prompt";
import type { PromptDraft } from "../../generated/PromptDraft";
import type { SuggestedEnvSet } from "../../generated/SuggestedEnvSet";
import type { SuggestedPrompt } from "../../generated/SuggestedPrompt";
import type { SuggestedWorkflow } from "../../generated/SuggestedWorkflow";
import type { Suggestions } from "../../generated/Suggestions";
import type { Workflow } from "../../generated/Workflow";
import type { WorkflowDraft } from "../../generated/WorkflowDraft";

export type { EnvKey } from "../../generated/EnvKey";
export type { LibraryKind as Kind } from "../../generated/LibraryKind";
export type { LibraryLimits } from "../../generated/LibraryLimits";
export type { LibrarySource } from "../../generated/LibrarySource";
export type { LibrarySummary } from "../../generated/LibrarySummary";
export type { ParamKind } from "../../generated/ParamKind";
export type { ImportAnswer, LibraryDocument, ProfileGrant };
export type { EnvSet, EnvSetDraft, Param, Prompt, PromptDraft, Suggestions, Workflow, WorkflowDraft };

/** `file` is a path under `.orbyn/` (`workflows.json`, `prompts.json` or `env-sets.json`); the service says it as a string. */
export type SuggestionFile = string;

/** A repository entry carries its own `file` and `index` (its zero-based position in that file, counting dropped entries): they name one entry and are what copy takes. A workflow names its sets (`env_set_names`), never ids. */
export type RepoWorkflow = SuggestedWorkflow;
export type RepoPrompt = SuggestedPrompt;
export type RepoEnvSet = SuggestedEnvSet;
export type Problem = Suggestions["problems"][number];

/** `POST .../suggestions/copy` answers `201` with the created entry beside its kind; the client narrows the entry by the kind. */
export type Copied =
  | { kind: "workflow"; entry: Workflow }
  | { kind: "prompt"; entry: Prompt }
  | { kind: "env_set"; entry: EnvSet };

/** The four parts of an export document, in the service's `kinds` spelling (`profiles,workflows,prompts,env_sets`). */
export type ExportKind = "profiles" | "workflows" | "prompts" | "env_sets";

/** Every kind: what an export holds unless a caller narrows it. */
export const EXPORT_KINDS: readonly ExportKind[] = ["profiles", "workflows", "prompts", "env_sets"];

export const LIMITS = { name: 60, description: 200, command: 4000, body: 32 * 1024, params: 16, defaultValue: 500, choiceText: 60, choiceMin: 2, choiceMax: 20, envSetsPerWorkflow: 4, keysPerSet: 64, value: 8192 } as const;

export interface LibraryApi {
  /** `null` when the service has no library route (`404`): the feature is hidden, with no error. */
  summary(signal?: AbortSignal): Promise<LibrarySummary | null>;
  workflows(signal?: AbortSignal): Promise<Workflow[]>;
  prompts(signal?: AbortSignal): Promise<Prompt[]>;
  envSets(signal?: AbortSignal): Promise<EnvSet[]>;
  /** Fills the parameters and answers the exact text. Saves and runs nothing. A read, so it is not Person-class. */
  renderWorkflow(id: string, values: Record<string, string>, signal?: AbortSignal): Promise<{ command: string }>;
  renderPrompt(id: string, values: Record<string, string>, signal?: AbortSignal): Promise<{ text: string }>;
  /** Writes. Person-class (`PENDING_PERSON_ROUTES`). */
  createWorkflow(draft: WorkflowDraft): Promise<Workflow>;
  updateWorkflow(id: string, draft: WorkflowDraft, seenRevision: number): Promise<Workflow>;
  deleteWorkflow(id: string): Promise<void>;
  /**
   * `where` is one of a thread and a project (a project runs in its latest thread). `ackGrants` is exactly the list a `409 confirm_required`
   * named, never a stored or guessed one; a first run sends none.
   */
  runWorkflow(id: string, values: Record<string, string>, where: { thread_id: string } | { project_id: string }, ackGrants?: readonly ProfileGrant[]): Promise<{ terminal_id: string }>;
  createPrompt(draft: PromptDraft): Promise<Prompt>;
  updatePrompt(id: string, draft: PromptDraft, seenRevision: number): Promise<Prompt>;
  deletePrompt(id: string): Promise<void>;
  createEnvSet(draft: EnvSetDraft): Promise<EnvSet>;
  updateEnvSet(id: string, draft: EnvSetDraft, seenRevision: number): Promise<EnvSet>;
  deleteEnvSet(id: string): Promise<void>;
  /** Write-only. Answers `{ name, present: true }`, never the value. */
  setKey(setId: string, key: string, value: string): Promise<{ name: string; present: boolean }>;
  removeKey(setId: string, key: string): Promise<{ name: string; present: boolean }>;
  suggestions(projectId: string, signal?: AbortSignal): Promise<Suggestions>;
  copySuggestion(projectId: string, file: SuggestionFile, index: number): Promise<Copied>;
  /** Person write. `dryRun` answers what would be created, renamed or refused and writes nothing. Over `import_bytes` is `400 invalid_input`. */
  importLibrary(document: unknown, dryRun: boolean): Promise<ImportAnswer>;
  /**
   * `GET /api/library/export?kinds=`: the export document, every kind unless `kinds` narrows it. A Local read, so no person key
   * and no write; core writes no file, so the caller saves it. A `404 not_found` is a service too old for export, and is reported.
   */
  exportLibrary(kinds?: readonly ExportKind[], signal?: AbortSignal): Promise<LibraryDocument>;
}
