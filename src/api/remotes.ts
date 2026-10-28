import { invoke } from "./invoke";

export interface RemoteInfo {
  name: string;
  url: string;
  branches: string[];
  /** Remote branch tracked by the checked-out local branch, if any. */
  trackedByHead: string | null;
  /** New branches are published to this remote (the one chosen, else origin, else the first). */
  isTarget: boolean;
  /** Local branches that have an upstream on this remote. */
  trackingBranches: number;
}

/** All remotes of the repository, sorted by name (empty when there are none). */
export const getRemotes = (path: string) => invoke<RemoteInfo[]>("get_remotes", { path });
export const addRemote = (path: string, name: string, url: string) =>
  invoke<void>("add_remote_cmd", { path, name, url });
/** Points a remote at a different URL (remote-tracking branches are kept). */
export const setRemoteUrl = (path: string, name: string, url: string) =>
  invoke<void>("set_remote_url_cmd", { path, name, url });
/**
 * Removes a remote from the configuration with its remote-tracking branches. Local branches that
 * tracked it lose their upstream. Nothing on the server is touched.
 */
export const deleteRemote = (path: string, name: string) => invoke<void>("delete_remote_cmd", { path, name });
/** Chooses the remote new branches are published to (Push on a branch without upstream). */
export const setTargetRemote = (path: string, name: string) => invoke<void>("set_target_remote", { path, name });

/** Commits that exist only on <remote>/<name> (not in HEAD or any local branch). */
export const countUnmergedRemoteCommits = (path: string, remote: string, name: string) =>
  invoke<number>("count_unmerged_remote_commits", { path, remote, name });
/** Deletes <remote>/<name> on the server. */
export const deleteRemoteBranch = (path: string, remote: string, name: string) =>
  invoke<string>("delete_remote_branch", { path, remote, name });

/** Renames <remote>/<name> on the server (push the new name, delete the old one). */
export const renameRemoteBranch = (path: string, remote: string, name: string, newName: string) =>
  invoke<string>("rename_remote_branch", { path, remote, name, newName });
