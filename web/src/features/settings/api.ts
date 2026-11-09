// The one door this feature uses to reach the API client. Named operations and
// hooks move here as they are extracted; until then it re-exports what the feature
// already calls, so no file in the feature imports `api/` directly.
export { ApiFailure, NETWORK_ERROR, api, authHeaders, request } from "../../api/client";
export { serviceUrl } from "../../api/origin";
export { inDesktopApp, onShellChange } from "../../api/shell";
export { SseParser } from "../../api/sse";
