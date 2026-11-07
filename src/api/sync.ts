import { invoke } from "./invoke";

export interface SyncStatus {
  branch: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  hasRemote: boolean;
}

export const getSyncStatus = (path: string) => invoke<SyncStatus>("get_sync_status", { path });
export const gitFetch = (path: string) => invoke<string>("git_fetch", { path });
export const gitPull = (path: string) => invoke<string>("git_pull", { path });
export const gitPush = (path: string) => invoke<string>("git_push", { path });
/** Overwrites the upstream with the local branch (--force-with-lease). */
export const gitForcePush = (path: string) => invoke<string>("git_force_push", { path });

export interface AutoFetchOutcome {
  /** "ok", "none" (no remotes), "auth" (credentials needed), "offline" or "error". */
  status: "ok" | "none" | "auth" | "offline" | "error";
  message: string;
}

/** Quiet background fetch; never rejects for git failures, reports them in `status`. */
export const autoFetch = (path: string) => invoke<AutoFetchOutcome>("git_auto_fetch", { path });

export interface BriefCommit {
  shortId: string;
  summary: string;
}

/** The two sides of a branch that can't fast-forward. Lists are capped; totals count everything. */
export interface Divergence {
  branch: string;
  upstream: string;
  ahead: BriefCommit[];
  behind: BriefCommit[];
  aheadTotal: number;
  behindTotal: number;
}

export const getDivergence = (path: string) => invoke<Divergence>("get_divergence", { path });

/**
 * Pulls by merging or rebasing. A rebase with conflicts is cancelled and the repository is left as it was; the
 * rejection says which files conflicted. A merge with a clean working directory stops on its conflicts instead and
 * resolves with a message naming them (they are resolved in the conflict view); with uncommitted changes it is cancelled too.
 */
export const gitPullWith = (path: string, mode: "merge" | "rebase") =>
  invoke<string>("git_pull_with", { path, mode });
