import { invoke } from "@tauri-apps/api/core";

export interface CommitFile {
  path: string;
  /** Previous path, for renamed and copied files. */
  oldPath: string | null;
  status: "new" | "modified" | "deleted" | "renamed" | "copied" | "typechange";
}

export interface CommitDetail {
  id: string;
  shortId: string;
  summary: string;
  /** Everything after the first line of the message; empty for one-liners. */
  body: string;
  author: string;
  email: string;
  /** Unix seconds. */
  time: number;
  /** Short ids of the parents. */
  parents: string[];
  /** Changes are relative to the first parent. */
  isMerge: boolean;
  /** "stash@{n}" when this commit is a stash. */
  stash: string | null;
  files: CommitFile[];
  totalFiles: number;
  /** The file list was cut off (very large commit). */
  truncated: boolean;
}

export const getCommitDetail = (path: string, id: string) =>
  invoke<CommitDetail>("get_commit_detail", { path, id });

/** One commit that changed a file. `path` is the file's name in that commit (it differs before a rename). */
export interface HistoryEntry {
  id: string;
  shortId: string;
  summary: string;
  author: string;
  /** Unix seconds. */
  time: number;
  status: CommitFile["status"];
  path: string;
  oldPath: string | null;
}

/**
 * The commits that changed a file, newest first, following it across renames (at most 500). With `from` (a commit id)
 * the list starts at that commit, where `file` is the file's name, and leaves out later changes.
 */
export const getFileHistory = (path: string, file: string, from: string | null = null) =>
  invoke<HistoryEntry[]>("get_file_history", { path, file, from });
