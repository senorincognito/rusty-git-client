import { invoke } from "@tauri-apps/api/core";

export interface StashEntry {
  /** Position in the list: 0 is the newest ("stash@{0}"). */
  index: number;
  id: string;
  shortId: string;
  /** "WIP on main: abc1234 subject", or "On main: <your message>". */
  message: string;
  /** Unix seconds. */
  time: number;
}

/** All stashes, newest first. */
export const getStashes = (path: string) => invoke<StashEntry[]>("get_stashes", { path });

/**
 * Moves every uncommitted change (staged, unstaged and untracked files) into a new stash and
 * cleans the working directory. Resolves to the stash commit's id.
 */
export const createStash = (path: string, message: string | null) =>
  invoke<string>("create_stash", { path, message });

/**
 * Applies a stash and removes it from the list (git stash pop), putting staged changes back as
 * staged. Needs a clean working directory. A pop that conflicts is undone and the stash is kept
 * (the rejection explains). The stash is addressed by commit id, not list position.
 */
export const popStash = (path: string, id: string) => invoke<void>("pop_stash_cmd", { path, id });

/** Deletes a stash without applying it (git stash drop). The changes in it are gone for good. */
export const dropStash = (path: string, id: string) => invoke<void>("drop_stash_cmd", { path, id });

/**
 * Stashes only these files (staged and unstaged edits, and untracked files among them); everything else
 * stays in the working directory.
 */
export const stashPaths = (path: string, paths: string[]) => invoke<void>("stash_paths_cmd", { path, paths });

/**
 * Applies a stash and keeps it in the list (git stash apply), putting staged changes back as staged. Needs a clean
 * working directory; an apply that conflicts is undone and the rejection explains.
 */
export const applyStash = (path: string, id: string) => invoke<void>("apply_stash_cmd", { path, id });

/** Deletes every stash without applying any (git stash clear). Resolves to how many there were. */
export const dropAllStashes = (path: string) => invoke<number>("drop_all_stashes_cmd", { path });
