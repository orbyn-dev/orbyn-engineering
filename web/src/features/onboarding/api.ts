// The one door this feature uses to reach the API client. Named operations and
// hooks live here; no file in the feature calls the transport directly.
export { readPreferences, queryPreferences, updatePreferences } from "../../api/preferences";
export { api } from "../../api/client";
