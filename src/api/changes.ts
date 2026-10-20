import { invoke } from "@tauri-apps/api/core";

export type ChangeKind = "new" | "modified" | "deleted" | "typechange" | "conflicted";

export interface FileChange {
  path: string;
  staged: ChangeKind | null;
  unstaged: ChangeKind | null;
}

// Several panels ask for the status at once on every refresh, and scanning a big working tree is slow. Requests
// share scans: they start on the next tick (so a burst joins one), and one that arrives while a scan runs is
// answered by a single follow-up scan, never by the running one, which may have started before the change it asks about.
interface Pending {
  waiters: { resolve: (c: FileChange[]) => void; reject: (e: unknown) => void }[];
  running: boolean;
}
const pending = new Map<string, Pending>();

function scan(path: string, entry: Pending) {
  const batch = entry.waiters.splice(0);
  entry.running = true;
  invoke<FileChange[]>("get_status", { path })
    .then(
      (c) => batch.forEach((w) => w.resolve(c)),
      (e) => batch.forEach((w) => w.reject(e)),
    )
    .finally(() => {
      entry.running = false;
      if (entry.waiters.length > 0) scan(path, entry);
      else pending.delete(path);
    });
}

export const getStatus = (path: string) =>
  new Promise<FileChange[]>((resolve, reject) => {
    let entry = pending.get(path);
    if (!entry) pending.set(path, (entry = { waiters: [], running: false }));
    const first = entry.waiters.length === 0 && !entry.running;
    entry.waiters.push({ resolve, reject });
    if (first) setTimeout(() => scan(path, entry));
  });
export const stagePaths = (path: string, paths: string[]) =>
  invoke<void>("stage_paths", { path, paths });
export const unstagePaths = (path: string, paths: string[]) =>
  invoke<void>("unstage_paths", { path, paths });
/**
 * Throws away the unstaged changes of whole files: modified and deleted files go back to the staged
 * version, untracked files are deleted. Not undoable.
 */
export const discardPaths = (path: string, paths: string[]) => invoke<void>("discard_paths", { path, paths });
/** Commits the index; with `amend`, replaces the last commit instead. */
export const createCommit = (path: string, message: string, amend = false) =>
  invoke<string>("create_commit", { path, message, amend });

export interface HeadCommit {
  shortId: string;
  message: string;
  /** Already on the branch's upstream: amending it means rewriting published history. */
  pushed: boolean;
}

/** The last commit, or null on a branch without commits. */
export const getHeadCommit = (path: string) => invoke<HeadCommit | null>("get_head_commit", { path });
