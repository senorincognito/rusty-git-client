import { useCallback, useEffect, useState } from "react";
import { confirmDialog } from "@/api/dialog";
import {
  getDivergence,
  getSyncStatus,
  gitFetch,
  gitForcePush,
  gitPull,
  gitPullWith,
  gitPush,
  type Divergence,
  type SyncStatus,
} from "@/api/sync";
import ContextMenu, { type MenuItem } from "@/components/ContextMenu";
import { useAutoFetch } from "@/hooks/useAutoFetch";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { usePersistentState } from "@/hooks/usePersistentState";
import { t } from "@/i18n";
import PullDialog from "./PullDialog";
import PushDialog from "./PushDialog";
import "./Toolbar.scss";

type Op = "fetch" | "pull" | "push" | "force";
type MenuKind = "fetch" | "pull" | "push";
type Notice = { kind: "ok" | "error"; text: string };

const OPS: Record<Op, { label: string; run: (path: string) => Promise<string> }> = {
  fetch: { label: t.sync.fetch, run: gitFetch },
  pull: { label: t.sync.pull, run: gitPull },
  push: { label: t.sync.push, run: gitPush },
  force: { label: t.sync.forcePush, run: gitForcePush },
};

// What git says when a pull can't fast-forward.
const isNotFastForward = (msg: string) => /not possible to fast-forward|diverging branches/i.test(msg);

const AUTO_FETCH_CHOICES = [60, 180, 300, 600]; // seconds
const intervalLabel = (secs: number) => t.sync.minutes(secs / 60);

const AUTO_STATE_HINT = {
  fetching: { icon: "⟳", text: t.sync.autoFetchFetching },
  auth: {
    icon: "⚠",
    text: t.sync.autoFetchAuth,
  },
  waiting: {
    icon: "…",
    text: t.sync.autoFetchWaiting,
  },
} as const;

export default function SyncBar({
  path,
  refreshKey,
  onFetchError,
}: {
  path: string;
  refreshKey: number;
  /** Called with what git said when fetching fails (manually or in the background), null once it works. */
  onFetchError: (message: string | null) => void;
}) {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [busy, setBusy] = useState<Op | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  // The last manual fetch failed (cleared by any successful fetch or pull).
  const [manualFetchError, setManualFetchError] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; kind: MenuKind } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  // Set while the user has to choose how to combine diverged branches.
  const [divergence, setDivergence] = useState<Divergence | null>(null);
  // Set when Push is pressed on a diverged branch: offers a force push instead.
  const [pushDivergence, setPushDivergence] = useState<Divergence | null>(null);
  const start = useLatestRequest();

  // Auto-fetch: on by default, every 3 minutes. Global settings, remembered between sessions.
  const [autoOn, setAutoOn] = usePersistentState("autoFetch.enabled", true, (v): v is boolean => typeof v === "boolean");
  const [autoSecs, setAutoSecs] = usePersistentState(
    "autoFetch.seconds",
    180,
    (v): v is number => typeof v === "number" && Number.isFinite(v) && v >= 30,
  );
  const auto = useAutoFetch({ path, enabled: autoOn, seconds: autoSecs, blocked: busy !== null });

  const fetchError = manualFetchError ?? auto.error;
  useEffect(() => {
    onFetchError(fetchError);
  }, [fetchError, onFetchError]);
  useEffect(() => () => onFetchError(null), [onFetchError]);

  const refresh = useCallback(async () => {
    const isCurrent = start();
    try {
      const s = await getSyncStatus(path);
      if (isCurrent()) setStatus(s);
    } catch {
      if (isCurrent()) setStatus(null);
    }
  }, [path, start]);

  // Reload when the repo changes (a fetch updates remote refs, which the watcher reports).
  useEffect(() => {
    refresh();
  }, [refresh, refreshKey]);

  useEffect(() => {
    setNotice(null);
    setManualFetchError(null);
  }, [path]);

  // Successful results fade away; errors stay until dismissed.
  useEffect(() => {
    if (notice?.kind !== "ok") return;
    const t = setTimeout(() => setNotice(null), 5000);
    return () => clearTimeout(t);
  }, [notice]);

  const run = async (op: Op) => {
    setBusy(op);
    setNotice(null);
    try {
      const out = await OPS[op].run(path);
      setNotice({ kind: "ok", text: out || t.sync.complete(OPS[op].label) });
      if (op !== "push" && op !== "force") setManualFetchError(null);
      auto.resume(); // the remote works and credentials are fine
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
      if (op === "fetch") setManualFetchError(String(e));
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const openDivergence = async () => {
    try {
      setDivergence(await getDivergence(path));
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    }
  };

  // Pull fast-forwards. When the branches have diverged (known beforehand, or discovered by the
  // pull's own fetch) the user chooses between merge and rebase instead of seeing an error.
  const pull = async () => {
    if (status && status.ahead > 0 && status.behind > 0) return openDivergence();
    setBusy("pull");
    setNotice(null);
    try {
      const out = await gitPull(path);
      setNotice({ kind: "ok", text: out || t.sync.pullComplete });
      setManualFetchError(null); // a pull fetches first
      auto.resume();
    } catch (e) {
      const text = String(e);
      if (isNotFastForward(text)) {
        await refresh();
        await openDivergence();
      } else {
        setNotice({ kind: "error", text });
      }
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const pullWith = async (mode: "merge" | "rebase") => {
    setDivergence(null);
    setBusy("pull");
    setNotice(null);
    try {
      const out = await gitPullWith(path, mode);
      setNotice({ kind: "ok", text: out || t.sync.pullComplete });
      auto.resume();
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
      refresh();
    }
  };

  // A branch that has diverged from its upstream can't be pushed normally: suggest a force push or abort. Only the
  // last known state is checked (no network); a push rejected because the remote moved shows git's own message.
  const push = async () => {
    if (status && status.upstream && status.ahead > 0 && status.behind > 0) {
      try {
        setPushDivergence(await getDivergence(path));
      } catch (e) {
        setNotice({ kind: "error", text: String(e) });
      }
      return;
    }
    await run("push");
  };

  const forcePush = async () => {
    if (!status?.upstream) return;
    const message = t.sync.forcePushConfirm(status.branch, status.upstream, status.behind);
    if (await confirmDialog(message, t.sync.forcePush, true, t.sync.forcePush)) await run("force");
  };

  const noRemote = status ? !status.hasRemote : false;
  const noBranch = status ? status.branch === null : true;
  // Manual operations wait while a background fetch runs, so two fetches never collide.
  const locked = busy !== null || auto.state === "fetching";
  // A force push in progress is shown on the Push button it came from.
  const shown = (op: Op) => busy === op || (op === "push" && busy === "force");
  const btn = (op: Op, disabled: boolean, title: string, badge?: string, action: () => void = () => run(op)) => (
    <button className="syncbtn" disabled={locked || disabled} title={title} onClick={action}>
      {shown(op) ? `${OPS[op].label}…` : OPS[op].label}
      {badge && !shown(op) && <span className="badge-count">{badge}</span>}
    </button>
  );

  // A button with a small arrow (and right-click) that opens a menu of related actions.
  const withMenu = (kind: MenuKind, label: string, button: React.ReactNode) => (
    <div
      className="splitbtn"
      onContextMenu={(e) => {
        e.preventDefault();
        setMenu({ x: e.clientX, y: e.clientY, kind });
      }}
    >
      {button}
      {/* stopPropagation keeps the menu's outside-click handler from closing it right before this toggles it */}
      <button
        className="syncbtn splitarrow"
        aria-haspopup="menu"
        aria-expanded={menu?.kind === kind}
        title={label}
        disabled={busy !== null}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          if (menu?.kind === kind) return closeMenu();
          const r = e.currentTarget.getBoundingClientRect();
          setMenu({ x: r.left, y: r.bottom + 4, kind });
        }}
      >
        ▾
      </button>
    </div>
  );

  const fetchMenuItems: MenuItem[] = [
    {
      label: t.sync.autoFetch,
      checked: autoOn,
      title: t.sync.autoFetchHint,
      onClick: () => setAutoOn(!autoOn),
    },
    ...AUTO_FETCH_CHOICES.map((secs, i) => ({
      label: t.sync.every(intervalLabel(secs)),
      checked: autoSecs === secs,
      disabled: !autoOn,
      separatorBefore: i === 0,
      onClick: () => setAutoSecs(secs),
    })),
  ];

  // Pull with an explicit strategy, skipping the "diverged" dialog.
  const pullDisabledReason = noRemote
    ? t.sync.noRemotes
    : noBranch
      ? t.sync.checkOutBranch
      : !status?.upstream
        ? t.sync.noUpstreamToPull
        : null;
  const pullMenuItems: MenuItem[] = [
    {
      label: t.sync.pullMerge,
      disabled: locked || pullDisabledReason !== null,
      title:
        pullDisabledReason ??
        t.sync.pullMergeHint(status?.upstream),
      onClick: () => pullWith("merge"),
    },
    {
      label: t.sync.pullRebase,
      disabled: locked || pullDisabledReason !== null,
      title:
        pullDisabledReason ??
        t.sync.pullRebaseHint(status?.upstream),
      onClick: () => pullWith("rebase"),
    },
  ];

  const pushMenuItems: MenuItem[] = [
    {
      label: t.sync.forcePush,
      danger: true,
      disabled: busy !== null || noRemote || noBranch || !status?.upstream,
      title: noRemote
        ? t.sync.noRemotes
        : noBranch
          ? t.sync.checkOutBranch
          : !status?.upstream
            ? t.sync.notPushedYet
            : t.sync.forcePushHint(status.upstream),
      onClick: forcePush,
    },
  ];

  const fetchTitle = noRemote
    ? t.sync.noRemotes
    : t.sync.fetchAll +
      (autoOn
        ? t.sync.autoFetchEvery(intervalLabel(autoSecs)) +
          (auto.lastFetchedAt
            ? t.sync.lastAt(new Date(auto.lastFetchedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }))
            : "")
        : t.sync.autoFetchOff);
  const hint = auto.state === "fetching" || auto.state === "auth" || auto.state === "waiting" ? AUTO_STATE_HINT[auto.state] : null;

  return (
    <>
      <div className="syncbar">
        {hint && (
          <span className={`autofetch ${auto.state}`} title={hint.text} aria-label={hint.text}>
            {hint.icon}
          </span>
        )}
        {withMenu("fetch", t.sync.autoFetchSettings, btn("fetch", noRemote, fetchTitle))}
        {withMenu(
          "pull",
          t.sync.morePull,
          btn(
            "pull",
            noRemote || noBranch || !status?.upstream,
            !status?.upstream
              ? t.sync.noUpstream
              : status.ahead > 0 && status.behind > 0
                ? t.sync.diverged(status.upstream, status.ahead, status.behind)
                : t.sync.pullFrom(status.upstream),
            status?.behind ? `↓${status.behind}` : undefined,
            pull,
          ),
        )}
        {withMenu(
          "push",
          t.sync.morePush,
          btn(
            "push",
            noRemote || noBranch,
            status?.upstream ? t.sync.pushTo(status.upstream) : t.sync.publish,
            status?.upstream ? (status.ahead ? `↑${status.ahead}` : undefined) : t.sync.newBadge,
            push,
          ),
        )}
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={menu.kind === "fetch" ? fetchMenuItems : menu.kind === "pull" ? pullMenuItems : pushMenuItems}
        />
      )}
      {divergence && (
        <PullDialog
          divergence={divergence}
          onMerge={() => pullWith("merge")}
          onRebase={() => pullWith("rebase")}
          onCancel={() => setDivergence(null)}
        />
      )}
      {pushDivergence && (
        <PushDialog
          divergence={pushDivergence}
          onForcePush={() => {
            setPushDivergence(null);
            run("force");
          }}
          onCancel={() => setPushDivergence(null)}
        />
      )}
      {notice && (
        <div className={`notice ${notice.kind}`} onClick={() => setNotice(null)} title={t.sync.dismiss}>
          <pre>{notice.text}</pre>
        </div>
      )}
    </>
  );
}
