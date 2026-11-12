import { useEffect, useState } from "react";
import type { AccountApprovalRequest } from "../../generated/AccountApprovalRequest";
import { relativeTime } from "../../lib/format";
import { useAsync } from "../../lib/useAsync";
import { Button } from "../../ui/controls";
import { Dialog } from "../../ui/Dialog";
import { accountApi } from "./api";
import { onAccountChange } from "./changes";
import { Failure } from "./Card";
import { ConfirmItsYou } from "./ConfirmItsYou";
import { NETWORK_TEXT, problemOf } from "./problem";
import { useCountdown, type Wait } from "./useCountdown";

const POLL_MS = 5000;
const appName = (request: AccountApprovalRequest) => (request.app === "engineering" ? "Orbyn Engineering" : "Orbyn Planner");

/** The new devices waiting for this one, read again every few seconds while the Devices page is open. */
function useApprovals() {
  const read = useAsync("account-approvals", (signal) => accountApi.approvals(signal));
  const { reload } = read;
  useEffect(() => onAccountChange(reload), [reload]);
  useEffect(() => {
    const timer = setInterval(reload, POLL_MS);
    return () => clearInterval(timer);
  }, [reload]);
  return read;
}

/** What went wrong with refusing or letting in: a request that is gone reads like any other close. */
function refusalText(error: unknown): string {
  const problem = problemOf(error);
  if (problem.kind === "network") return NETWORK_TEXT;
  if (problem.kind === "gone") return "Denied or cancelled.";
  return problem.kind === "other" ? problem.message : "Couldn’t answer this request. Nothing was changed.";
}

/** A device that asked to sign in, at the top of the Devices page, with Review and Don't allow. */
export function ApprovalRequests() {
  const approvals = useApprovals();
  const [reviewing, setReviewing] = useState<AccountApprovalRequest | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const list = approvals.data ?? [];

  // The request ended (answered, given up, expired or finished by the digits) while its dialog was open.
  const ended = reviewing !== null && approvals.data !== null && !approvals.data.some((request) => request.id === reviewing.id);

  const deny = async (request: AccountApprovalRequest) => {
    if (busy) return;
    setBusy(request.id);
    setProblem(null);
    try {
      await accountApi.deny(request.id);
    } catch (error) {
      setProblem(refusalText(error));
    }
    approvals.reload();
    setBusy(null);
  };

  if (list.length === 0 && !problem && !reviewing) return null;
  return (
    <>
      {problem ? <Failure>{problem}</Failure> : null}
      <ul className="ac-devs ac-waiting">
        {list.map((request) => (
          <li key={request.id} className="ac-row ac-dev" aria-label={`${request.device_name} asks to sign in`}>
            <div>
              <b>{request.device_name}</b>
              <small>asks to sign in</small>
              <small>Waiting for approval{request.near ? ` · ${request.near}` : ""} · {relativeTime(request.asked_at)}</small>
            </div>
            <span className="ac-tags"><span className="ac-tag">{appName(request)}</span></span>
            <Button small disabled={busy !== null} onClick={() => void deny(request)}>Don’t allow</Button>
            <Button small tone="primary" disabled={busy !== null || request.allowed} onClick={() => setReviewing(request)}>Review</Button>
          </li>
        ))}
      </ul>
      {reviewing ? <ApprovePrompt request={reviewing} ended={ended} onClose={() => { setReviewing(null); approvals.reload(); }} /> : null}
    </>
  );
}

/**
 * A new sign-in to allow or refuse. Allow asks for the password for this request, and then shows six digits to type on the
 * new device: allowing never finishes the sign-in. The digits are held here only while the dialog is open and are never
 * kept or logged. Cancel on the digits closes the request.
 */
function ApprovePrompt({ request, ended, onClose }: { request: AccountApprovalRequest; ended: boolean; onClose(): void }) {
  const [confirming, setConfirming] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [refusing, setRefusing] = useState(false);
  const [digits, setDigits] = useState<{ text: string; wait: Wait } | null>(null);
  // The dialog closes when its request ends; the password step stays, so what it says about the request is read.
  useEffect(() => {
    if (ended && !confirming) onClose();
  }, [ended, confirming, onClose]);
  if (confirming) {
    return (
      <ConfirmItsYou
        action="approve_device"
        deviceId={request.id}
        subject={`To let ${request.device_name} sign in`}
        onConfirmed={async (nonce) => {
          const allowed = await accountApi.approve(request.id, nonce);
          setDigits({ text: allowed.digits, wait: { seconds: request.expires_in_secs, at: Date.now() } });
        }}
        onClose={() => setConfirming(false)}
      />
    );
  }
  const refuse = async () => {
    if (refusing) return;
    setRefusing(true);
    try {
      await accountApi.deny(request.id);
      onClose();
    } catch (error) {
      setProblem(refusalText(error));
      setRefusing(false);
    }
  };
  if (digits) return <DigitsDialog request={request} digits={digits.text} wait={digits.wait} refusing={refusing} problem={problem} onCancel={() => void refuse()} />;
  return (
    <Dialog
      title="New sign-in"
      onClose={onClose}
      footer={
        <>
          <Button disabled={refusing} onClick={() => void refuse()}>Don’t allow</Button>
          <Button tone="primary" disabled={refusing} onClick={() => setConfirming(true)}>Allow</Button>
        </>
      }
    >
      <p>Allow it only if you started this sign-in on that device.</p>
      <dl className="ac-facts">
        <dt>Device</dt>
        <dd>{`${appName(request)} on ${request.device_name}`}</dd>
        {request.near ? (
          <>
            <dt>Where</dt>
            <dd>{request.near}</dd>
          </>
        ) : null}
        <dt>When</dt>
        <dd>{relativeTime(request.asked_at)}</dd>
      </dl>
      {problem ? <Failure>{problem}</Failure> : null}
    </Dialog>
  );
}

/** "418 207" read as digits in two groups: "4 1 8, 2 0 7". */
const spoken = (digits: string) => `Digits ${digits.split(" ").map((group) => [...group].join(" ")).join(", ")}`;
const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;

/** The six digits to type on the new device, with the time left. They change each time and are never shown on the new device. */
function DigitsDialog({ request, digits, wait, refusing, problem, onCancel }: { request: AccountApprovalRequest; digits: string; wait: Wait; refusing: boolean; problem: string | null; onCancel(): void }) {
  const left = useCountdown(wait);
  return (
    <Dialog title="Type these digits on the new device" onClose={onCancel} footer={<Button disabled={refusing} onClick={onCancel}>Cancel</Button>}>
      <p>Type these six digits on the {request.device_name} to finish the sign-in. Nothing is signed in until you do.</p>
      <div className="ac-digits">
        <b aria-label={spoken(digits)}>{digits}</b>
        <small>This request expires in {clock(left ?? 0)}.</small>
      </div>
      {problem ? <Failure>{problem}</Failure> : null}
    </Dialog>
  );
}
