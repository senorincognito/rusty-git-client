import { invoke } from "@tauri-apps/api/core";

export interface RenameInfo {
  /** Full current message, to pre-fill the editor. */
  message: string;
  /** Already on the upstream: renaming it means rewriting published history. */
  pushed: boolean;
  /** Later commits on the branch that get rebuilt along with it. */
  laterCommits: number;
}

export const getRenameInfo = (path: string, id: string) =>
  invoke<RenameInfo>("get_rename_info", { path, id });

/** Changes a commit's message (rewriting later commits as needed); resolves to its new id. */
export const renameCommitMessage = (path: string, id: string, message: string) =>
  invoke<string>("rename_commit_message", { path, id, message });

export interface RebaseCommit {
  id: string;
  shortId: string;
  /** Full message, to pre-fill the editor. */
  message: string;
  author: string;
  /** Unix seconds. */
  time: number;
  /** Already on the upstream: changing it rewrites published history. */
  pushed: boolean;
  isMerge: boolean;
}

export interface RebasePlan {
  /** HEAD when the plan was made; applying refuses if the branch moved since. */
  headId: string;
  /** The commits after the base on the current branch, newest first. */
  commits: RebaseCommit[];
}

/** The commits an interactive rebase onto `id` covers (every later commit on the current branch). */
export const getRebasePlan = (path: string, id: string) => invoke<RebasePlan>("get_rebase_plan", { path, id });

export type RebaseAction = "pick" | "reword" | "squash" | "drop";

export interface RebaseStep {
  id: string;
  action: RebaseAction;
  /** The new message of a "reword". */
  message?: string;
}

/**
 * Applies an interactive rebase. A "squash" commit is melded into the commit before it (the next older one)
 * with its message appended; a "reword" gets a new message; commits without a step are picked. Every commit
 * after the oldest change is rebuilt and the branch moves. A "drop" removes the commit and its changes: the later
 * commits are replayed (new ids, new content), which needs a clean working directory and is abandoned, with nothing
 * changed, if one of them depends on a dropped commit.
 *
 * `order` lists the ids of all the plan's commits, newest first, in the order they should end up in (the plan's own
 * order when nothing moved). A different order replays the commits the same way a drop does: it needs a clean
 * working directory and is abandoned if a commit depends on one that now comes after it.
 */
export const applyRebase = (path: string, baseId: string, headId: string, steps: RebaseStep[], order: string[]) =>
  invoke<void>("apply_rebase_cmd", { path, baseId, headId, steps, order });

export interface DropInfo {
  shortId: string;
  summary: string;
  /** Already on the upstream: dropping it rewrites published history (needs a force push). */
  pushed: boolean;
  /** A merge commit: the branch goes back to its first parent. */
  isMerge: boolean;
  /** Commits after it on the branch, which are re-created (new ids) on top of its parent. */
  laterCommits: number;
  /** How many of those are already on the upstream. */
  laterPushed: number;
}

/** What dropping a commit would involve; rejects if that commit can't be dropped. */
export const getDropInfo = (path: string, id: string) => invoke<DropInfo>("get_drop_info", { path, id });

/**
 * Removes a commit from the current branch, re-creating the commits after it on top of its parent.
 * Needs a clean working directory. If a later commit depends on the dropped one, nothing is changed
 * and the rejection says which.
 */
export const dropLatestCommit = (path: string, id: string) => invoke<void>("drop_latest_commit", { path, id });

export type ResetMode = "soft" | "mixed" | "hard";

/** What resetting to a commit would do to the branch, the staging area and the files. */
export interface ResetInfo {
  /** The checked-out branch; null on a detached HEAD. */
  branch: string | null;
  fromShort: string;
  targetShort: string;
  targetSummary: string;
  /** The target is the commit HEAD is already on. */
  sameCommit: boolean;
  /** The target is HEAD or one of its ancestors (the branch moves back). */
  isAncestor: boolean;
  /** Commits that would no longer be on the branch, and the newest few of their summaries. */
  removed: number;
  removedSummaries: string[];
  /** Commits the branch would gain. */
  added: number;
  /** How many removed commits are already on the upstream (a force push would be needed). */
  pushedRemoved: number;
  /** Files with uncommitted changes (a hard reset throws these away). */
  workingChanges: number;
}

export const getResetInfo = (path: string, id: string) => invoke<ResetInfo>("get_reset_info", { path, id });

/**
 * git reset --soft / --mixed / --hard to a commit. Soft moves only the branch, mixed also resets
 * the staging area, hard also resets the files (uncommitted changes are lost).
 */
export const resetToCommit = (path: string, id: string, mode: ResetMode) =>
  invoke<void>("reset_to_commit", { path, id, mode });

/** Moves the checked-out branch forward to a commit id or full ref name; resolves to the number of commits gained. */
export const fastForward = (path: string, target: string) => invoke<number>("fast_forward_cmd", { path, target });

export type MergeOutcome = "upToDate" | "fastForward" | "merged";

/** Merges a branch (full ref name) into the checked-out one. Conflicts cancel the merge and reject with the files named. */
export const mergeBranch = (path: string, target: string) => invoke<MergeOutcome>("merge_branch_cmd", { path, target });

export type RebaseOutcome = "upToDate" | "fastForward" | "rebased";

export interface RebaseOntoInfo {
  /** The checked-out branch. */
  branch: string;
  /** Its commits that the target lacks: these are replayed. */
  ahead: number;
  /** Commits of the target that the branch lacks. */
  behind: number;
  /** How many of the replayed commits are already on the branch's upstream (a force push is needed afterwards). */
  pushed: number;
  /** Merge commits among the replayed ones (a plain rebase flattens them). */
  merges: number;
}

/** What rebasing the checked-out branch onto a branch (full ref name) would replay. */
export const getRebaseOntoInfo = (path: string, target: string) =>
  invoke<RebaseOntoInfo>("get_rebase_onto_info", { path, target });

/** Rebases the checked-out branch onto a branch (full ref name). Conflicts cancel it and reject with the files named. */
export const rebaseOnto = (path: string, target: string) => invoke<RebaseOutcome>("rebase_onto_cmd", { path, target });
