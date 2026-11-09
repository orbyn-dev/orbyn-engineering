import { type ReactNode } from "react";
import { type PullData, type PullFilters, type PullSort } from "../model/pullModel";

export interface ToolbarProject {
  id: string;
  name: string;
  /** A project colour name, as `project-color-*` knows it. */
  color: string;
  count: number;
}

export interface ToolbarProps {
  openCount: number;
  draftCount: number;
  /** While the list loads the pill counts read as an ellipsis, never 0. */
  loading?: boolean;
  /** When the list could not be read they read as an en dash. */
  unavailable?: boolean;
  filters: PullFilters;
  onFilters(next: PullFilters): void;
  sort: PullSort;
  onSort(next: PullSort): void;
  text: string;
  onText(next: string): void;
  tokens: string[];
  onTokens(next: string[]): void;
  /** Runs the typed words as the search. */
  onSubmit(query: string): void;
  projects: ToolbarProject[];
  allCount: number;
  authors: string[];
  labels: { name: string; count: number }[];
  /** How many rows the current filters leave, for the phone sheet's button. */
  shown: number;
  phone: boolean;
  /** Whether the list covers one repository, which Created and Oldest need. */
  oneRepo: boolean;
  /** Which hidden controls the data now supports. */
  has?: PullData;
  /** On a phone the refresh button sits beside the search. */
  phoneRefresh?: ReactNode;
  /** People asked to review, other than the viewer, for the Review requested from group. */
  requesters?: string[];
  /** The signed-in login, which the group offers as Me. */
  viewer?: string | null;
  /** A filter that needs details is chosen and they are on their way, or could not load. */
  detailsWait?: "waiting" | "failed" | "paused";
}
