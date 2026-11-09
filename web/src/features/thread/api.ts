// The one door this feature uses to reach the API client. Named operations and
// hooks move here as they are extracted; until then it re-exports what the feature
// already calls, so no file in the feature imports `api/` directly.
export { ApiFailure, api, attachmentBlob, request } from "../../api/client";
export { pullsApi } from "../../api/pulls";
export { inDesktopApp } from "../../api/shell";
