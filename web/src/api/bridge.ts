/** What the desktop shell hands a page: the Tauri bridge the window has. Only `shell.ts` and `personKey.ts` read it. */
export type Bridge = { invoke?: (command: string, args?: Record<string, unknown>) => unknown };

export const bridge = (): Bridge | undefined => (window as unknown as { __TAURI_INTERNALS__?: Bridge }).__TAURI_INTERNALS__;
