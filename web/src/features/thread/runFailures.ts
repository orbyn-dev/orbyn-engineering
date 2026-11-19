/** The two engine failures whose answer is to send the message again.
 *
 * The words are the core's, word for word, and part of the run contract
 * (docs/engine-run-contract.md): a run that produced no assistant output
 * (crates/orbyn-core/src/conversation/runs/turn.rs, NO_REPLY) and a run whose
 * conversation the engine no longer has (execute.rs). Nothing ran in either
 * case, so the person's last message can be sent again as a new run.
 */
export const NO_REPLY_FAILURE = "No reply: the engine finished without answering. Send your message again.";
export const LOST_CONVERSATION_FAILURE = "The engine lost this conversation.";

/** Whether a failed run's message names a failure that resend answers. */
export function failureWantsResend(text: string): boolean {
  return text === NO_REPLY_FAILURE || text === LOST_CONVERSATION_FAILURE;
}
