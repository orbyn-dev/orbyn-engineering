/** The platform of the browser rendering this view, including a user-agent fallback. */
export const isMacClient = () => /mac/i.test(navigator.platform || navigator.userAgent);
