import { invoke } from "./invoke";

export interface WorktreeInfo {
  /** Same form as `RepoInfo.path`: `openRepo` takes it as it is. */
  path: string;
  /** The folder's name. */
  name: string;
  /** Checked-out branch; null when HEAD is detached. */
  branch: string | null;
  /** Short id of the commit HEAD points at. */
  head: string;
  detached: boolean;
  /** The repository's own working directory: it can't be removed. */
  isMain: boolean;
  /** The worktree the open tab shows. */
  isCurrent: boolean;
  locked: boolean;
  /** The folder was deleted by hand: pruning forgets it. */
  missing: boolean;
}

/** Every working directory of the repository, the main one first, the others sorted by name. */
export const getWorktrees = (path: string) => invoke<WorktreeInfo[]>("get_worktrees", { path });

/**
 * How many files have uncommitted changes in each worktree, by path (a folder that is gone has no entry). Slower
 * than `getWorktrees`, so it is asked for separately.
 */
export const countWorktreeChanges = (path: string) => invoke<Record<string, number>>("count_worktree_changes", { path });

/**
 * Adds a worktree in `<repo>.worktrees/<branch>` next to the repository, with `branch` checked out: an existing
 * branch as it is (git refuses one that another worktree has checked out), otherwise a new one from the current
 * commit. Resolves to the new worktree's path.
 */
export const addWorktree = (path: string, branch: string) => invoke<string>("add_worktree_cmd", { path, branch });

/**
 * Deletes a linked worktree's folder; its branch stays. Without `force` a worktree with modified or untracked
 * files is refused (the rejection says so).
 */
export const removeWorktree = (path: string, worktree: string, force: boolean) =>
  invoke<void>("remove_worktree_cmd", { path, worktree, force });

/** Forgets worktrees whose folder is gone. */
export const pruneWorktrees = (path: string) => invoke<void>("prune_worktrees_cmd", { path });
