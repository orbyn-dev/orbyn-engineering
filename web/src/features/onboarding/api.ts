// The one door this feature uses to reach the API client. Named operations and
// hooks move here as they are extracted; until then it re-exports what the feature
// already calls, so no file in the feature imports `api/` directly.
export { api, request } from "../../api/client";
