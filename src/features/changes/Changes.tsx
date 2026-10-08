import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  createCommit,
  getHeadCommit,
  discardPaths,
  getStatus,
  stagePaths,
  unstagePaths,
  type ChangeKind,
  type FileChange,
  type HeadCommit,
} from "@/api/changes";
import { confirmDialog } from "@/api/dialog";
import { stashPaths } from "@/api/stash";
import ContextMenu, { type MenuItem } from "@/components/ContextMenu";
import FileBadge from "@/components/FileBadge";
import Splitter from "@/components/Splitter";
import { followSelection } from "@/hooks/followSelection";
import { useArrowKeys } from "@/hooks/useArrowKeys";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { usePersistentState } from "@/hooks/usePersistentState";
import { t } from "@/i18n";
import StashDialog from "./StashDialog";
import "./Changes.scss";

function FileList(props: {
  title: string;
  files: { path: string; kind: ChangeKind }[];
  actionLabel: string;
  onAction: (paths: string[]) => void;
  onActionAll: () => void;
  /** The file whose diff is open in the centre, if it is in this list. */
  selectedPath: string | null;
  onSelect: (file: { path: string; kind: ChangeKind }) => void;
  /** Right-click on a row. */
  onContextMenu: (file: { path: string; kind: ChangeKind }, x: number, y: number) => void;
  /** The file whose context menu is open, if it is in this list. */
  menuPath: string | null;
  /** The share of the panel's list height this list gets. */
  grow: number;
}) {
  const { title, files, actionLabel, onAction, onActionAll, selectedPath, onSelect, onContextMenu, menuPath, grow } =
    props;
  return (
    <section className="filelist" style={{ flexGrow: grow }}>
      <header>
        <span>
          {title} <span className="count">{files.length}</span>
        </span>
        {files.length > 0 && (
          <button className="ghost" onClick={onActionAll}>
            {t.changes.actionAll(actionLabel)}
          </button>
        )}
      </header>
      <ul>
        {files.map((f) => (
          <li
            key={f.path}
            className={"selectable" + (f.path === selectedPath ? " selected" : "") + (f.path === menuPath ? " ctx" : "")}
            title={f.path}
            role="button"
            tabIndex={0}
            onClick={() => onSelect(f)}
            onContextMenu={(e) => {
              e.preventDefault();
              onContextMenu(f, e.clientX, e.clientY);
            }}
            onKeyDown={(e) => {
              // Only for the row itself, not for keys pressed on the Stage/Unstage button inside it.
              if (e.target === e.currentTarget && (e.key === "Enter" || e.key === " ")) {
                e.preventDefault();
                onSelect(f);
              }
            }}
          >
            <FileBadge kind={f.kind} />
            <span className="fname">{f.path}</span>
            <button
              className="ghost"
              onClick={(e) => {
                e.stopPropagation(); // staging a file must not also open its diff
                onAction([f.path]);
              }}
            >
              {actionLabel}
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

export default function Changes({
  path,
  refreshKey = 0,
  hidden = false,
  selected = null,
  onSelectFile,
  onFileHistory,
  onCommitted,
}: {
  path: string;
  refreshKey?: number;
  /** Keep mounted (so the draft message survives) but not visible. */
  hidden?: boolean;
  /** The file whose diff is open in the centre (a file can be listed both staged and unstaged). */
  selected?: { path: string; staged: boolean } | null;
  onSelectFile: (file: { path: string; staged: boolean; status: ChangeKind }) => void;
  /** Show the commits that changed this file (right-click menu). */
  onFileHistory: (path: string) => void;
  onCommitted: () => void;
}) {
  const [changes, setChanges] = useState<FileChange[]>([]);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [amend, setAmend] = useState(false);
  const [stashOpen, setStashOpen] = useState(false);
  const [head, setHead] = useState<HeadCommit | null>(null);
  // Right-click menu on a file row (which list it was opened in decides what it offers).
  const [menu, setMenu] = useState<{ x: number; y: number; path: string; kind: ChangeKind; staged: boolean } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const draft = useRef(""); // the message typed before switching amend on

  const start = useLatestRequest();
  const refresh = useCallback(async () => {
    const isCurrent = start();
    try {
      const c = await getStatus(path);
      if (!isCurrent()) return;
      setChanges(c);
      setError(null);
    } catch (e) {
      if (isCurrent()) setError(String(e));
    }
  }, [path, start]);

  useEffect(() => {
    setMessage("");
    setAmend(false);
    draft.current = "";
    refresh();
    // Pick up edits made outside the app when the window regains focus.
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [path, refresh]);

  // Reload when the parent reports a repo change (e.g. a commit or checkout made elsewhere).
  useEffect(() => {
    refresh();
  }, [refreshKey, refresh]);

  // The last commit: used to pre-fill the amend message and to warn about rewriting pushed history.
  const startHead = useLatestRequest();
  useEffect(() => {
    const isCurrent = startHead();
    getHeadCommit(path)
      .then((h) => isCurrent() && setHead(h))
      .catch(() => isCurrent() && setHead(null));
  }, [path, refreshKey, startHead]);

  // The last commit disappeared (e.g. reset in a terminal): there is nothing left to amend.
  useEffect(() => {
    if (amend && !head) setAmend(false);
  }, [amend, head]);

  const toggleAmend = (on: boolean) => {
    if (on) {
      draft.current = message;
      setMessage(head?.message ?? "");
    } else {
      setMessage(draft.current);
    }
    setAmend(on);
  };

  const staged = useMemo(
    () => changes.filter((c) => c.staged).map((c) => ({ path: c.path, kind: c.staged! })),
    [changes],
  );
  const unstaged = useMemo(
    () => changes.filter((c) => c.unstaged).map((c) => ({ path: c.path, kind: c.unstaged! })),
    [changes],
  );

  // While a file's diff is open, Up/Down open the file above/below it: the unstaged files, then the staged ones.
  useArrowKeys(!hidden && selected !== null, (step) => {
    if (!selected) return false;
    const all = [...unstaged.map((f) => ({ ...f, staged: false })), ...staged.map((f) => ({ ...f, staged: true }))];
    const at = all.findIndex((f) => f.path === selected.path && f.staged === selected.staged);
    if (at < 0) return false;
    const next = all[at + step];
    if (next) onSelectFile({ path: next.path, staged: next.staged, status: next.kind });
    return true;
  });
  const root = useRef<HTMLElement>(null);
  // How the height of the two lists is shared: the unstaged list gets this fraction.
  const [split, setSplit] = usePersistentState<number>(
    "changesSplit",
    0.5,
    (v): v is number => typeof v === "number" && v > 0 && v < 1,
  );
  const dragSplit = useRef({ from: 0, total: 1 });
  const beginSplit = () => {
    const lists = root.current?.querySelectorAll<HTMLElement>(".filelist");
    if (!lists || lists.length < 2) return;
    dragSplit.current = { from: lists[0].offsetHeight, total: lists[0].offsetHeight + lists[1].offsetHeight || 1 };
  };
  const splitTo = (px: number) => {
    const { total } = dragSplit.current;
    const margin = Math.min(0.4, 56 / total); // a list keeps at least its header and a row or two
    setSplit(Math.min(Math.max(px / total, margin), 1 - margin));
  };
  useEffect(() => {
    followSelection(root.current, ".filelist li.selected");
  }, [selected]);

  const run = async (op: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await op();
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      await refresh();
      setBusy(false);
    }
  };

  // Discard throws the edits away for good: ask first, and say what happens to the file.
  const discard = async (file: { path: string; kind: ChangeKind }) => {
    const text =
      file.kind === "new"
        ? t.changes.deleteUntrackedConfirm(file.path)
        : file.kind === "deleted"
          ? t.changes.restoreConfirm(file.path)
          : t.changes.discardConfirm(file.path, staged.some((f) => f.path === file.path));
    const ok = file.kind === "deleted" ? t.changes.restoreOk : t.changes.discardOk;
    if (!(await confirmDialog(text, t.changes.discardTitle, true, ok))) return;
    await run(async () => {
      await discardPaths(path, [file.path]);
      onCommitted(); // reloads everything and closes a diff that is no longer true
    });
  };

  const stashFile = (file: { path: string }) =>
    run(async () => {
      await stashPaths(path, [file.path]);
      onCommitted();
    });

  const menuItems = (m: NonNullable<typeof menu>): MenuItem[] => {
    const file = { path: m.path, kind: m.kind };
    const stashItem: MenuItem = {
      label: t.changes.stash,
      disabled: busy,
      title: t.changes.stashFileHint,
      onClick: () => stashFile(file),
    };
    const historyItem: MenuItem = {
      label: t.changes.fileHistory,
      // A file that is new has no past yet.
      disabled: m.kind === "new",
      title: m.kind === "new" ? t.changes.fileHistoryNew : t.changes.fileHistoryHint,
      onClick: () => onFileHistory(m.path),
    };
    if (m.staged) {
      return [
        { label: t.changes.unstage, disabled: busy, onClick: () => run(() => unstagePaths(path, [m.path])) },
        stashItem,
        { ...historyItem, separatorBefore: true },
      ];
    }
    return [
      { label: t.changes.stage, disabled: busy, onClick: () => run(() => stagePaths(path, [m.path])) },
      {
        label: m.kind === "new" ? t.changes.deleteFile : m.kind === "deleted" ? t.changes.restoreFile : t.changes.discard,
        danger: true,
        disabled: busy || m.kind === "conflicted",
        title:
          m.kind === "conflicted"
            ? t.changes.conflicted
            : t.changes.discardHint,
        onClick: () => discard(file),
      },
      stashItem,
      { ...historyItem, separatorBefore: true },
    ];
  };

  const commit = () =>
    run(async () => {
      await createCommit(path, message, amend);
      setMessage("");
      setAmend(false);
      draft.current = "";
      onCommitted();
    });

  // An amend may change only the message, so it doesn't need staged files.
  const canCommit = !busy && message.trim().length > 0 && (amend || staged.length > 0);

  return (
    <aside className="changes" ref={root} style={hidden ? { display: "none" } : undefined}>
      <FileList
        title={t.changes.unstaged}
        grow={split}
        files={unstaged}
        selectedPath={selected && !selected.staged ? selected.path : null}
        onSelect={(f) => onSelectFile({ path: f.path, staged: false, status: f.kind })}
        menuPath={menu && !menu.staged ? menu.path : null}
        onContextMenu={(f, x, y) => setMenu({ x, y, path: f.path, kind: f.kind, staged: false })}
        actionLabel={t.changes.stage}
        onAction={(p) => run(() => stagePaths(path, p))}
        onActionAll={() =>
          run(() =>
            stagePaths(
              path,
              unstaged.map((f) => f.path),
            ),
          )
        }
      />
      <Splitter
        onStart={beginSplit}
        onMove={(dy) => splitTo(dragSplit.current.from + dy)}
        onNudge={(dy) => splitTo(dragSplit.current.from + dy)}
        onEnd={() => {}}
        onReset={() => setSplit(0.5)}
      />
      <FileList
        title={t.changes.staged}
        grow={1 - split}
        files={staged}
        selectedPath={selected?.staged ? selected.path : null}
        onSelect={(f) => onSelectFile({ path: f.path, staged: true, status: f.kind })}
        menuPath={menu?.staged ? menu.path : null}
        onContextMenu={(f, x, y) => setMenu({ x, y, path: f.path, kind: f.kind, staged: true })}
        actionLabel={t.changes.unstage}
        onAction={(p) => run(() => unstagePaths(path, p))}
        onActionAll={() =>
          run(() =>
            unstagePaths(
              path,
              staged.map((f) => f.path),
            ),
          )
        }
      />
      <div className="commitbox">
        <div className="commit-toolbar">
          <label
            className={"switch" + (!head || busy ? " disabled" : "")}
            title={
              head
                ? t.changes.amendHint(head.shortId)
                : t.changes.nothingToAmend
            }
          >
            <input
              type="checkbox"
              role="switch"
              checked={amend}
              disabled={!head || busy}
              onChange={(e) => toggleAmend(e.target.checked)}
            />
            <span className="switch-track" aria-hidden="true" />
            <span>{t.changes.amend}</span>
          </label>
        </div>
        {amend && head?.pushed && (
          <p className="warn">
            {t.changes.amendPushed}
          </p>
        )}
        <textarea
          placeholder={t.changes.messagePlaceholder}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && canCommit) commit();
          }}
          rows={4}
        />
        {error && <p className="error">{error}</p>}
        <div className="commit-actions">
          <button
            className="secondary"
            disabled={busy || changes.length === 0}
            onClick={() => setStashOpen(true)}
            title={t.changes.stashAllHint}
          >
            {t.changes.stashAll}
          </button>
          <button className="primary" disabled={!canCommit} onClick={commit}>
            {amend ? t.changes.amendCommit : t.changes.commit}
            {staged.length > 0 ? t.changes.count(staged.length) : ""}
          </button>
        </div>
      </div>
      {menu && <ContextMenu x={menu.x} y={menu.y} onClose={closeMenu} items={menuItems(menu)} />}
      {stashOpen && (
        <StashDialog
          path={path}
          fileCount={changes.length}
          onClose={() => setStashOpen(false)}
          onStashed={() => {
            setStashOpen(false);
            onCommitted(); // reloads everything; also closes an open working-tree diff
          }}
        />
      )}
    </aside>
  );
}
