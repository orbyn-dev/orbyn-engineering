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

/** A device that asked to sign in, at the top of the Devices page, with Compare codes and Don't allow. */
export function ApprovalRequests() {
  const approvals = useApprovals();
  const [comparing, setComparing] = useState<AccountApprovalRequest | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const list = approvals.data ?? [];

  // The request ended (answered, given up or expired) while its prompt was open.
  const ended = comparing !== null && approvals.data !== null && !approvals.data.some((request) => request.id === comparing.id);

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

  if (list.length === 0 && !problem && !comparing) return null;
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
            <Button small tone="primary" disabled={busy !== null} onClick={() => setComparing(request)}>Compare codes</Button>
          </li>
        ))}
      </ul>
      {comparing ? <ApprovePrompt request={comparing} ended={ended} onClose={() => { setComparing(null); approvals.reload(); }} /> : null}
    </>
  );
}

/** Compare the code with the new device's screen: refuse, or allow after the password is typed for this request. */
function ApprovePrompt({ request, ended, onClose }: { request: AccountApprovalRequest; ended: boolean; onClose(): void }) {
  const [confirming, setConfirming] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [refusing, setRefusing] = useState(false);
  // The comparison closes when its request ends; the password step stays, so what it says about the request is read.
  useEffect(() => {
    if (ended && !confirming) onClose();
  }, [ended, confirming, onClose]);
  if (confirming) {
    return (
      <ConfirmItsYou
        action="approve_device"
        deviceId={request.id}
        subject={`To let ${request.device_name} sign in`}
        onConfirmed={(nonce) => accountApi.approve(request.id, nonce)}
        onClose={onClose}
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
  return (
    <Dialog
      title="New sign-in"
      onClose={onClose}
      footer={
        <>
          <Button disabled={refusing} onClick={() => void refuse()}>Don’t allow</Button>
          <Button tone="primary" disabled={refusing} onClick={() => setConfirming(true)}>Codes match · Allow</Button>
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
