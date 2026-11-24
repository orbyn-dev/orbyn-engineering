/** The platform the browser reports, or its user agent when the platform is empty. */
export const clientPlatform = (): string => navigator.platform || navigator.userAgent;

/** True when the client's platform is macOS. */
export const isMacClient = (): boolean => /mac/i.test(clientPlatform());
