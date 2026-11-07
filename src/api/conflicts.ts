import { invoke } from "./invoke";

/** A stretch of a conflicted file: text both sides agree on, or one conflict with each side's version. */
export type Segment = { kind: "text"; text: string } | { kind: "conflict"; ours: string; theirs: string };

export interface ConflictFile {
  /** Empty when the file can't be merged line by line (`binary`, or deleted on one side). */
  segments: Segment[];
  /** The checked-out branch ("current") and the branch merged in ("incoming"). */
  oursLabel: string;
  theirsLabel: string;
  /** False when that side deleted the file. */
  oursExists: boolean;
  theirsExists: boolean;
  binary: boolean;
}

export interface MergeState {
  /** The message git prepared for the merge commit. */
  message: string;
  /** Files that still have conflicts. */
  conflicts: number;
}

/** The merge in progress (stopped on conflicts, or waiting for its commit), or null. */
export const getMergeState = (path: string) => invoke<MergeState | null>("get_merge_state", { path });

/** A conflicted file split into segments for the conflict view. */
export const getConflict = (path: string, file: string) => invoke<ConflictFile>("get_conflict", { path, file });

/** Resolves a file with one whole side ("ours" = current, "theirs" = incoming) and stages it. */
export const resolveConflictSide = (path: string, file: string, side: "ours" | "theirs") =>
  invoke<void>("resolve_conflict_side_cmd", { path, file, side });

/** Resolves a file with the given content and stages it. */
export const saveResolution = (path: string, file: string, content: string) =>
  invoke<void>("save_resolution_cmd", { path, file, content });

/** Gives up the merge in progress: everything goes back to how it was before it started. */
export const abortMerge = (path: string) => invoke<void>("abort_merge_cmd", { path });
