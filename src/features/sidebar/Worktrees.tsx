import { useCallback, useEffect, useState } from "react";
import { confirmDialog } from "@/api/dialog";
import { openRepo, type RepoInfo } from "@/api/repo";
import { addWorktree, getWorktrees, pruneWorktrees, removeWorktree, type WorktreeInfo } from "@/api/worktrees";
import ContextMenu from "@/components/ContextMenu";
import Section from "@/components/Section";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { t } from "@/i18n";
import { matchesFilter } from "./filter";

// What git says when it refuses to remove a worktree that has work in it.
const isDirty = (message: string) => /modified or untracked files|contains modified/i.test(message);

/**
 * The working directories of the repository (`git worktree`): the main one and the linked ones. Clicking one opens
 * it in its own tab, where everything else in the app works on that folder and its branch.
 */
export default function Worktrees({
  path,
  refreshKey,
  filter,
  onChanged,
  onOpen,
  onRemoved,
}: {
  path: string;
  refreshKey: number;
  /** Only worktrees whose branch or folder name matches are listed. */
  filter: string;
  onChanged: () => void;
  /** Open a worktree as a tab. */
  onOpen: (repo: RepoInfo) => void;
  /** A worktree was removed (its tab, if open, has nothing left to show). */
  onRemoved: (path: string) => void;
}) {
  const [list, setList] = useState<WorktreeInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; worktree: WorktreeInfo } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [sectionMenu, setSectionMenu] = useState<{ x: number; y: number } | null>(null);
  const closeSectionMenu = useCallback(() => setSectionMenu(null), []);
  const [adding, setAdding] = useState(false);
  const [branch, setBranch] = useState("");
  const start = useLatestRequest();

  const shown = list?.filter((w) => matchesFilter(filter, w.branch ?? "", w.name));
  const others = list?.filter((w) => !w.isMain).length ?? 0;
  const anyMissing = list?.some((w) => w.missing) ?? false;

  const refresh = useCallback(async () => {
    const isCurrent = start();
    try {
      const next = await getWorktrees(path);
      if (!isCurrent()) return;
      setList(next);
      setError(null);
    } catch (e) {
      if (isCurrent()) setError(String(e));
    }
  }, [path, start]);

  useEffect(() => {
    refresh();
  }, [refresh, refreshKey]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const open = (w: WorktreeInfo) => {
    if (w.isCurrent || w.missing) return;
    run(async () => onOpen(await openRepo(w.path)));
  };

  const submitAdd = () =>
    run(async () => {
      await addWorktree(path, branch);
      setAdding(false);
      setBranch("");
      onChanged();
    });

  const remove = async (w: WorktreeInfo) => {
    if (!(await confirmDialog(t.worktrees.removeConfirm(w.path), t.worktrees.remove, true, t.common.delete))) return;
    await run(async () => {
      try {
        await removeWorktree(path, w.path, false);
      } catch (e) {
        // Work in the folder: say so and let the user decide, instead of leaving them with git's bare message.
        if (!isDirty(String(e))) throw e;
        if (!(await confirmDialog(t.worktrees.removeForceConfirm(w.path, String(e)), t.worktrees.remove, true, t.worktrees.removeForce))) return;
        await removeWorktree(path, w.path, true);
      }
      onRemoved(w.path);
      onChanged();
    });
  };

  return (
    <Section
      title={t.worktrees.title}
      resizeKey="worktrees"
      count={shown?.length}
      action={{
        label: t.worktrees.actions,
        active: sectionMenu !== null,
        onClick: (r) => (sectionMenu ? closeSectionMenu() : setSectionMenu({ x: r.left, y: r.bottom + 4 })),
      }}
    >
      {error && <p className="error side-msg">{error}</p>}
      {adding && (
        <form
          className="addremote"
          onSubmit={(e) => {
            e.preventDefault();
            submitAdd();
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault();
              setAdding(false);
            }
          }}
        >
          <p className="muted">{t.worktrees.addInfo}</p>
          <input
            value={branch}
            onChange={(e) => setBranch(e.target.value)}
            placeholder={t.worktrees.addPlaceholder}
            aria-label={t.worktrees.addLabel}
            spellCheck={false}
            autoComplete="off"
            autoFocus
          />
          <div className="addremote-actions">
            <button className="primary" type="submit" disabled={busy || branch.trim() === ""}>
              {t.worktrees.addSubmit}
            </button>
            <button type="button" onClick={() => setAdding(false)} disabled={busy}>
              {t.common.cancel}
            </button>
          </div>
        </form>
      )}
      {others === 0 && !adding && <p className="muted side-msg">{t.worktrees.none}</p>}
      {others > 0 && shown?.length === 0 && <p className="muted side-msg">{t.worktrees.noMatch}</p>}
      <ul className="branchlist worktreelist">
        {shown?.map((w) => (
          <li
            key={w.path}
            className={(w.isCurrent ? "current" : "") + (w.missing ? " missing" : "") + (menu?.worktree.path === w.path ? " ctx" : "")}
            title={w.missing ? t.worktrees.missingHint(w.path) : w.isCurrent ? t.worktrees.currentHint(w.path) : t.worktrees.rowHint(w.path)}
            role="button"
            tabIndex={0}
            onClick={() => open(w)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                open(w);
              }
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenu({ x: e.clientX, y: e.clientY, worktree: w });
            }}
          >
            <span className="bname">{w.branch ?? t.worktrees.detached(w.head)}</span>
            <span className="wt-folder">{w.name}</span>
            {w.isMain && <span className="wt-badge">{t.worktrees.mainBadge}</span>}
            {w.locked && <span className="wt-badge">{t.worktrees.lockedBadge}</span>}
            {w.missing && <span className="wt-badge">{t.worktrees.missingBadge}</span>}
          </li>
        ))}
      </ul>
      {sectionMenu && (
        <ContextMenu
          x={sectionMenu.x}
          y={sectionMenu.y}
          onClose={closeSectionMenu}
          items={[
            { label: t.worktrees.add, title: t.worktrees.addHint, disabled: busy, onClick: () => setAdding(true) },
            {
              label: t.worktrees.prune,
              disabled: busy || !anyMissing,
              title: anyMissing ? t.worktrees.pruneHint : t.worktrees.pruneNone,
              onClick: () =>
                run(async () => {
                  await pruneWorktrees(path);
                  onChanged();
                }),
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
              label: t.worktrees.open,
              disabled: busy || menu.worktree.isCurrent || menu.worktree.missing,
              title: menu.worktree.missing ? t.worktrees.missingHint(menu.worktree.path) : menu.worktree.isCurrent ? t.worktrees.currentHint(menu.worktree.path) : undefined,
              onClick: () => open(menu.worktree),
            },
            {
              label: t.worktrees.remove,
              danger: true,
              separatorBefore: true,
              disabled: busy || menu.worktree.isMain || menu.worktree.isCurrent || menu.worktree.missing,
              title: menu.worktree.isMain
                ? t.worktrees.removeMain
                : menu.worktree.isCurrent
                  ? t.worktrees.removeCurrent
                  : menu.worktree.missing
                    ? t.worktrees.pruneHint
                    : t.worktrees.removeHint,
              onClick: () => remove(menu.worktree),
            },
          ]}
        />
      )}
    </Section>
  );
}
