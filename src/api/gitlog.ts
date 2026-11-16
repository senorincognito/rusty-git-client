import { invoke } from "./invoke";

/** One call of the system `git` (libgit2 operations run no command and are not listed). */
export interface GitLogEntry {
  id: number;
  /** When git was started, ms since the Unix epoch. */
  time: number;
  /** The repository folder. */
  path: string;
  /** The arguments, without `git -C <path>`. */
  args: string[];
  running: boolean;
  ok: boolean;
  durationMs: number;
  /** stdout and stderr, or the error; cut to 2000 characters. */
  output: string;
}

/** The recorded calls, oldest first. New ones arrive as `git-log` events (the same `id` again when one ends). */
export const getGitLog = () => invoke<GitLogEntry[]>("get_git_log");
export const clearGitLog = () => invoke<void>("clear_git_log");
