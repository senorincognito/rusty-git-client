import { useCallback, useEffect, useState } from "react";
import {
  checkoutLocalBranch,
  countUnmergedCommits,
  deleteLocalBranch,
  deleteSyncedBranches,
  renameLocalBranch,
  getLocalBranches,
  type BranchInfo,
} from "@/api/branches";
import { confirmDialog } from "@/api/dialog";
import ContextMenu from "@/components/ContextMenu";
import Section from "@/components/Section";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import NewBranchForm from "@/features/toolbar/NewBranchForm";
import { t } from "@/i18n";
import BranchNameInput from "./BranchNameInput";
import { buildRows, FolderRow, leafIndent, useClosedFolders } from "./branchTree";
import { matchesFilter } from "./filter";

export default function LocalBranches({
  path,
  refreshKey,
  onChanged,
  filter,
  onFastForward,
  onMerge,
}: {
  path: string;
  refreshKey: number;
  onChanged: () => void;
  /** Only branches whose name matches are listed. */
  filter: string;
  /** Fast-forward the checked-out branch to this ref name. */
  onFastForward: (target: string) => void;
  /** Merge this branch (full ref name, display name) into the checked-out branch. */
  onMerge: (target: string, source: string) => void;
}) {
  const [branches, setBranches] = useState<BranchInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const start = useLatestRequest();

  const refresh = useCallback(async () => {
    const isCurrent = start();
    try {
      const b = await getLocalBranches(path);
      if (!isCurrent()) return;
      setBranches(b);
      setError(null);
    } catch (e) {
      if (isCurrent()) setError(String(e));
    }
  }, [path, start]);

  const [switching, setSwitching] = useState(false);
  const switchTo = async (b: BranchInfo) => {
    if (b.isHead || switching) return;
    setSwitching(true);
    try {
      await checkoutLocalBranch(path, b.name);
      setError(null);
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setSwitching(false);
    }
  };

  const shown = branches?.filter((b) => matchesFilter(filter, b.name));
  const currentBranch = branches?.find((b) => b.isHead)?.name;
  const folders = useClosedFolders();
  const rows = shown && buildRows(shown, (b) => b.name, folders.closed, filter.trim() !== "");
  const [menu, setMenu] = useState<{ x: number; y: number; branch: BranchInfo } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  // The "⋯" menu in the section's headline, and the inline new-branch form it opens.
  const [sectionMenu, setSectionMenu] = useState<{ x: number; y: number } | null>(null);
  const closeSectionMenu = useCallback(() => setSectionMenu(null), []);
  const [creating, setCreating] = useState(false);
  // The branch whose name is being edited inline.
  const [editing, setEditing] = useState<string | null>(null);

  const renameBranch = async (b: BranchInfo, newName: string) => {
    try {
      await renameLocalBranch(path, b.name, newName);
      setError(null);
      setEditing(null);
      onChanged();
    } catch (e) {
      setError(String(e)); // the editor stays open so the name can be corrected
    }
  };

  // Branches that point at the same commit as their upstream (as last fetched), except the checked-out one.
  const synced = (branches ?? []).filter((b) => !b.isHead && b.upstream !== null && b.ahead === 0 && b.behind === 0);

  const deleteSynced = async () => {
    const names = synced.map((b) => b.name);
    try {
      const ok = await confirmDialog(
        t.localBranches.deleteSyncedConfirm(names),
        t.localBranches.deleteSyncedTitle,
        true,
        t.common.delete,
      );
      if (!ok) return;
      await deleteSyncedBranches(path, names);
      setError(null);
      onChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  const deleteBranch = async (b: BranchInfo) => {
    try {
      const unmerged = await countUnmergedCommits(path, b.name);
      const message =
        unmerged > 0 ? t.localBranches.deleteUnmerged(b.name, unmerged) : t.localBranches.deleteConfirm(b.name);
      if (!(await confirmDialog(message, t.localBranches.deleteTitle, unmerged > 0))) return;
      await deleteLocalBranch(path, b.name);
      setError(null);
      onChanged();
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    refresh();
  }, [refresh, refreshKey]);

  return (
    <Section
      title={t.localBranches.title}
      resizeKey="local"
      count={shown?.length}
      action={{
        label: t.localBranches.actions,
        active: sectionMenu !== null,
        onClick: (r) => (sectionMenu ? closeSectionMenu() : setSectionMenu({ x: r.left, y: r.bottom + 4 })),
      }}
    >
      {creating && (
        <NewBranchForm
          path={path}
          className="addremote"
          onCreated={() => {
            setCreating(false);
            onChanged();
          }}
          onCancel={() => setCreating(false)}
        />
      )}
      {error && <p className="error side-msg">{error}</p>}
      {branches?.length === 0 && <p className="muted side-msg">{t.localBranches.none}</p>}
      {branches && branches.length > 0 && shown?.length === 0 && <p className="muted side-msg">{t.localBranches.noMatch}</p>}
      <ul className="branchlist">
        {rows?.map((row) => {
          if (row.kind === "folder") return <FolderRow key={"f:" + row.key} row={row} onToggle={folders.toggle} />;
          const b = row.item;
          return (
          <li
            key={b.name}
            style={leafIndent(row.depth)}
            className={(b.isHead ? "current" : "") + (menu?.branch.name === b.name ? " ctx" : "")}
            title={b.isHead ? t.localBranches.current(b.name) : t.localBranches.checkoutHint(b.name)}
            onDoubleClick={() => editing === null && switchTo(b)}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, branch: b });
            }}
          >
            {editing === b.name ? (
              <BranchNameInput
                initial={b.name}
                onSubmit={(name) => renameBranch(b, name)}
                onCancel={() => setEditing(null)}
              />
            ) : (
              <span className="bname">{row.label}</span>
            )}
            {b.behind > 0 && <span className="sync">↓{b.behind}</span>}
            {b.ahead > 0 && <span className="sync">↑{b.ahead}</span>}
          </li>
          );
        })}
      </ul>
      {sectionMenu && (
        <ContextMenu
          x={sectionMenu.x}
          y={sectionMenu.y}
          onClose={closeSectionMenu}
          items={[
            {
              label: t.localBranches.newBranch,
              disabled: creating,
              title: t.localBranches.newBranchHint,
              onClick: () => {
                setError(null);
                setCreating(true);
              },
            },
            {
              label: t.localBranches.deleteSynced(synced.length),
              danger: true,
              separatorBefore: true,
              disabled: synced.length === 0,
              title: synced.length === 0 ? t.localBranches.deleteSyncedNone : t.localBranches.deleteSyncedHint,
              onClick: deleteSynced,
            },
          ]}
        />
      )}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={closeMenu}
          items={[
            {
              label: t.localBranches.fastForward(currentBranch ?? "…", menu.branch.name),
              disabled: menu.branch.isHead || currentBranch === undefined,
              title: menu.branch.isHead
                ? t.localBranches.fastForwardCurrent
                : currentBranch === undefined
                  ? t.localBranches.fastForwardDetached
                  : t.localBranches.fastForwardHint,
              onClick: () => onFastForward(`refs/heads/${menu.branch.name}`),
            },
            {
              label: t.localBranches.merge(menu.branch.name, currentBranch ?? "…"),
              disabled: menu.branch.isHead || currentBranch === undefined,
              title: menu.branch.isHead
                ? t.localBranches.mergeCurrent
                : currentBranch === undefined
                  ? t.localBranches.mergeDetached
                  : t.localBranches.mergeHint,
              onClick: () => onMerge(`refs/heads/${menu.branch.name}`, menu.branch.name),
            },
            {
              label: t.localBranches.rename,
              separatorBefore: true,
              disabled: menu.branch.isHead,
              title: menu.branch.isHead
                ? t.localBranches.renameCurrent
                : undefined,
              onClick: () => {
                setError(null);
                setEditing(menu.branch.name);
              },
            },
            {
              label: t.localBranches.delete,
              danger: true,
              disabled: menu.branch.isHead,
              title: menu.branch.isHead
                ? t.localBranches.deleteCurrent
                : undefined,
              onClick: () => deleteBranch(menu.branch),
            },
          ]}
        />
      )}
    </Section>
  );
}
