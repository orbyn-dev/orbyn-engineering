/**
 * The personal library contract of docs/execution-profiles-and-library.md, Part 2 (W9). Hand-written: no generated types exist for
 * these routes and no service answers them yet. Field names are the doc's. Anything the doc leaves open is marked "guess" and is
 * listed in the pull request.
 */

export type ParamKind = "text" | "choice" | "path";

export interface Param {
  /** `[a-z][a-z0-9_]{0,31}`, unique in the item, at most 16 per item. */
  name: string;
  label: string;
  kind: ParamKind;
  /** At most 500 characters. */
  default?: string;
  required: boolean;
  /** 2 to 20 choices of at most 60 characters, for `choice`. */
  choices?: string[];
}

interface Stamped {
  id: string;
  revision: number;
  created_at: string;
  updated_at: string;
}

export interface Workflow extends Stamped {
  name: string;
  description: string;
  /** At most 4,000 bytes. Text: the page never runs it. */
  command: string;
  params: Param[];
  /** At most 4 environment sets, by id; workflow runs only. */
  env_set_ids: string[];
}

export interface Prompt extends Stamped {
  name: string;
  description: string;
  /** At most 32 KiB. */
  body: string;
  params: Param[];
}

export interface EnvKey { name: string; present: boolean }

export interface EnvSet extends Stamped {
  name: string;
  description: string;
  /** Names and `present` only. A value is never in a read. */
  keys: EnvKey[];
  keychain: "ok" | "unavailable" | "blocked";
  /** `planned`: Windows and Linux in the beta, names only. */
  status: "stored" | "planned";
}

export type Kind = "workflow" | "prompt" | "env_set";

/** `GET /api/library`: counts and the keychain state. The `limits` shape is a guess; the doc names the numbers, not the keys. */
export interface LibrarySummary {
  workflows: number;
  prompts: number;
  env_sets: number;
  keychain: "ok" | "unavailable" | "blocked";
  limits?: { workflows?: number; prompts?: number; env_sets?: number; env_keys?: number };
}

/** The writable part of each kind. */
export interface WorkflowDraft { name: string; description: string; command: string; params: Param[]; env_set_ids: string[] }
export interface PromptDraft { name: string; description: string; body: string; params: Param[] }
export interface EnvSetDraft { name: string; description: string; keys: string[] }

/** Guess: a repository entry is the library shape without `id`, `revision` and timestamps, plus the `index` it has in its file. */
export type RepoWorkflow = WorkflowDraft & { index: number };
export type RepoPrompt = PromptDraft & { index: number };
export type RepoEnvSet = { name: string; description: string; keys: string[]; index: number };

export interface Problem { file: string; index: number; code: string }

/** `GET /api/projects/{id}/library/suggestions`. The array names are a guess. */
export interface Suggestions {
  workflows: RepoWorkflow[];
  prompts: RepoPrompt[];
  env_sets: RepoEnvSet[];
  problems: Problem[];
}

export type SuggestionFile = "workflows.json" | "prompts.json" | "env-sets.json";

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
  runWorkflow(id: string, values: Record<string, string>, where: { thread_id: string } | { project_id: string }): Promise<{ terminal_id: string }>;
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
  copySuggestion(projectId: string, file: SuggestionFile, index: number): Promise<void>;
}
