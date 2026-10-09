import { invoke } from "@tauri-apps/api/core";

export interface RepoInfo {
  path: string;
  name: string;
  head: string | null;
  detached: boolean;
}

export const openRepo = (path: string) => invoke<RepoInfo>("open_repo", { path });
export const getRecentRepos = () => invoke<RepoInfo[]>("get_recent_repos");
export const removeRecentRepo = (path: string) =>
  invoke<void>("remove_recent_repo", { path });

/** A repository found inside the start screen's folder. */
export interface FolderRepo extends RepoInfo {
  /** The folders between the chosen one and the repository ("" for a direct child), with "/". */
  parent: string;
  /** Unix seconds of the commit HEAD points at; null for a repository without commits. */
  lastCommit: number | null;
}

/** The remembered folder of repositories, if there is one (and it still exists). */
export const getRepoFolder = () => invoke<string | null>("get_repo_folder");
/** Remembers the folder of repositories; null forgets it. */
export const setRepoFolder = (path: string | null) => invoke<void>("set_repo_folder", { path });
/** The repositories inside a folder (up to three levels down), sorted by name. */
export const scanRepoFolder = (path: string) => invoke<FolderRepo[]>("scan_repo_folder", { path });
