import type { Page } from "@playwright/test";
import type { Bridge } from "../../src/api/bridge";
import { PERSON_HEADER, sendsPersonKey } from "../../src/api/personKey";

/** The test owns the service's parent pipe and acts as its desktop shell. */
export async function desktopPerson(
  page: Page,
  key: string,
): Promise<(method: string, path: string) => Record<string, string>> {
  if (!key) throw new Error("The desktop fixture did not receive a person key.");
  await page.addInitScript((personKey) => {
    (window as unknown as { __TAURI_INTERNALS__: Bridge }).__TAURI_INTERNALS__ = {
      invoke: async (command) => {
        if (command === "person_key") return personKey;
        throw new Error(`Unexpected desktop fixture command: ${command}`);
      },
    };
  }, key);
  return (method, path): Record<string, string> => sendsPersonKey(method, path) ? { [PERSON_HEADER]: key } : {};
}
