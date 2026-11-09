// The one door this feature uses to reach the API client. Named operations and
// hooks move here as they are extracted; until then it re-exports what the feature
// already calls, so no file in the feature imports `api/` directly.
export { ApiFailure, api, authHeaders, request } from "../../api/client";
export { serviceSocketUrl, serviceUrl } from "../../api/origin";
export { pullsApi } from "../../api/pulls";
export { inDesktopApp } from "../../api/shell";
export { readToken } from "../../api/token";
