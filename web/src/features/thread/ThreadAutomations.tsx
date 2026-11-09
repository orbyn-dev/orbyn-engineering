import { CalendarClock, CirclePlay, Pause, Pencil, Play } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { request } from "../../api/client";
import type { ScheduledTask } from "../../api/types";
import { toFailure, useAsync } from "../../lib/useAsync";
import { useNow } from "../../lib/useNow";
import { scheduleLabel, taskTone } from "../../lib/taskSchedule";
import { useWaited } from "../../lib/motion";
import { DRow } from "../../ui/DetailRows";
import { navigate, useRoute } from "../../state/route";
import { useUi } from "../../state/ui";
import { Button, IconButton } from "../../ui/controls";

const q = encodeURIComponent;

/** Tasks bound to this thread, with the same run, pause and edit controls as Settings. */
export function ThreadAutomations({ threadId }: { threadId: string }) {
  const tasks = useAsync(`details-tasks:${threadId}`, (signal) =>
    request<ScheduledTask[]>("GET", "/api/tasks", undefined, signal),
  );
  const [busy, setBusy] = useState<string | null>(null);
  const [pending, setPending] = useState<Record<string, boolean>>({});
  const [saved, setSaved] = useState<{
    threadId: string;
    name: string;
    enabled: boolean;
    rows: ScheduledTask[];
  } | null>(null);
  const rows = useRef(tasks.data);
  const shownThread = useRef<string | null>(threadId);
  useLayoutEffect(() => {
    rows.current = tasks.data;
  });
  useLayoutEffect(() => {
    shownThread.current = threadId;
    return () => { shownThread.current = null; };
  }, [threadId]);
  const pushToast = useUi((state) => state.pushToast);
  const route = useRoute();
  const inSettings = route.name === "settings";
  const wasInSettings = useRef(inSettings);
  const bound = (tasks.data ?? []).filter((task) => task.thread_id === threadId);
  const now = useNow(bound.some((task) => task.enabled && task.next_run_at !== null), 30_000);
  const reload = tasks.reload;
  const dueAt = bound.reduce(
    (earliest, task) =>
      task.enabled && task.next_run_at ? Math.min(earliest, new Date(task.next_run_at).getTime()) : earliest,
    Number.POSITIVE_INFINITY,
  );
  const refreshedAt = useRef(Date.now());

  useEffect(() => {
    if (wasInSettings.current && !inSettings) void reload();
    wasInSettings.current = inSettings;
  }, [inSettings, reload]);

  // A fired task gets a new next_run_at in the service. Re-read once the old one
  // passes so the row cannot stay "overdue" while the task keeps running.
  useEffect(() => {
    if (!Number.isFinite(dueAt) || now < dueAt + 5_000 || now - refreshedAt.current < 60_000) return;
    refreshedAt.current = now;
    void reload();
  }, [dueAt, now, reload]);

  const [expanded, setExpanded] = useState(false);
  const timestampFormat = useUi((state) => state.timestampFormat);
  const waited = useWaited(tasks.status === "loading");
  const icon = <CalendarClock size={16} aria-hidden />;
  if (tasks.status === "loading") return <DRow icon={icon} label="Automations" loading waited={waited} skeleton={80} />;
  if (tasks.status === "error" && bound.length === 0) {
    return (
      <DRow
        icon={icon}
        label="Automations"
        title={tasks.error.message}
        value={<span className="t3">Couldn’t load automations.</span>}
        extra={
          <Button small tone="quiet" onClick={tasks.reload}>
            Retry
          </Button>
        }
      >
        <ManageLink />
      </DRow>
    );
  }
  if (bound.length === 0) return <DRow icon={icon} label="Automations" value={<span className="t3">None on this thread</span>} />;
  const clock = (iso: string) =>
    new Date(iso).toLocaleTimeString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      hour12: timestampFormat === "locale" ? undefined : timestampFormat === "12-hour",
    });
  const upcoming = bound
    .filter((task) => (pending[task.id] ?? task.enabled) && task.next_run_at)
    .map((task) => task.next_run_at as string)
    .sort()[0];

  const toggle = async (task: ScheduledTask, enabled: boolean) => {
    setBusy(task.id);
    setPending((current) => ({ ...current, [task.id]: enabled }));
    setSaved(null);
    let row: ScheduledTask;
    try {
      row = await request<ScheduledTask>("POST", `/api/tasks/${q(task.id)}/enabled`, { enabled });
    } catch (failure) {
      if (shownThread.current === threadId) {
        pushToast(`Couldn't ${enabled ? "resume" : "pause"} ${task.name}. ${toFailure(failure).message}`);
      }
      finish();
      return;
    }

    if (shownThread.current !== threadId) {
      finish();
      return;
    }
    if (rows.current) {
      // The POST returns the saved row. Keep it even when the follow-up GET
      // fails, and fence reads started before the save with setData.
      const next = rows.current.map((entry) => entry.id === row.id ? row : entry);
      rows.current = next;
      tasks.setData(next);
      setSaved({ threadId, name: row.name, enabled: row.enabled, rows: next });
    }
    await reload();
    finish();

    function finish() {
      setBusy(null);
      setPending((current) => {
        const next = { ...current };
        delete next[task.id];
        return next;
      });
    }
  };

  const run = async (task: ScheduledTask) => {
    setBusy(task.id);
    try {
      await request("POST", `/api/tasks/${q(task.id)}/run`);
    } catch (failure) {
      pushToast(`Couldn't run ${task.name}. ${toFailure(failure).message}`);
    } finally {
      // A failed run records its reason in last_status, so refresh in both cases.
      await reload();
      setBusy(null);
    }
  };

  return (
    <DRow
      icon={icon}
      label="Automations"
      ariaLabel="Automations"
      expanded={expanded}
      onClick={() => setExpanded(!expanded)}
      value={<span className="t2">{bound.length === 1 ? "1 task" : `${bound.length} tasks`}</span>}
      sub={
        tasks.status === "error"
          ? "Couldn’t refresh automations."
          : upcoming
            ? `Next run ${clock(upcoming)}`
            : bound.every((task) => !(pending[task.id] ?? task.enabled))
              ? "Paused"
              : undefined
      }
    >
      {expanded ? (
        <>
          <ul className="dauto" aria-label="Automations">
            {bound.map((task) => {
              const tone = taskTone(task);
              const enabled = pending[task.id] ?? task.enabled;
              const when = tone === "fail" ? task.last_status : !enabled ? "Paused" : task.next_run_at ? `Next run ${clock(task.next_run_at)}` : null;
              return (
                <li key={task.id} className="dtask" role="group" aria-label={task.name}>
                  <span className="dtext">
                    <b title={task.name}>{task.name}</b>
                    <small className={tone === "fail" ? "is-failed" : undefined}>{[scheduleLabel(task.schedule), when].filter(Boolean).join(" · ")}</small>
                  </span>
                  <span className="end">
                    <IconButton label={enabled ? "Pause" : "Resume"} disabled={busy !== null} onClick={() => void toggle(task, !enabled)}>
                      {enabled ? <Pause size={14} aria-hidden /> : <CirclePlay size={14} aria-hidden />}
                    </IconButton>
                    {task.schedule.kind === "webhook" ? null : (
                      <IconButton label="Run now" disabled={busy !== null} onClick={() => void run(task)}>
                        <Play size={14} aria-hidden />
                      </IconButton>
                    )}
                    <IconButton
                      label="Edit"
                      disabled={busy !== null}
                      onClick={() => navigate({ name: "settings", section: "tasks", task: task.id, edit: true })}
                    >
                      <Pencil size={14} aria-hidden />
                    </IconButton>
                  </span>
                </li>
              );
            })}
          </ul>
          <ManageLink />
          {tasks.status === "error" ? (
            <p className="detail-automation-error" role="alert">
              {saved?.threadId === threadId && saved.rows === tasks.data
                ? `${saved.enabled ? "Resumed" : "Paused"} ${saved.name}. `
                : ""}
              Couldn't refresh automations. {tasks.error.message}
            </p>
          ) : null}
        </>
      ) : null}
    </DRow>
  );
}

/** The last row of the open Automations row: Scheduled tasks in Settings. */
function ManageLink() {
  return (
    <div className="dauto">
      <div className="dmanage">
        <button type="button" className="dlink" onClick={() => navigate({ name: "settings", section: "tasks" })}>
          Manage scheduled tasks
        </button>
      </div>
    </div>
  );
}
