import type { Thread } from "../../../api/types";
import { CODE_LANGUAGES, openFenceAt, setFenceLanguage } from "./composerCodeBlock";

/** The language picker over an open fenced block in the draft. */
export function CodeBar({
  thread,
  draft,
  fence,
  setDraft,
}: {
  thread: Thread;
  draft: string;
  fence: NonNullable<ReturnType<typeof openFenceAt>>;
  setDraft(threadId: string, text: string): void;
}) {
  return (
    <div className="composer-codebar">
      <span>Code block</span>
      <select
        aria-label="Code block language"
        value={fence.language}
        onChange={(event) => setDraft(thread.id, setFenceLanguage(draft, fence, event.target.value))}
      >
        {CODE_LANGUAGES.map((language) => (
          <option key={language || "plain"} value={language}>
            {language || "Plain text"}
          </option>
        ))}
        {(CODE_LANGUAGES as readonly string[]).includes(fence.language) ? null : (
          <option value={fence.language}>{fence.language}</option>
        )}
      </select>
    </div>
  );
}
