import { useUi } from "../state/ui";

export async function copyText(text: string, done: string) {
  const { pushToast } = useUi.getState();
  try {
    await navigator.clipboard.writeText(text);
    pushToast(done, "copied");
  } catch {
    pushToast("Couldn't copy.");
  }
}
