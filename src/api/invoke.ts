import { invoke as tauriInvoke, type InvokeArgs } from "@tauri-apps/api/core";

/** The panels that show an activity bar. */
export type Panel = "sidebar" | "graph" | "right";

// Commands that only read, and the panel that shows what they return. Every other command that takes a repository
// `path` and is not in `QUIET` counts as a change to the repository: all panels reload after it, so all are busy.
const READS: Record<string, Panel> = {
  get_graph: "graph",
  get_local_branches: "sidebar",
  get_remotes: "sidebar",
  get_stashes: "sidebar",
  get_status: "right",
  get_head_commit: "right",
  get_commit_detail: "right",
};

// Commands that run in the background or have their own indicator, so they must not light up the panels.
const QUIET = new Set([
  "watch_repo",
  "git_auto_fetch",
  "get_sync_status",
  "get_undo_state",
  "get_divergence",
  "search_commits",
  "get_file_diff",
  "get_working_diff",
  "get_file_history",
  "open_repo",
  "answer_credentials",
]);

const isInfo = (cmd: string) => cmd.startsWith("get_") || cmd.startsWith("count_") || cmd.startsWith("scan_");

interface Running {
  path: string;
  /** null = a change: every panel is busy. */
  panel: Panel | null;
}

const running = new Set<Running>();
const listeners = new Set<() => void>();
let version = 0;

function changed() {
  version++;
  listeners.forEach((l) => l());
}

/** Subscription for `useSyncExternalStore`. */
export const subscribeActivity = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};
export const activityVersion = () => version;

/** Is a command that `panel` shows (or any change to the repository) running for `path` right now? */
export function isPanelActive(path: string, panel: Panel): boolean {
  for (const r of running) if (r.path === path && (r.panel === null || r.panel === panel)) return true;
  return false;
}

/**
 * `invoke` from Tauri, plus a record of which commands are in flight per repository, which is what the panels'
 * activity bars read. Every API wrapper goes through this instead of importing `invoke` itself.
 */
export function invoke<T>(cmd: string, args?: InvokeArgs): Promise<T> {
  const path = (args as { path?: unknown } | undefined)?.path;
  if (typeof path !== "string" || QUIET.has(cmd) || cmd.startsWith("term_")) return tauriInvoke<T>(cmd, args);
  const panel = READS[cmd] ?? (isInfo(cmd) ? undefined : null);
  if (panel === undefined) return tauriInvoke<T>(cmd, args);
  const entry: Running = { path, panel };
  running.add(entry);
  changed();
  return tauriInvoke<T>(cmd, args).finally(() => {
    running.delete(entry);
    changed();
  });
}
