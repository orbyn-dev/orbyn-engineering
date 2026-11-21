import { useState } from "react";
import { checkSearchAddress, customSearchUrl, SEARCH_PREVIEW_WORDS } from "../../panels";
import { useUi } from "../../../product/state/ui";
import { settingsLabel } from "../settingsSearch";
import { SettingRow } from "../rows/SettingRow";

/** The Browser panel's search engine, and for Custom the address, the sentence for a bad one and a preview (parity H2). */
export function SearchEngineRows() {
  const engine = useUi((state) => state.previewSearchEngine);
  const saved = useUi((state) => state.previewSearchAddress);
  const setPrefs = useUi((state) => state.setPrefs);
  const [draft, setDraft] = useState(saved);
  const custom = engine === "custom";
  const check = checkSearchAddress(draft);
  const problem = !check.ok ? check.message : null;
  const change = (next: string) => {
    setDraft(next);
    // Saved as it is typed once it is valid; while it is invalid the last valid address stays in use.
    if (checkSearchAddress(next).ok) setPrefs({ previewSearchAddress: next.trim() });
  };
  const meanwhile = saved && checkSearchAddress(saved).ok ? "the last saved address" : "DuckDuckGo";
  const preview = check.ok ? customSearchUrl(draft, SEARCH_PREVIEW_WORDS) : null;
  return (
    <>
      <SettingRow title={settingsLabel("chat-search-engine")} description="For the Browser panel’s address bar">
        <select aria-label="Search engine" value={engine} onChange={(event) => {
          setDraft(saved);
          setPrefs({ previewSearchEngine: event.target.value });
        }}>
          <option value="duckduckgo">DuckDuckGo</option>
          <option value="google">Google</option>
          <option value="bing">Bing</option>
          <option value="brave">Brave</option>
          <option value="kagi">Kagi</option>
          <option value="startpage">Startpage</option>
          <hr />
          <option value="custom">Custom…</option>
        </select>
      </SettingRow>
      {custom ? (
        <>
          <SettingRow title="Search address" description="Use %s where the search words go. Start with https:// or http://.">
            <div className="search-address">
              <input
                className="mono"
                type="text"
                aria-label="Search address"
                aria-invalid={problem ? true : undefined}
                aria-describedby={problem ? "search-address-error" : undefined}
                placeholder="https://example.com/search?q=%s"
                value={draft}
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => change(event.target.value)}
              />
              {problem ? <p id="search-address-error" className="field-error" role="alert">{problem}</p> : null}
            </div>
          </SettingRow>
          <SettingRow title="Preview" description={`What typing “${SEARCH_PREVIEW_WORDS}” in the address bar opens`}>
            {preview ? (
              <span className="mono search-address-preview">{preview}</span>
            ) : (
              <span className="search-address-preview">{`Not used yet. Searches use ${meanwhile} until ${check.ok || check.reason !== "empty" ? "this is fixed" : "an address is added"}.`}</span>
            )}
          </SettingRow>
        </>
      ) : null}
    </>
  );
}
