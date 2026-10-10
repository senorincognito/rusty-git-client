import { invoke } from "@tauri-apps/api/core";

export interface DiffLine {
  /** "hunk" is a hunk header (changes-only view); "note" is e.g. "No newline at end of file". */
  kind: "ctx" | "add" | "del" | "hunk" | "note";
  oldNo: number | null;
  newNo: number | null;
  text: string;
  /** The hunk (run of added/removed lines) this line belongs to; null for context and headers. */
  block: number | null;
}

export interface FileDiff {
  lines: DiffLine[];
  /** Binary or too large to show. */
  binary: boolean;
  truncated: boolean;
  additions: number;
  deletions: number;
  /** One fingerprint per hunk, used to detect that the file changed before stage/discard. */
  blocks: string[];
}

/**
 * One file of a commit as a line diff against the first parent. `fullFile` returns the whole
 * file with additions and deletions marked in place; otherwise only the changed hunks.
 */
export const getFileDiff = (
  path: string,
  id: string,
  file: string,
  oldPath: string | null,
  fullFile: boolean,
) => invoke<FileDiff>("get_file_diff", { path, id, file, oldPath, fullFile });

/**
 * A file's uncommitted changes. `staged` compares HEAD with the index (what the next commit
 * contains); otherwise the index with the file on disk. Untracked files show as all additions.
 */
export const getWorkingDiff = (path: string, file: string, staged: boolean, fullFile: boolean) =>
  invoke<FileDiff>("get_working_diff", { path, file, staged, fullFile });

/**
 * Puts one hunk of a file's unstaged changes into the staging area. `blockId` is the hunk's
 * fingerprint from the diff; if the file changed since the diff was shown the call is refused.
 */
export const stageHunk = (path: string, file: string, block: number, blockId: string) =>
  invoke<void>("stage_hunk_cmd", { path, file, block, blockId });

/** Throws away one hunk of a file's unstaged changes (Undo can bring it back). */
export const discardHunk = (path: string, file: string, block: number, blockId: string) =>
  invoke<void>("discard_hunk_cmd", { path, file, block, blockId });

/** Takes one hunk of a file's staged changes back out of the staging area. */
export const unstageHunk = (path: string, file: string, block: number, blockId: string) =>
  invoke<void>("unstage_hunk_cmd", { path, file, block, blockId });

/**
 * Puts a single changed line of a file's unstaged changes into the staging area: the `line`-th added or removed line
 * of hunk `block`, counted from 0 within the hunk. `blockId` is the hunk's fingerprint, as for a whole hunk.
 */
export const stageLine = (path: string, file: string, block: number, blockId: string, line: number) =>
  invoke<void>("stage_line_cmd", { path, file, block, blockId, line });

/** Throws away a single changed line of a file's unstaged changes (same addressing as `stageLine`). */
export const discardLine = (path: string, file: string, block: number, blockId: string, line: number) =>
  invoke<void>("discard_line_cmd", { path, file, block, blockId, line });
