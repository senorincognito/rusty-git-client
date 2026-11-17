import { useCallback, useEffect, useState } from "react";
import { deleteLocalBranch } from "@/api/branches";
import { confirmDialog, showInfo } from "@/api/dialog";
import { mergeBranch } from "@/api/history";
import { openRepo, type RepoInfo } from "@/api/repo";
import { checkAgentCommands, termRun, termStop } from "@/api/terminal";
import { addWorktree, countWorktreeChanges, getWorktrees, pruneWorktrees, removeWorktree, type WorktreeInfo } from "@/api/worktrees";
import AgentDot from "@/components/AgentDot";
import ContextMenu, { type MenuItem } from "@/components/ContextMenu";
import Section from "@/components/Section";
import { useLatestRequest } from "@/hooks/useLatestRequest";
import { useAgentPresets, usablePresets, type AgentPreset } from "@/hooks/useSetting";
import { refreshTerminalSessions, useTerminalSessions } from "@/hooks/useTerminalSessions";
import { t } from "@/i18n";
import { matchesFilter } from "./filter";

// What git says when it refuses to remove a worktree that has work in it.
const isDirty = (message: string) => /modified or untracked files|contains modified/i.test(message);

// While an agent runs, the change counts are looked up this often (other worktrees' indexes aren't watched).
const COUNT_POLL_MS = 10_000;

/**
 * The working directories of the repository (`git worktree`): the main one and the linked ones. Clicking one opens
 * it in its own tab, where everything else in the app works on that folder and its branch. Each can run an agent in
 * its own terminal session (shown as a dot), and its branch can be merged back into the branch of this tab.
 */
export default function Worktrees({
  path,
  refreshKey,
  filter,
  onChanged,
  onOpen,
  onRemoved,
  onMerge,
  onRebaseOnto,
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
  /** Merge the branch (full ref name, display name) into the checked-out branch of this tab. */
  onMerge: (target: string, source: string) => void;
  /** Rebase the checked-out branch of this tab onto this branch (full ref name, display name). */
  onRebaseOnto: (target: string, source: string) => void;
}) {
  const [list, setList] = useState<WorktreeInfo[] | null>(null);
  // Uncommitted files per worktree path. Scanning every working tree is slow, so it arrives after the list.
  const [changes, setChanges] = useState<Record<string, number>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; worktree: WorktreeInfo } | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const [sectionMenu, setSectionMenu] = useState<{ x: number; y: number } | null>(null);
  const closeSectionMenu = useCallback(() => setSectionMenu(null), []);
  const [adding, setAdding] = useState(false);
  const [branch, setBranch] = useState("");
  const [withAgent, setWithAgent] = useState(false);
  const [agentIndex, setAgentIndex] = useState(0);
  const start = useLatestRequest();
  const startCounts = useLatestRequest();

  const [allPresets] = useAgentPresets();
  const presets = usablePresets(allPresets);
  // Which agent commands have their program on PATH. Only a warning: the shell may know it as an alias.
  const [found, setFound] = useState<Record<string, boolean>>({});
  const commands = presets.map((p) => p.command).join("\n");
  useEffect(() => {
    if (commands === "") return;
    checkAgentCommands(commands.split("\n")).then(setFound).catch(() => {});
  }, [commands]);

  const sessions = useTerminalSessions();
  const shown = list?.filter((w) => matchesFilter(filter, w.branch ?? "", w.name));
  const others = list?.filter((w) => !w.isMain).length ?? 0;
  const anyMissing = list?.some((w) => w.missing) ?? false;
  const currentBranch = list?.find((w) => w.isCurrent)?.branch ?? null;
  const agentRunning = list?.some((w) => sessions.get(w.path)?.agent && sessions.get(w.path)?.running) ?? false;

  const refreshCounts = useCallback(async () => {
    const isCurrent = startCounts();
    try {
      const counts = await countWorktreeChanges(path);
      if (isCurrent()) setChanges(counts);
    } catch {
      /* the counts are an extra: the list is still right */
    }
  }, [path, startCounts]);

  const refresh = useCallback(async () => {
    const isCurrent = start();
    try {
      const next = await getWorktrees(path);
      if (!isCurrent()) return;
      setList(next);
      setError(null);
      if (next.length > 1) void refreshCounts();
    } catch (e) {
      if (isCurrent()) setError(String(e));
    }
  }, [path, start, refreshCounts]);

  useEffect(() => {
    refresh();
  }, [refresh, refreshKey]);

  // Edits inside a worktree's folder aren't watched (nor are other worktrees' indexes), so look again on focus.
  useEffect(() => {
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [refresh]);

  useEffect(() => {
    if (!agentRunning) return;
    const id = setInterval(refreshCounts, COUNT_POLL_MS);
    return () => clearInterval(id);
  }, [agentRunning, refreshCounts]);

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

  // The agent runs in the worktree's terminal session in the background: its dot shows how it is doing, and opening the
  // worktree's tab shows the terminal.
  const startAgent = async (worktree: string, preset: AgentPreset) => {
    await termRun(worktree, preset.command.trim(), preset.name.trim());
    await refreshTerminalSessions();
  };

  const submitAdd = () =>
    run(async () => {
      const created = await addWorktree(path, branch);
      setAdding(false);
      setBranch("");
      onChanged();
      const preset = presets[agentIndex] ?? presets[0];
      if (withAgent && preset) await startAgent(created, preset);
    });

  const stopSession = (w: WorktreeInfo) =>
    run(async () => {
      await termStop(w.path);
      await refreshTerminalSessions();
    });

  const remove = async (w: WorktreeInfo) => {
    const running = sessions.get(w.path)?.running ?? false;
    if (!(await confirmDialog(t.worktrees.removeConfirm(w.path, running), t.worktrees.remove, true, t.common.delete))) return;
    await run(async () => {
      // A shell running in the folder would keep it from being deleted (Windows).
      await termStop(w.path);
      try {
        await removeWorktree(path, w.path, false);
      } catch (e) {
        // Work in the folder: say so and let the user decide, instead of leaving them with git's bare message.
        if (!isDirty(String(e))) throw e;
        if (!(await confirmDialog(t.worktrees.removeForceConfirm(w.path, String(e)), t.worktrees.remove, true, t.worktrees.removeForce))) return;
        await removeWorktree(path, w.path, true);
      }
      await refreshTerminalSessions();
      onRemoved(w.path);
      onChanged();
    });
  };

  // Merge the worktree's branch into the branch of this tab, then clear it away: its folder, its terminal and the branch.
  const finish = async (w: WorktreeInfo) => {
    if (!w.branch || !currentBranch) return;
    const running = sessions.get(w.path)?.running ?? false;
    if (!(await confirmDialog(t.worktrees.finishConfirm(w.branch, currentBranch, w.path, running), t.worktrees.finishTitle, false, t.worktrees.finishOk))) return;
    await run(async () => {
      const outcome = await mergeBranch(path, `refs/heads/${w.branch}`);
      onChanged();
      if (outcome === "conflicts") {
        await showInfo(t.worktrees.finishConflicts(w.branch!), t.worktrees.finishTitle);
        return;
      }
      // The merge is done: from here a failure says so, instead of looking like nothing happened.
      try {
        await termStop(w.path);
        await removeWorktree(path, w.path, false);
        await deleteLocalBranch(path, w.branch!);
        onRemoved(w.path);
      } catch (e) {
        setError(t.worktrees.finishPartial(String(e)));
      }
      await refreshTerminalSessions();
      onChanged();
    });
  };

  // Merging or rebasing takes commits only: say so when the agent left work uncommitted, before it looks like an empty merge.
  const withCommitsOnly = async (w: WorktreeInfo, action: () => void) => {
    const uncommitted = changes[w.path] ?? 0;
    if (uncommitted > 0 && !(await confirmDialog(t.worktrees.uncommittedConfirm(w.branch ?? "", uncommitted), t.worktrees.uncommittedTitle, true, t.worktrees.uncommittedOk))) return;
    action();
  };

  // Why a worktree can't be merged (or rebased onto, or finished): this tab's own, gone, detached, or no branch here.
  const blocked = (w: WorktreeInfo) =>
    w.missing
      ? t.worktrees.missingHint(w.path)
      : w.isCurrent
        ? t.worktrees.currentHint(w.path)
        : !w.branch
          ? t.worktrees.noBranch
          : !currentBranch
            ? t.worktrees.tabDetached
            : undefined;

  const agentItems = (w: WorktreeInfo): MenuItem[] => {
    const session = sessions.get(w.path);
    return presets.map((p) => ({
      label: p.name,
      title: found[p.command] === false ? t.worktrees.agentNotFound(p.command) : t.worktrees.startAgentHint(p.command),
      disabled: busy || !!session?.agent,
      onClick: () => run(() => startAgent(w.path, p)),
    }));
  };

  const worktreeMenu = (w: WorktreeInfo): MenuItem[] => {
    const session = sessions.get(w.path);
    const reason = blocked(w);
    const uncommitted = changes[w.path] ?? 0;
    return [
      {
        label: t.worktrees.open,
        disabled: busy || w.isCurrent || w.missing,
        title: w.missing ? t.worktrees.missingHint(w.path) : w.isCurrent ? t.worktrees.currentHint(w.path) : undefined,
        onClick: () => open(w),
      },
      {
        label: t.worktrees.startAgent,
        disabled: busy || w.missing || presets.length === 0 || !!session?.agent,
        title: w.missing
          ? t.worktrees.missingHint(w.path)
          : presets.length === 0
            ? t.worktrees.startAgentNone
            : session?.agent
              ? t.worktrees.agentRunning(session.agent)
              : undefined,
        children: agentItems(w),
      },
      ...(session
        ? [{ label: t.worktrees.stopSession, title: t.worktrees.stopSessionHint, disabled: busy, onClick: () => stopSession(w) }]
        : []),
      {
        label: t.worktrees.merge(w.branch ?? "…", currentBranch ?? "…"),
        separatorBefore: true,
        disabled: busy || !!reason,
        title: reason ?? (uncommitted > 0 ? `${t.worktrees.mergeHint}\n${t.worktrees.uncommitted(uncommitted)}` : t.worktrees.mergeHint),
        onClick: () => withCommitsOnly(w, () => onMerge(`refs/heads/${w.branch}`, w.branch!)),
      },
      {
        label: t.worktrees.rebase(currentBranch ?? "…", w.branch ?? "…"),
        disabled: busy || !!reason,
        title: reason ?? (uncommitted > 0 ? `${t.worktrees.rebaseHint}\n${t.worktrees.uncommitted(uncommitted)}` : t.worktrees.rebaseHint),
        onClick: () => withCommitsOnly(w, () => onRebaseOnto(`refs/heads/${w.branch}`, w.branch!)),
      },
      {
        label: t.worktrees.finish,
        disabled: busy || !!reason || w.isMain || uncommitted > 0,
        title: reason ?? (w.isMain ? t.worktrees.removeMain : uncommitted > 0 ? t.worktrees.finishDirty : t.worktrees.finishHint),
        onClick: () => finish(w),
      },
      {
        label: t.worktrees.remove,
        danger: true,
        separatorBefore: true,
        disabled: busy || w.isMain || w.isCurrent || w.missing,
        title: w.isMain
          ? t.worktrees.removeMain
          : w.isCurrent
            ? t.worktrees.removeCurrent
            : w.missing
              ? t.worktrees.pruneHint
              : t.worktrees.removeHint,
        onClick: () => remove(w),
      },
    ];
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
          {presets.length > 0 && (
            <div className="wt-agentpick">
              <label>
                <input type="checkbox" checked={withAgent} onChange={(e) => setWithAgent(e.target.checked)} />
                {t.worktrees.agentToggle}
              </label>
              {withAgent && presets.length > 1 && (
                <select value={agentIndex} onChange={(e) => setAgentIndex(Number(e.target.value))} aria-label={t.worktrees.agentChoose}>
                  {presets.map((p, i) => (
                    <option key={i} value={i}>
                      {p.name}
                    </option>
                  ))}
                </select>
              )}
            </div>
          )}
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
        {shown?.map((w) => {
          const session = sessions.get(w.path);
          return (
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
              {session?.agent && <AgentDot agent={session.agent} status={session.status} />}
              {!w.missing && (changes[w.path] ?? 0) > 0 && (
                <span className="wt-changes" title={t.worktrees.changes(changes[w.path])}>
                  {changes[w.path]}
                </span>
              )}
              {w.isMain && <span className="wt-badge">{t.worktrees.mainBadge}</span>}
              {w.locked && <span className="wt-badge">{t.worktrees.lockedBadge}</span>}
              {w.missing && <span className="wt-badge">{t.worktrees.missingBadge}</span>}
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
      {menu && <ContextMenu x={menu.x} y={menu.y} onClose={closeMenu} items={worktreeMenu(menu.worktree)} />}
    </Section>
  );
}
