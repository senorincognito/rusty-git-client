import { invoke } from "@tauri-apps/api/core";

export interface BranchInfo {
  name: string;
  isHead: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
}

export const getLocalBranches = (path: string) =>
  invoke<BranchInfo[]>("get_local_branches", { path });

/** Creates a branch at the current commit and checks it out. */
export const createBranch = (path: string, name: string) =>
  invoke<void>("create_branch", { path, name });

/** Switches to an existing local branch; rejects if local changes would be overwritten. */
export const checkoutLocalBranch = (path: string, name: string) =>
  invoke<void>("checkout_local_branch", { path, name });

/** Checks out a remote branch as a local branch of the same name that tracks it (created if needed). */
export const checkoutRemoteBranch = (path: string, remote: string, name: string) =>
  invoke<void>("checkout_remote_branch", { path, remote, name });

/** Number of commits that would be left unreachable by deleting the branch (0 if merged). */
export const countUnmergedCommits = (path: string, name: string) =>
  invoke<number>("count_unmerged_commits", { path, name });
export const deleteLocalBranch = (path: string, name: string) =>
  invoke<void>("delete_local_branch", { path, name });

/** Renames a local branch that is not checked out. */
export const renameLocalBranch = (path: string, name: string, newName: string) =>
  invoke<void>("rename_local_branch", { path, name, newName });

/**
 * Deletes those of the given local branches that are still fully synced with their upstream (same commit); the
 * checked-out branch and anything ahead, behind or without an upstream is skipped. Resolves to the deleted names.
 */
export const deleteSyncedBranches = (path: string, names: string[]) =>
  invoke<string[]>("delete_synced_branches", { path, names });
