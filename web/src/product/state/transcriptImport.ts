import { create } from "zustand";
import type { Project } from "../../api/types";

/** One file chosen for one project; `nonce` makes a second choice a fresh sheet. */
export interface TranscriptImportRequest {
  project: Project;
  file: File;
  nonce: number;
}

interface TranscriptImportState {
  request: TranscriptImportRequest | null;
  open(project: Project, file: File): void;
  close(): void;
}

let requestNonce = 0;

/** The file picker reads this in the thread feature; the sidebar only asks for it. */
export const useTranscriptImport = create<TranscriptImportState>((set) => ({
  request: null,
  open(project, file) {
    requestNonce += 1;
    set({ request: { project, file, nonce: requestNonce } });
  },
  close() {
    set({ request: null });
  },
}));

export const openTranscriptImport = (project: Project, file: File): void =>
  useTranscriptImport.getState().open(project, file);

type Picker = (project: Project) => void;

let picker: Picker | null = null;

/** The mounted import host offers its hidden file input; `null` when no host is mounted. */
export function setTranscriptPicker(next: Picker | null): void {
  picker = next;
}

/** Opens the system file picker for a project. Called from the folder menu inside a user gesture. */
export function pickTranscript(project: Project): void {
  picker?.(project);
}
