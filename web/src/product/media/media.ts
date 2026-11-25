import { serviceUrl } from "../../api/origin";
import { readToken } from "../../api/token";

export type MediaKind = "image" | "video" | "audio" | "pdf" | "font";

const KINDS: [RegExp, MediaKind][] = [
  [/\.(png|jpe?g|gif|webp|avif|bmp|ico|svg)$/, "image"],
  [/\.(mp4|m4v|webm|mov|ogv)$/, "video"],
  [/\.(mp3|wav|ogg|oga|opus|flac|m4a|aac)$/, "audio"],
  [/\.pdf$/, "pdf"],
  [/\.(woff2?|ttf|otf)$/, "font"],
];

/** What the service shows inline; it decides from the name, so this does too. */
export function mediaKind(path: string): MediaKind | null {
  const name = path.toLowerCase();
  return KINDS.find(([pattern]) => pattern.test(name))?.[1] ?? null;
}

/**
 * Elements can't set a header, so this route takes the token in the query.
 * `repoId` names another repository of a multi-repository thread, whose worktree holds the
 * repo-relative `path`; left out, the route reads the thread's own checkout, as always.
 */
export function rawUrl(threadId: string, path: string, download = false, repoId?: string): string {
  const token = readToken();
  const query = new URLSearchParams({ path });
  if (repoId !== undefined) query.set("repo_id", repoId);
  if (token) query.set("token", token);
  if (download) query.set("download", "true");
  return serviceUrl(`/api/threads/${encodeURIComponent(threadId)}/files/raw?${query}`);
}
