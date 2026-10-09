import { useCallback, useEffect, useState } from "react";
import { checkoutRemoteBranch, deleteMergedRemote, getMergedBranches } from "@/api/branches";
import { confirmDialog, showInfo } from "@/api/dialog";
import {
  countUnmergedRemoteCommits,
  deleteRemote,
  deleteRemoteBranch,
  getRemotes,
  renameRemoteBranch,
  setRemoteUrl,
  setTargetRemote,
  type RemoteInfo,
} from "@/api/remotes";
import ContextMenu from "@/components/ContextMenu";
import Section from "@/components/Section";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { t } from "@/i18n";
import AddRemote from "./AddRemote";
import BranchNameInput from "./BranchNameInput";
import { buildRows, FolderRow, leafIndent, useClosedFolders } from "./branchTree";
import { matchesFilter } from "./filter";

export default function Remotes({
  path,
  refreshKey,
  onChanged,
  fetchError,
  filter,
  onMerge,
}: {
  path: string;
  refreshKey: number;
  onChanged: () => void;
  /** Why the last fetch failed, if it did: shown as a warning beside each remote's name. */
  fetchError: string | null;
  /** Only branches whose name (or "remote/name") matches are listed; a remote whose own name matches shows all of its branches. */
  filter: string;
  /** Merge this remote branch (full ref name, display name) into the checked-out branch. */
  onMerge: (target: string, source: string) => void;
}) {
  const [remotes, setRemotes] = useState<RemoteInfo[] | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const start = useLatestRequest();
  // Right-click menus: on a remote branch, or on a remote's header.
  const [menu, setMenu] = useState<{ x: number; y: number; remote: string; branch: string } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [headMenu, setHeadMenu] = useState<{ x: number; y: number; remote: string } | null>(null);
  const closeHeadMenu = useCallback(() => setHeadMenu(null), []);
  // The "⋯" menu in the section's headline.
  const [sectionMenu, setSectionMenu] = useState<{ x: number; y: number } | null>(null);
  const closeSectionMenu = useCallback(() => setSectionMenu(null), []);
  // What a running server operation is doing, e.g. "Deleting origin/x…".
  const [busy, setBusy] = useState<string | null>(null);
  // Inline editors: a remote branch's name, or a remote's URL.
  const [editing, setEditing] = useState<{ remote: string; branch: string } | null>(null);
  const [editingUrl, setEditingUrl] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);

  const refresh = useCallback(async () => {
    const isCurrent = start();
    try {
      const list = await getRemotes(path);
      if (!isCurrent()) return;
      setRemotes(list);
      setError(null);
    } catch (e) {
      if (isCurrent()) setError(String(e));
    }
  }, [path, start]);

  useEffect(() => {
    setRemotes(undefined);
    setMenu(null);
    setHeadMenu(null);
    setSectionMenu(null);
    setEditing(null);
    setEditingUrl(null);
    setAdding(false);
  }, [path]);

  useEffect(() => {
    refresh();
  }, [refresh, refreshKey]);

  const deleteBranch = async (remote: string, name: string) => {
    const full = `${remote}/${name}`;
    try {
      const unique = await countUnmergedRemoteCommits(path, remote, name);
      const ok = await confirmDialog(t.remotes.deleteBranchConfirm(full, unique), t.remotes.deleteBranch, true);
      if (!ok) return;
      setBusy(t.remotes.deleting(full));
      setError(null);
      await deleteRemoteBranch(path, remote, name);
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const renameBranch = async (remote: string, name: string, newName: string) => {
    const from = `${remote}/${name}`;
    const to = `${remote}/${newName}`;
    try {
      const ok = await confirmDialog(
        t.remotes.renameBranchConfirm(from, to),
        t.remotes.renameBranch,
        true,
        t.remotes.renameOk,
      );
      if (!ok) {
        setEditing(null);
        return;
      }
      setBusy(t.remotes.renaming(from));
      setError(null);
      await renameRemoteBranch(path, remote, name, newName);
      setEditing(null);
      onChanged();
    } catch (e) {
      setError(String(e)); // the editor stays open so the name can be corrected
    } finally {
      setBusy(null);
    }
  };

  const changeUrl = async (remote: string, url: string) => {
    try {
      await setRemoteUrl(path, remote, url);
      setEditingUrl(null);
      setError(null);
      onChanged();
    } catch (e) {
      setError(String(e)); // the editor stays open so the URL can be corrected
    }
  };

  const makeTarget = async (remote: string) => {
    try {
      setError(null);
      await setTargetRemote(path, remote);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const removeRemote = async (r: RemoteInfo) => {
    const text = t.remotes.removeConfirm(r.name, r.url, r.branches.length, r.trackingBranches);
    try {
      if (!(await confirmDialog(text, t.remotes.removeTitle, true, t.remotes.removeOk))) return;
      setError(null);
      await deleteRemote(path, r.name);
      onChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  // "Clean up merged": remote branches already merged into their remote's main branch are deleted on the server.
  const cleanUpMerged = async () => {
    if (busy !== null) return;
    try {
      const found = await getMergedBranches(path, true);
      if (found.branches.length === 0) {
        await showInfo(t.remotes.cleanUpMergedNone(found.bases), t.remotes.cleanUpMergedTitle);
        return;
      }
      const labels = found.branches.map((b) => `${b.remote}/${b.name}`);
      const ok = await confirmDialog(
        t.remotes.cleanUpMergedConfirm(labels, found.bases),
        t.remotes.cleanUpMergedTitle,
        true,
        t.common.delete,
      );
      if (!ok) return;
      setError(null);
      setBusy(t.remotes.cleanUpMergedBusy(found.branches.length));
      try {
        await deleteMergedRemote(path, found.branches);
      } finally {
        setBusy(null);
        onChanged();
      }
    } catch (e) {
      setError(String(e));
    }
  };

  const checkOut = async (remote: string, branch: string) => {
    if (busy !== null) return;
    setBusy(t.remotes.checkingOut(`${remote}/${branch}`));
    try {
      await checkoutRemoteBranch(path, remote, branch);
      setError(null);
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  // With a filter, a remote shows only the branches that match (all of them when its own name matches);
  // remotes with nothing to show are hidden.
  const visible = remotes
    ?.map((r) => ({
      r,
      branches: matchesFilter(filter, r.name) ? r.branches : r.branches.filter((b) => matchesFilter(filter, `${r.name}/${b}`)),
    }))
    .filter(({ r, branches }) => filter.trim() === "" || branches.length > 0 || matchesFilter(filter, r.name));
  const folders = useClosedFolders();
  const several = (remotes?.length ?? 0) > 1;
  const headRemote = headMenu ? remotes?.find((r) => r.name === headMenu.remote) : undefined;
  const branchRemote = menu ? remotes?.find((r) => r.name === menu.remote) : undefined;

  return (
    <Section
      title={t.remotes.title}
      resizeKey="remotes"
      count={visible?.length}
      action={{
        label: t.remotes.actions,
        active: sectionMenu !== null,
        onClick: (r) => (sectionMenu ? closeSectionMenu() : setSectionMenu({ x: r.left, y: r.bottom + 4 })),
      }}
    >
      {error && <p className="error side-msg">{error}</p>}
      {busy && <p className="muted side-msg">{busy}</p>}
      {remotes?.length === 0 && <AddRemote path={path} first onAdded={onChanged} />}
      {adding && remotes && remotes.length > 0 && (
        <AddRemote
          path={path}
          first={false}
          onAdded={() => {
            setAdding(false);
            onChanged();
          }}
          onCancel={() => setAdding(false)}
        />
      )}
      {remotes && remotes.length > 0 && visible?.length === 0 && (
        <p className="muted side-msg">{t.remotes.noMatch}</p>
      )}
      {visible?.map(({ r, branches }) => (
        <div key={r.name} className="remote">
          <div
            className={"remote-head" + (headMenu?.remote === r.name ? " ctx" : "")}
            title={r.url}
            onContextMenu={(e) => {
              e.preventDefault();
              setHeadMenu({ x: e.clientX, y: e.clientY, remote: r.name });
            }}
          >
            <span className="rname">
              {r.name}
              {several && r.isTarget && (
                <span className="rtarget" title={t.remotes.targetHint}>
                  {t.remotes.target}
                </span>
              )}
              {fetchError && (
                <span className="rwarn" role="img" title={t.remotes.fetchFailedHint(fetchError)} aria-label={t.remotes.fetchFailed}>
                  ⚠
                </span>
              )}
            </span>
            {editingUrl === r.name ? (
              <BranchNameInput
                initial={r.url}
                label={t.remotes.urlLabel}
                onSubmit={(url) => changeUrl(r.name, url)}
                onCancel={() => setEditingUrl(null)}
              />
            ) : (
              <span className="rurl">{r.url}</span>
            )}
          </div>
          {branches.length === 0 && r.branches.length === 0 && <p className="muted side-msg">{t.remotes.noBranches}</p>}
          <ul className="branchlist">
            {buildRows(branches, (x) => x, folders.closed, filter.trim() !== "", r.name + ":").map((row) => {
              if (row.kind === "folder") return <FolderRow key={"f:" + row.key} row={row} onToggle={folders.toggle} />;
              const b = row.item;
              return (
              <li
                key={b}
                style={leafIndent(row.depth)}
                className={menu?.remote === r.name && menu.branch === b ? "ctx" : ""}
                title={t.remotes.checkoutHint(`${r.name}/${b}`)}
                onDoubleClick={() => editing === null && checkOut(r.name, b)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setMenu({ x: e.clientX, y: e.clientY, remote: r.name, branch: b });
                }}
              >
                {editing?.remote === r.name && editing.branch === b ? (
                  <BranchNameInput
                    initial={b}
                    onSubmit={(name) => renameBranch(r.name, b, name)}
                    onCancel={() => setEditing(null)}
                  />
                ) : (
                  <span className="bname">{row.label}</span>
                )}
              </li>
              );
            })}
          </ul>
        </div>
      ))}
      {sectionMenu && remotes && (
        <ContextMenu
          x={sectionMenu.x}
          y={sectionMenu.y}
          onClose={closeSectionMenu}
          items={[
            {
              label: t.remotes.addRemote,
              disabled: adding || remotes.length === 0,
              title: remotes.length === 0 ? t.remotes.addRemoteUseForm : t.remotes.addRemoteHint,
              onClick: () => {
                setError(null);
                setAdding(true);
              },
            },
            // With several remotes: where Push publishes new branches.
            ...(remotes.length > 1
              ? [
                  {
                    label: t.remotes.targetRemote,
                    separatorBefore: true,
                    title: t.remotes.targetRemoteHint,
                    children: remotes.map((r) => ({
                      label: r.name,
                      checked: r.isTarget,
                      title: r.url,
                      onClick: () => makeTarget(r.name),
                    })),
                  },
                ]
              : []),
            {
              label: t.remotes.cleanUpMerged,
              danger: true,
              separatorBefore: true,
              disabled: busy !== null || remotes.length === 0,
              title: t.remotes.cleanUpMergedHint,
              onClick: cleanUpMerged,
            },
          ]}
        />
      )}
      {headMenu && headRemote && (
        <ContextMenu
          x={headMenu.x}
          y={headMenu.y}
          onClose={closeHeadMenu}
          items={[
            {
              label: t.remotes.setTarget,
              disabled: headRemote.isTarget,
              title: headRemote.isTarget
                ? t.remotes.alreadyTarget
                : t.remotes.setTargetHint,
              onClick: () => makeTarget(headRemote.name),
            },
            {
              label: t.remotes.editUrl,
              onClick: () => {
                setError(null);
                setEditingUrl(headRemote.name);
              },
            },
            {
              label: t.remotes.remove,
              danger: true,
              separatorBefore: true,
              disabled: busy !== null,
              title: t.remotes.removeHint,
              onClick: () => removeRemote(headRemote),
            },
          ]}
        />
      )}
      {menu && branchRemote && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={[
            {
              label: t.remotes.checkout,
              disabled: busy !== null,
              onClick: () => checkOut(menu.remote, menu.branch),
            },
            {
              label: t.remotes.merge(`${menu.remote}/${menu.branch}`),
              title: t.remotes.mergeHint,
              onClick: () => onMerge(`refs/remotes/${menu.remote}/${menu.branch}`, `${menu.remote}/${menu.branch}`),
            },
            {
              label: t.remotes.renameBranch,
              separatorBefore: true,
              disabled: busy !== null || branchRemote.trackedByHead === menu.branch,
              title:
                branchRemote.trackedByHead === menu.branch
                  ? t.remotes.upstreamOfHead
                  : undefined,
              onClick: () => {
                setError(null);
                setEditing({ remote: menu.remote, branch: menu.branch });
              },
            },
            {
              label: t.remotes.deleteBranch,
              danger: true,
              disabled: busy !== null || branchRemote.trackedByHead === menu.branch,
              title:
                branchRemote.trackedByHead === menu.branch
                  ? t.remotes.upstreamOfHead
                  : undefined,
              onClick: () => deleteBranch(menu.remote, menu.branch),
            },
          ]}
        />
      )}
    </Section>
  );
}
