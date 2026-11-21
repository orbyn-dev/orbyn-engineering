/**
 * MOCK of the terminal-attention event of docs/cli-agent-attention.md (provisional), for tests and the dev-only preview
 * (`#/preview/attention`), which installs it through `setAttentionApi`. The app's own default is no client at all: nothing here ships.
 * The scenario is the word after the route, as in `#/preview/attention?secret`: `default`, `yesno`, `secret`, `many`, `cleared`,
 * `hostile`. The excerpts include markup and escape sequences on purpose: the item must draw them as plain text.
 */
import { ATTENTION_EVENT, type AttentionApi, type AttentionDelivery } from "../src/features/panels/terminal/attention/contract.ts";

export const ATTENTION_SCENARIOS = ["default", "yesno", "secret", "many", "cleared", "hostile"] as const;
export type AttentionScenario = (typeof ATTENTION_SCENARIOS)[number];

const THREAD = "preview";

export function createAttentionMock(options: { scenario?: AttentionScenario; delayMs?: number } = {}): AttentionApi {
  const { scenario = "default", delayMs = 50 } = options;
  const now = Date.now();
  let seq = 0;
  const send = (value: Record<string, unknown>): AttentionDelivery => ({
    seq: ++seq, ts: new Date().toISOString(), thread_id: THREAD, event: { type: "CUSTOM", name: ATTENTION_EVENT, value: { v: 1, ...value } },
  });
  const waiting = (id: string, over: Record<string, unknown> = {}) => send({
    op: "waiting", id, terminal_id: `tm-${id}`, terminal_title: "dev", engine: "claude", kind: "question",
    since: new Date(now - 38_000).toISOString(), excerpt: ["Which test command should I run to check the fix?"], ...over,
  });
  const script = (): AttentionDelivery[] => {
    switch (scenario) {
      case "yesno":
        return [waiting("a1", { kind: "yes_no", excerpt: ["Overwrite auth/redirect.ts with the new version? (y/N)"] })];
      case "secret":
        return [waiting("a1", { kind: "secret", excerpt: ["Password:"] })];
      case "many":
        return [
          waiting("a1", { kind: "yes_no", excerpt: ["Overwrite auth/redirect.ts? (y/N)"] }),
          waiting("a2", { terminal_title: "test", engine: "codex", kind: "yes_no", excerpt: ["Apply these 3 changes to auth/session.ts? [Y/n]"] }),
          waiting("a3", { terminal_title: "release", kind: "secret", excerpt: ["Password:"] }),
        ];
      case "cleared":
        return [waiting("a1", { kind: "yes_no", excerpt: ["Overwrite auth/redirect.ts? (y/N)"] }), send({ op: "clear", id: "a1", reason: "input" })];
      case "hostile":
        return [waiting("a1", { terminal_title: "<b>dev</b>", excerpt: ["<img src=x onerror=alert(1)>", "\u001b]8;;http://example.invalid\u0007a link\u001b]8;;\u0007", "\u001b[31mDelete everything? (y/N)\u001b[0m"] })];
      default:
        return [waiting("a1")];
    }
  };
  return {
    subscribe(onEvent) {
      const timers = script().map((delivery, index) => setTimeout(() => onEvent(delivery), delayMs * (index + 1)));
      return () => timers.forEach((timer) => clearTimeout(timer));
    },
  };
}
