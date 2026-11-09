import { AppWindow, ArrowLeft, ArrowRight, Clock, CloudOff, Crosshair, EyeOff, Hand, MoreHorizontal, Plus, RotateCw, X } from "lucide-react";
import { useEffect, useRef, useState, type MouseEvent } from "react";
import { request } from "../../../api/client";
import type { PickedElement, PreviewTab } from "../../../api/types";
import { useCommand } from "../../../product/shell/shortcuts";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useConnection, useConnectionKind } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { chipId } from "../../../product/composer/composerContext";
import { Button, IconButton } from "../../../ui/controls";
import { Menu, type MenuItem } from "../../../ui/Menu";
import { Presence } from "../../../ui/Presence";
import { SiteIcon } from "./SiteIcon";
import { BrowserRecording } from "./BrowserRecording";
import { PreviewRecordings } from "./previewRecording";
import { PageBridge } from "./PageBridge";
import { useBrowserPanels } from "./browserPanels";
import { inDesktopApp } from "../../../api/shell";
import { DEVICES, TAB_POLL_MS, VIEWPORT, addRecent, isLocalUrl, q, resolveAddress, stepZoom } from "./browserModel";
import { PageView } from "./PageView";
import { StartPage } from "./StartPage";
import { deviceChoice, pageSize, type PageSize } from "./viewportResize";

/** The Browser tab's body when the device has no network and the page needs one. */
function OfflineNotice({ onRetry }: { onRetry(): void }) {
  return (
    <div className="browser-offline" role="status">
      <CloudOff size={16} aria-hidden />
      <b>You’re offline</b>
      <span>Pages that need the network will load when you’re back.</span>
      <Button small icon={<RotateCw size={14} aria-hidden />} onClick={onRetry}>
        Retry
      </Button>
    </div>
  );
}

export function BrowserPanel({ threadId }: { threadId: string }) {
  const moved = useBrowserPanels((state) => state.moved[threadId]);
  const move = useBrowserPanels((state) => state.move);
  return (
    <div className="browser-panels">
      <section className="browser-pane" aria-label="Browser panel">
        <BrowserPane threadId={threadId} excludedTabId={moved} onMove={(id) => move(threadId, id)} />
      </section>
      {moved ? <section className="browser-pane" aria-label="Second Browser panel">
        <BrowserPane threadId={threadId} onlyTabId={moved} onMove={() => move(threadId, null)} />
      </section> : null}
    </div>
  );
}

export function BrowserPane({ threadId, onlyTabId, excludedTabId, onMove, detached = false }: {
  threadId: string;
  onlyTabId?: string;
  excludedTabId?: string;
  onMove?(tabId: string): void;
  detached?: boolean;
}) {
  const base = `/api/threads/${q(threadId)}/previews`;
  const pane = useRef<HTMLDivElement>(null);
  const ownsFocus = () => Boolean(pane.current?.contains(document.activeElement)) ||
    (document.activeElement === document.body && (!onlyTabId || detached));
  const tabs = useAsync(`previews:${threadId}`, (signal) => request<PreviewTab[]>("GET", base, undefined, signal));
  const [activeId, setActiveId] = useState<string | null>(null);
  const [draftTab, setDraftTab] = useState(false);
  const [address, setAddress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recordings] = useState(() => new PreviewRecordings());
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [picking, setPicking] = useState(false);
  const [isPrivate, setPrivate] = useState(false);
  const [profile, setProfile] = useState("");
  /** The device each tab is shown as, while the toolbar is open. */
  const [devices, setDevices] = useState<Record<string, string>>({});
  /** The size a handle has dragged or stepped the page to, before it is committed. */
  const [live, setLive] = useState<PageSize | null>(null);
  const [toolbar, setToolbar] = useState(false);
  const recent = useUi((state) => state.recentUrls);
  const waiting = useUi((state) => state.previewRequest);
  const searchEngine = useUi((state) => state.previewSearchEngine);
  const setPrefs = useUi((state) => state.setPrefs);
  const pushToast = useUi((state) => state.pushToast);
  const reload = tabs.reload;
  useCommand("preview.focusUrl", () => {
    pane.current?.querySelector<HTMLInputElement>("[data-preview-address]")?.focus();
  }, ownsFocus);
  const list = (tabs.data ?? []).filter((tab) => tab.id !== excludedTabId && (!onlyTabId || tab.id === onlyTabId));
  const active = draftTab ? null : (list.find((tab) => tab.id === activeId) ?? list[0] ?? null);

  const remember = (url: string) => {
    setPrefs({ recentUrls: addRecent(useUi.getState().recentUrls, url) });
  };
  const act = async (work: () => Promise<unknown>) => {
    setError(null);
    setBusy(true);
    try {
      await work();
      reload();
    } catch (failure) {
      setError(toFailure(failure).message);
    } finally {
      setBusy(false);
    }
  };
  const go = (raw: string) => {
    if (!active && onlyTabId) return;
    const url = resolveAddress(raw, searchEngine);
    setAddress(url);
    if (!(active?.private ?? isPrivate)) remember(url);
    void act(async () => {
      if (active) {
        await request("POST", `${base}/${q(active.id)}/navigate`, { url });
      } else {
        const how = { private: isPrivate, ...(profile ? { profile } : {}) };
        const made = await request<PreviewTab>("POST", base, { url, ...VIEWPORT, ...how });
        setDraftTab(false);
        setActiveId(made.id);
        setPrivate(false);
      }
    });
  };
  const view = (change: { zoom?: number; appearance?: string; mobile?: boolean }) => {
    if (active) void act(() => request("POST", `${base}/${q(active.id)}/view`, change));
  };
  const reloadPage = (hard: boolean) => {
    if (active) void act(() => request("POST", `${base}/${q(active.id)}/reload`, { hard }));
  };
  const clear = (what: { cookies?: boolean; cache?: boolean }) => {
    if (active) void act(() => request("POST", `${base}/${q(active.id)}/clear`, what));
  };
  const chosen = active ? (devices[active.id] ?? "fit") : "fit";
  const size = { width: active?.width ?? 0, height: active?.height ?? 0 };
  // A resized page is no preset, so the select stops claiming one.
  const device = active ? deviceChoice(chosen, size, DEVICES) : "fit";
  /** What the toolbar reads out, which follows the handles while they move. */
  const shownSize = live ?? size;
  useEffect(() => {
    setLive(null);
  }, [active?.id, toolbar, device]);
  /** One resize for a whole drag or run of key presses, then the tab list catches up. */
  const commitResize = (next: PageSize) => {
    if (!active) return;
    request("POST", `${base}/${q(active.id)}/resize`, next).then(
      () => void reload().then(() => setLive(null)),
      (failure: unknown) => {
        // The page keeps the size it had.
        setLive(null);
        pushToast(`Couldn't resize the page. ${toFailure(failure).message}`);
      },
    );
  };
  /** Rotates the chosen preset, or the size a custom page has reached. */
  const rotate = () => {
    if (!active || device === "fit") return;
    if (device !== "custom") {
      setDevice(device, true);
      return;
    }
    setLive(null);
    void act(() => request("POST", `${base}/${q(active.id)}/resize`, pageSize(active.height, active.width)));
  };
  const setDevice = (id: string, rotate = false) => {
    // Custom is what the select says about a size a handle already set.
    if (!active || id === "custom") return;
    const preset = DEVICES.find((entry) => entry.id === id);
    setDevices((all) => ({ ...all, [active.id]: id }));
    if (!preset?.size) {
      view({ mobile: false });
      return;
    }
    const next = rotate ? { width: active.height, height: active.width } : preset.size;
    void act(async () => {
      await request("POST", `${base}/${q(active.id)}/resize`, next);
      await request("POST", `${base}/${q(active.id)}/view`, { mobile: preset.phone });
    });
  };
  const openWaiting = useRef(go);
  useEffect(() => {
    openWaiting.current = go;
  });
  useEffect(() => {
    if (waiting === null || onlyTabId) return;
    useUi.getState().showInBrowser(null);
    // An empty request only brings the panel forward, for a tab already made.
    if (typeof waiting === "object") {
      setActiveId(waiting.tabId);
      setDraftTab(false);
      reload();
    } else if (waiting) openWaiting.current(waiting);
    else reload();
  }, [waiting, reload, onlyTabId]);
  const onPicked = (picked: PickedElement) => {
    useComposerChips.getState().add(threadId, {
      id: chipId(),
      kind: "annotation",
      url: active?.url ?? "",
      selector: picked.selector,
      text: picked.text,
      html: picked.html,
    });
    useUi.getState().pushToast("Element added to your message");
    setPicking(false);
  };
  const moreItems = (tab: PreviewTab): MenuItem[] => [
    { label: "Hard reload", onSelect: () => reloadPage(true) },
    { label: "Zoom in", disabled: stepZoom(tab.zoom, true) === tab.zoom, onSelect: () => view({ zoom: stepZoom(tab.zoom, true) }) },
    { label: "Zoom out", disabled: stepZoom(tab.zoom, false) === tab.zoom, onSelect: () => view({ zoom: stepZoom(tab.zoom, false) }) },
    { label: "Actual size", disabled: tab.zoom === 1, onSelect: () => view({ zoom: 1 }) },
    {
      label: "Appearance",
      separatorBefore: true,
      children: [
        { label: "System", selected: tab.appearance === "system", onSelect: () => view({ appearance: "system" }) },
        { label: "Light", selected: tab.appearance === "light", onSelect: () => view({ appearance: "light" }) },
        { label: "Dark", selected: tab.appearance === "dark", onSelect: () => view({ appearance: "dark" }) },
      ],
    },
    { label: toolbar ? "Hide device toolbar" : "Show device toolbar", separatorBefore: true, onSelect: () => setToolbar(!toolbar) },
    ...(!detached ? [{ label: "Float over chat", onSelect: () => useUi.getState().setPreviewFloat({ threadId, tabId: tab.id }) }] : []),
    ...(onMove ? [{ label: onlyTabId ? "Move to Browser panel" : "Move to another panel", onSelect: () => onMove(tab.id) }] : []),
    { label: "Open in separate window", disabled: !inDesktopApp(), onSelect: () => void act(() => request("POST", `${base}/${q(tab.id)}/window`)) },
    { label: "Clear cookies", separatorBefore: true, destructive: true, onSelect: () => clear({ cookies: true }) },
    { label: "Clear cache", destructive: true, onSelect: () => clear({ cache: true }) },
  ];
  const step = (back: boolean) => {
    if (!active) return;
    void act(() => request("POST", `${base}/${q(active.id)}/history`, { back }));
  };
  // A link clicked in the page moves the tab with nobody telling this side.
  const watching = active !== null || Boolean(onlyTabId);
  useEffect(() => {
    if (!watching) return;
    const timer = setInterval(reload, TAB_POLL_MS);
    return () => clearInterval(timer);
  }, [watching, reload]);
  const shown = active?.url ?? "";
  useCommand("preview.refresh", () => {
    if (!shown) return false;
    go(shown);
  }, ownsFocus);
  // Keys can outrun the tab list, so each press steps from the last one sent.
  const sentZoom = useRef<number | null>(null);
  const shownZoom = active?.zoom;
  useEffect(() => {
    if (shownZoom === sentZoom.current) sentZoom.current = null;
  }, [shownZoom]);
  const zoomTo = (zoom: (from: number) => number) => {
    if (!active) return false;
    sentZoom.current = zoom(sentZoom.current ?? active.zoom);
    view({ zoom: sentZoom.current });
  };
  useCommand("preview.zoomIn", () => zoomTo((from) => stepZoom(from, true)), ownsFocus);
  useCommand("preview.zoomOut", () => zoomTo((from) => stepZoom(from, false)), ownsFocus);
  useCommand("preview.resetZoom", () => zoomTo(() => 1), ownsFocus);
  const human = active?.control === "human";
  // Offline, a page on this machine still opens; anything else waits for the network. Sending to
  // the thread needs the Orbyn service, so while it is away Pick element is off and says why.
  const connection = useConnectionKind();
  const waitsForNetwork = connection === "offline" && !(active && isLocalUrl(active.url));
  const serviceAway = connection === "service-down";
  const retryNetwork = () => {
    useConnection.getState().setOffline(navigator.onLine === false);
    reload();
  };

  return (
    <div ref={pane} className="browser-panel">
      {waitsForNetwork && list.length === 0 ? null : <div className="browser-tabs" role="tablist" aria-label="Browser tabs">
        {list.map((tab) => (
          <span key={tab.id} className="surface-tab" data-selected={tab.id === active?.id || undefined}>
            <button
              type="button"
              role="tab"
              aria-selected={tab.id === active?.id}
              onClick={() => {
                setDraftTab(false);
                setActiveId(tab.id);
              }}
            >
              <SiteIcon tab={tab} />
              {tab.title || tab.url.replace(/^https?:\/\//, "") || "New tab"}
            </button>
            <IconButton
              label="Close tab"
              onClick={() => void act(() => request("DELETE", `${base}/${q(tab.id)}`))}
            >
              <X size={12} aria-hidden />
            </IconButton>
          </span>
        ))}
        {!onlyTabId ? <><IconButton
          label="New tab"
          onClick={() => {
            setPrivate(false);
            setDraftTab(true);
          }}
        >
          <Plus size={14} aria-hidden />
        </IconButton>
        <IconButton
          label="New private tab"
          onClick={() => {
            setPrivate(true);
            setDraftTab(true);
          }}
        >
          <EyeOff size={14} aria-hidden />
        </IconButton></> : null}
        {active && !detached && inDesktopApp() ? (
          <span className="browser-popout">
            <IconButton
              label="Open in separate window"
              onClick={() => void act(() => request("POST", `${base}/${q(active.id)}/window`))}
            >
              <AppWindow size={14} aria-hidden />
            </IconButton>
          </span>
        ) : null}
        {onlyTabId && onMove ? <IconButton label="Close second Browser panel" onClick={() => onMove(onlyTabId)}><X size={14} aria-hidden /></IconButton> : null}
      </div>}
      {waitsForNetwork ? <OfflineNotice onRetry={retryNetwork} /> : <>
      <div className="browser-bar">
        <IconButton label="Back" disabled={!active?.can_go_back} onClick={() => step(true)}>
          <ArrowLeft size={14} aria-hidden />
        </IconButton>
        <IconButton label="Forward" disabled={!active?.can_go_forward} onClick={() => step(false)}>
          <ArrowRight size={14} aria-hidden />
        </IconButton>
        <IconButton label="Reload" disabled={!active} onClick={() => go(shown)}>
          <RotateCw size={14} aria-hidden />
        </IconButton>
        <input
          key={shown}
          data-preview-address
          list={`browser-history-${onlyTabId ?? "main"}`}
          aria-label="Search or enter URL"
          placeholder="Search or enter URL"
          disabled={Boolean(onlyTabId) && !active}
          defaultValue={shown || address}
          onKeyDown={(event) => {
            if (event.key === "Enter" && event.currentTarget.value.trim()) go(event.currentTarget.value);
          }}
        />
        <datalist id={`browser-history-${onlyTabId ?? "main"}`}>
          {recent.map((url) => (
            <option key={url} value={url} />
          ))}
        </datalist>
        {active ? (
          <>
            {!detached ? <IconButton label={serviceAway ? "Pick element, unavailable" : "Pick an element"} pressed={picking && !serviceAway} disabled={serviceAway} onClick={() => setPicking(!picking)}>
              <Crosshair size={14} aria-hidden />
            </IconButton> : null}
            <IconButton
              label={human ? "Give control back" : "Take control"}
              pressed={human}
              onClick={() => void act(() => request("POST", `${base}/${q(active.id)}/control`, { human: !human }))}
            >
              <Hand size={14} aria-hidden />
            </IconButton>
            <IconButton
              label="More page actions"
              onClick={(event: MouseEvent<HTMLButtonElement>) => {
                const box = event.currentTarget.getBoundingClientRect();
                setMenu({ x: box.right - 180, y: box.bottom + 4 });
              }}
            >
              <MoreHorizontal size={14} aria-hidden />
            </IconButton>
          </>
        ) : null}
      </div>
      {active ? <BrowserRecording key={`${threadId}:${active.id}`} tab={active}
        endpoint={`${base}/${q(active.id)}/recording`} actions={recordings} refresh={async () => {
          // Recording writes need a rejecting read; useAsync.reload intentionally
          // consumes read errors for the general panel polling flow.
          const fresh = await request<PreviewTab[]>("GET", base);
          const target = fresh.find((tab) => tab.id === active.id);
          if (!target) throw new Error("The recorded tab closed before its status could be read.");
          if (recordings.owns(`${base}/${q(active.id)}/recording`)) tabs.setData(fresh);
          return target;
        }} /> : null}
      <Presence>{menu && active ? <Menu x={menu.x} y={menu.y} label="Page actions" items={moreItems(active)} onClose={() => setMenu(null)} /> : null}</Presence>
      {toolbar && active ? (
        <div className="browser-devices" role="toolbar" aria-label="Device toolbar">
          <select aria-label="Device" value={device} onChange={(event) => setDevice(event.target.value)}>
            {DEVICES.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.label}
              </option>
            ))}
            {device === "custom" ? (
              <option value="custom">{`Custom, ${shownSize.width} by ${shownSize.height}`}</option>
            ) : null}
          </select>
          <span>
            {shownSize.width} by {shownSize.height}
          </span>
          <IconButton label="Rotate" disabled={device === "fit"} onClick={rotate}>
            <RotateCw size={14} aria-hidden />
          </IconButton>
        </div>
      ) : null}
      {serviceAway && active ? (
        <p className="conn-banner conn-panel-banner" role="status">
          <Clock size={13} aria-hidden />
          <span>Sending to the thread is paused while the service is away.</span>
        </p>
      ) : null}
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
      {active ? <PageBridge threadId={threadId} tab={active} onChanged={reload} /> : null}
      {active ? (
        <PageView
          threadId={threadId}
          tab={active}
          fitted={!detached && device === "fit"}
          picking={picking && !serviceAway}
          resize={device === "fit" && !detached ? null : { live, handles: device !== "fit" && toolbar, onPreview: setLive, onCommit: commitResize }}
          onPicked={onPicked}
        />
      ) : busy ? (
        <p className="browser-note" role="status">
          Connecting...
        </p>
      ) : onlyTabId ? (
        <p className="browser-note" role="status">{tabs.status === "loading" ? "Connecting..." : "This preview tab is closed."}</p>
      ) : (
        <StartPage threadId={threadId} onOpen={go} profile={profile} setProfile={setProfile} isPrivate={isPrivate} />
      )}
      </>}
    </div>
  );
}
