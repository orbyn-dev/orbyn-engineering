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
import { problemOf } from "./problem";

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

/** A device that asked to sign in, at the top of the Devices page, with Compare codes and Don't allow. */
export function ApprovalRequests() {
  const approvals = useApprovals();
  const [comparing, setComparing] = useState<AccountApprovalRequest | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const list = approvals.data ?? [];

  const deny = async (request: AccountApprovalRequest) => {
    setProblem(null);
    try {
      await accountApi.deny(request.id);
    } catch (error) {
      setProblem(problemOf(error).kind === "network" ? "Couldn’t reach Orbyn. Nothing was changed." : "Denied or cancelled.");
    }
    approvals.reload();
  };

  if (list.length === 0 && !problem) return null;
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
            <Button small onClick={() => void deny(request)}>Don’t allow</Button>
            <Button small tone="primary" onClick={() => setComparing(request)}>Compare codes</Button>
          </li>
        ))}
      </ul>
      {comparing ? <ApprovePrompt request={comparing} onClose={() => { setComparing(null); approvals.reload(); }} /> : null}
    </>
  );
}

/** Compare the code with the new device's screen: refuse, or allow after the password is typed for this request. */
function ApprovePrompt({ request, onClose }: { request: AccountApprovalRequest; onClose(): void }) {
  const [confirming, setConfirming] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  if (confirming) {
    return (
      <ConfirmItsYou
        action="approve_device"
        deviceId={request.id}
        subject={`To let ${request.device_name} sign in`}
        onConfirmed={() => accountApi.approve(request.id)}
        onClose={onClose}
      />
    );
  }
  const refuse = async () => {
    try {
      await accountApi.deny(request.id);
      onClose();
    } catch {
      setProblem("Denied or cancelled.");
    }
  };
  return (
    <Dialog
      title="New sign-in"
      onClose={onClose}
      footer={
        <>
          <Button onClick={() => void refuse()}>Codes don’t match</Button>
          <Button tone="primary" onClick={() => setConfirming(true)}>Codes match · Allow</Button>
        </>
      }
    >
      <p>Compare this code with the one on the new device. Only allow it if they match.</p>
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
      <div className="ac-digits">
        <b>{request.digits}</b>
        <small>Check that the {request.device_name} shows the same code.</small>
      </div>
      {problem ? <Failure>{problem}</Failure> : null}
    </Dialog>
  );
}
