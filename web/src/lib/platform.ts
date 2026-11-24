/** The platform the browser reports, or its user agent when the platform is empty. */
export const clientPlatform = (): string => navigator.platform || navigator.userAgent;

/** True when a platform string names macOS, not an iPhone, iPad or iPod user agent. */
export const isMacPlatform = (platform: string): boolean => /mac/i.test(platform) && !/iphone|ipad|ipod/i.test(platform);

/** True when the client's platform is macOS. */
export const isMacClient = (): boolean => isMacPlatform(clientPlatform());
